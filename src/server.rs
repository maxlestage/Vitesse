//! The server: sockets, accept loop, graceful shutdown and runtimes.

use std::future::{Future, pending};
use std::io;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::{Pin, pin};
use std::task::{Context, Poll};
use std::time::Duration;

use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::TcpListener;
use tokio::task::JoinSet;

use crate::app::AppService;
use crate::handler::{BoxFuture, panic_response};
use crate::http1::{ServerState, serve_connection};
use crate::request::Request;
use crate::response::Response;
use crate::upgrade::Io;

/// The engine's timeouts and limits (see [`App::keep_alive`](crate::App::keep_alive)
/// and the following methods).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ServerConfig {
    /// How long an idle connection is kept open; zero: no keep-alive.
    pub(crate) keep_alive: Duration,
    /// Time given to a client to send a complete request head.
    pub(crate) header_timeout: Duration,
    /// Time given to in-flight requests during a graceful shutdown.
    pub(crate) shutdown_timeout: Duration,
    /// Open connections per accept loop (per thread with `thread_per_core`).
    pub(crate) max_connections: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            keep_alive: Duration::from_secs(60),
            header_timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(10),
            max_connections: usize::MAX,
        }
    }
}

impl ServerConfig {
    /// The period of the HTTP/1.1 inactivity timer, and the number of
    /// periods after which an idle connection, or one whose request head is
    /// incomplete, is closed. A single timer per connection, which is not
    /// reset by each request: the delays are respected to within one period.
    pub(crate) fn ticks(&self) -> (Duration, u32, u32) {
        let shortest = if self.keep_alive.is_zero() {
            self.header_timeout
        } else {
            self.keep_alive.min(self.header_timeout)
        };
        let tick = (shortest / 2).max(Duration::from_millis(10));
        let count = |d: Duration| (d.as_nanos().div_ceil(tick.as_nanos()) as u32).max(1);
        (tick, count(self.keep_alive), count(self.header_timeout))
    }
}

/// An address to listen on: `3000`, `"127.0.0.1:8080"`, `([0, 0, 0, 0], 80)`…
///
/// A bare port number listens on all IPv4 interfaces.
pub trait ListenAddr {
    /// Resolves the address.
    fn socket_addrs(self) -> io::Result<Vec<SocketAddr>>;
}

impl ListenAddr for u16 {
    fn socket_addrs(self) -> io::Result<Vec<SocketAddr>> {
        Ok(vec![SocketAddr::from(([0, 0, 0, 0], self))])
    }
}

impl ListenAddr for i32 {
    fn socket_addrs(self) -> io::Result<Vec<SocketAddr>> {
        u16::try_from(self)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid port"))?
            .socket_addrs()
    }
}

impl ListenAddr for &str {
    fn socket_addrs(self) -> io::Result<Vec<SocketAddr>> {
        if let Ok(port) = self.parse::<u16>() {
            return port.socket_addrs();
        }
        Ok(self.to_socket_addrs()?.collect())
    }
}

impl ListenAddr for String {
    fn socket_addrs(self) -> io::Result<Vec<SocketAddr>> {
        self.as_str().socket_addrs()
    }
}

impl ListenAddr for &String {
    fn socket_addrs(self) -> io::Result<Vec<SocketAddr>> {
        self.as_str().socket_addrs()
    }
}

impl ListenAddr for SocketAddr {
    fn socket_addrs(self) -> io::Result<Vec<SocketAddr>> {
        Ok(vec![self])
    }
}

impl<I: Into<IpAddr>> ListenAddr for (I, u16) {
    fn socket_addrs(self) -> io::Result<Vec<SocketAddr>> {
        Ok(vec![SocketAddr::new(self.0.into(), self.1)])
    }
}

/// Creates a non-blocking listening socket.
fn listener(addr: SocketAddr, reuse_port: bool) -> io::Result<std::net::TcpListener> {
    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    #[cfg(target_os = "linux")]
    if reuse_port {
        socket.set_reuse_port(true)?;
    }
    #[cfg(not(target_os = "linux"))]
    let _ = reuse_port;
    socket.set_tcp_nodelay(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(4096)?;
    Ok(socket.into())
}

/// Opens the first address that works.
pub(crate) fn bind(addrs: &[SocketAddr], reuse_port: bool) -> io::Result<std::net::TcpListener> {
    let mut last = io::Error::new(io::ErrorKind::InvalidInput, "no address to listen on");
    for &addr in addrs {
        match listener(addr, reuse_port) {
            Ok(l) => return Ok(l),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// A server whose port is open, ready to start.
///
/// ```no_run
/// # use vitesse::prelude::*;
/// # async fn demo() -> std::io::Result<()> {
/// let mut app = App::new();
/// app.get("/", |_| async { "ok" });
///
/// let server = app.bind("127.0.0.1:0").await?;
/// println!("listening on http://{}", server.local_addr());
/// server.with_graceful_shutdown(async {
///     tokio::signal::ctrl_c().await.ok();
/// }).await
/// # }
/// ```
pub struct Server {
    app: &'static AppService,
    listener: TcpListener,
    addr: SocketAddr,
    #[cfg(feature = "http3")]
    quic: Option<quinn::Endpoint>,
}

impl Server {
    /// Must be called from a tokio runtime (it may open the HTTP/3 port).
    pub(crate) fn new(app: &'static AppService, listener: TcpListener) -> io::Result<Self> {
        let addr = listener.local_addr()?;
        Ok(Server {
            app,
            listener,
            addr,
            #[cfg(feature = "http3")]
            quic: crate::http3::bind(app, addr)?,
        })
    }

    /// The address actually being listened on.
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// The UDP address of HTTP/3, if [`App::http3`](crate::App::http3) is
    /// configured.
    #[cfg(feature = "http3")]
    pub fn http3_addr(&self) -> Option<SocketAddr> {
        self.quic.as_ref().and_then(|e| e.local_addr().ok())
    }

    /// Serves requests until `Ctrl+C` or `SIGTERM`, then gives in-flight
    /// requests time to finish ([`App::shutdown_timeout`](crate::App::shutdown_timeout),
    /// 10 s by default), like [`App::run`](crate::App::run).
    ///
    /// To stop on another signal, use
    /// [`with_graceful_shutdown`](Self::with_graceful_shutdown).
    pub async fn run(self) -> io::Result<()> {
        self.with_graceful_shutdown(shutdown_signal()).await
    }

    /// Serves requests until `signal` completes, then gives in-flight
    /// requests time to finish ([`App::shutdown_timeout`](crate::App::shutdown_timeout),
    /// 10 s by default).
    pub async fn with_graceful_shutdown<F>(self, signal: F) -> io::Result<()>
    where
        F: Future<Output = ()>,
    {
        #[cfg(feature = "http3")]
        if let Some(endpoint) = self.quic {
            // HTTP/3 tourne à côté et s'arrête sur le même signal.
            let (stop, stopped) = tokio::sync::watch::channel(false);
            let quic = tokio::spawn(crate::http3::serve(endpoint, self.app, stopped));
            let result = serve(self.listener, self.app, async move {
                signal.await;
                let _ = stop.send(true);
            })
            .await;
            let _ = quic.await;
            return result;
        }
        serve(self.listener, self.app, signal).await
    }
}

/// A listening socket: TCP, or a Unix socket.
pub(crate) trait Listener: Send {
    type Io: Io;

    /// The next connection, and the client's address if there is one.
    fn next_connection(
        &self,
    ) -> impl Future<Output = io::Result<(Self::Io, Option<SocketAddr>)>> + Send;
}

impl Listener for TcpListener {
    type Io = tokio::net::TcpStream;

    async fn next_connection(&self) -> io::Result<(Self::Io, Option<SocketAddr>)> {
        let (stream, peer) = self.accept().await?;
        let _ = stream.set_nodelay(true);
        Ok((stream, Some(peer)))
    }
}

#[cfg(unix)]
impl Listener for tokio::net::UnixListener {
    type Io = tokio::net::UnixStream;

    async fn next_connection(&self) -> io::Result<(Self::Io, Option<SocketAddr>)> {
        let (stream, _) = self.accept().await?;
        Ok((stream, None))
    }
}

/// The accept loop.
pub(crate) async fn serve<L: Listener>(
    listener: L,
    app: &'static AppService,
    shutdown: impl Future<Output = ()>,
) -> io::Result<()> {
    // Vit aussi longtemps que les connexions, qui peuvent survivre à la boucle.
    let state: &'static ServerState = Box::leak(Box::default());
    let mut connections = JoinSet::new();
    let mut shutdown = pin!(shutdown);

    loop {
        if connections.len() >= app.config.max_connections {
            // Plein : on attend qu'une connexion se termine avant d'en accepter une autre.
            tokio::select! {
                biased;
                _ = &mut shutdown => break,
                _ = connections.join_next() => continue,
            }
        }
        let (stream, peer) = tokio::select! {
            biased;
            _ = &mut shutdown => break,
            accepted = listener.next_connection() => match accepted {
                Ok(conn) => conn,
                Err(e) => {
                    if !is_transient(&e) {
                        // Ex. trop de fichiers ouverts : on souffle un peu.
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    continue;
                }
            },
        };
        #[cfg(feature = "tls")]
        if let Some(acceptor) = &app.tls {
            connections.spawn(crate::tls::serve(
                acceptor.clone(),
                stream,
                peer,
                app,
                state,
            ));
            while connections.try_join_next().is_some() {}
            continue;
        }
        connections.spawn(serve_connection(stream, peer, false, app, state));
        // Libère les connexions terminées.
        while connections.try_join_next().is_some() {}
    }

    // Arrêt propre : on n'accepte plus rien, on laisse les requêtes en cours
    // se terminer, puis on ferme les connexions inactives.
    drop(listener);
    state.stop();
    let deadline = tokio::time::Instant::now() + app.config.shutdown_timeout;
    while state.busy() > 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    connections.shutdown().await;
    Ok(())
}

fn is_transient(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::Interrupted
            | io::ErrorKind::WouldBlock
    )
}

/// Waits for `Ctrl+C` or `SIGTERM`.
pub(crate) async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

/// Runs the server on its own runtime (see [`App::run`](crate::App::run)).
pub(crate) fn run(
    app: &'static AppService,
    addrs: &[SocketAddr],
    workers: usize,
    thread_per_core: bool,
) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    if thread_per_core {
        return run_thread_per_core(app, addrs, workers);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = thread_per_core;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let listener = TcpListener::from_std(bind(addrs, false)?)?;
        Server::new(app, listener)?.run().await
    })
}

/// One single-threaded runtime per core, each with its own `SO_REUSEPORT`
/// socket: the kernel spreads connections across them, and a request never
/// leaves its thread.
#[cfg(target_os = "linux")]
fn run_thread_per_core(
    app: &'static AppService,
    addrs: &[SocketAddr],
    workers: usize,
) -> io::Result<()> {
    let first = bind(addrs, true)?;
    // On réutilise l'adresse effective (utile avec le port 0).
    let addr = first.local_addr()?;
    // Le thread principal surveille les signaux, et sert HTTP/3 s'il est
    // configuré (un seul socket UDP).
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    #[cfg(feature = "http3")]
    let quic = {
        let _context = runtime.enter();
        crate::http3::bind(app, addr)?
    };
    let mut listeners = vec![first];
    for _ in 1..workers {
        listeners.push(listener(addr, true)?);
    }

    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let mut threads = Vec::with_capacity(workers);
    for (i, std_listener) in listeners.into_iter().enumerate() {
        let mut stop = stop_rx.clone();
        let thread = std::thread::Builder::new()
            .name(format!("vitesse-{i}"))
            .spawn(move || -> io::Result<()> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                runtime.block_on(async move {
                    let listener = TcpListener::from_std(std_listener)?;
                    serve(listener, app, async move {
                        let _ = stop.wait_for(|stopped| *stopped).await;
                    })
                    .await
                })
            })?;
        threads.push(thread);
    }

    runtime.block_on(async {
        #[cfg(feature = "http3")]
        let quic = quic.map(|endpoint| tokio::spawn(crate::http3::serve(endpoint, app, stop_rx)));
        #[cfg(not(feature = "http3"))]
        drop(stop_rx);
        shutdown_signal().await;
        let _ = stop_tx.send(true);
        #[cfg(feature = "http3")]
        if let Some(quic) = quic {
            let _ = quic.await;
        }
    });

    let mut result = Ok(());
    for thread in threads {
        match thread.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => result = Err(e),
            Err(_) => result = Err(io::Error::other("a server thread panicked")),
        }
    }
    result
}

/// A server listening on a Unix domain socket, created by
/// [`App::bind_unix`](crate::App::bind_unix): the usual way to sit behind
/// a reverse proxy on the same machine (Nginx's `proxy_pass
/// http://unix:/run/my-app.sock`), without a TCP port.
///
/// Requests on a Unix socket have no client address: [`Request::ip`]
/// returns `None` unless [`App::trust_proxy`](crate::App::trust_proxy) is
/// set (the proxy then reports it in `X-Forwarded-For`).
#[cfg(unix)]
pub struct UnixServer {
    app: &'static AppService,
    listener: tokio::net::UnixListener,
    path: std::path::PathBuf,
}

#[cfg(unix)]
impl UnixServer {
    pub(crate) fn new(
        app: &'static AppService,
        listener: tokio::net::UnixListener,
        path: std::path::PathBuf,
    ) -> Self {
        UnixServer {
            app,
            listener,
            path,
        }
    }

    /// The path of the socket (to change its permissions, for example).
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Serves requests until `Ctrl+C` or `SIGTERM`, then shuts down
    /// gracefully and removes the socket file.
    pub async fn run(self) -> io::Result<()> {
        self.with_graceful_shutdown(shutdown_signal()).await
    }

    /// Serves requests until `signal` completes, then shuts down gracefully
    /// and removes the socket file.
    pub async fn with_graceful_shutdown<F>(self, signal: F) -> io::Result<()>
    where
        F: Future<Output = ()>,
    {
        let result = serve(self.listener, self.app, signal).await;
        let _ = std::fs::remove_file(&self.path);
        result
    }
}

/// Opens a Unix socket at `path`, replacing a socket file left over by a
/// previous run (but not a live one, nor any other kind of file).
#[cfg(unix)]
pub(crate) fn bind_unix(path: &std::path::Path) -> io::Result<tokio::net::UnixListener> {
    use std::os::unix::fs::FileTypeExt;

    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.file_type().is_socket() {
            if std::os::unix::net::UnixStream::connect(path).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!("{} is used by a running server", path.display()),
                ));
            }
            std::fs::remove_file(path)?;
        }
    }
    tokio::net::UnixListener::bind(path)
}

/// The `Future` of a response, which turns a panic into a `500`.
pub(crate) enum ResponseFuture {
    Pending(BoxFuture<Response>),
    Ready(Option<Box<Response>>),
}

impl ResponseFuture {
    #[inline]
    pub(crate) fn new(app: &'static AppService, req: Request) -> Self {
        match catch_unwind(AssertUnwindSafe(|| app.handle(req))) {
            Ok(fut) => ResponseFuture::Pending(fut),
            Err(_) => ResponseFuture::Ready(Some(Box::new(panic_response()))),
        }
    }
}

impl Future for ResponseFuture {
    type Output = Response;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Response> {
        match self.get_mut() {
            ResponseFuture::Pending(fut) => {
                match catch_unwind(AssertUnwindSafe(|| fut.as_mut().poll(cx))) {
                    Ok(poll) => poll,
                    Err(_) => Poll::Ready(panic_response()),
                }
            }
            ResponseFuture::Ready(res) => {
                Poll::Ready(res.take().map_or_else(panic_response, |res| *res))
            }
        }
    }
}
