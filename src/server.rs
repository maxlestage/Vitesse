//! The server: sockets, accept loop, graceful shutdown and runtimes.

use std::future::{Future, pending};
use std::io;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::{Pin, pin};
use std::sync::atomic::Ordering;
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

/// Time given to in-flight requests during a graceful shutdown.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

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
}

impl Server {
    pub(crate) fn new(app: &'static AppService, listener: TcpListener) -> io::Result<Self> {
        let addr = listener.local_addr()?;
        Ok(Server {
            app,
            listener,
            addr,
        })
    }

    /// The address actually being listened on.
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Serves requests forever.
    pub async fn run(self) -> io::Result<()> {
        serve(self.listener, self.app, pending()).await
    }

    /// Serves requests until `signal` completes, then gives in-flight
    /// requests time to finish (10 s at most).
    pub async fn with_graceful_shutdown<F>(self, signal: F) -> io::Result<()>
    where
        F: Future<Output = ()>,
    {
        serve(self.listener, self.app, signal).await
    }
}

/// The accept loop.
pub(crate) async fn serve(
    listener: TcpListener,
    app: &'static AppService,
    shutdown: impl Future<Output = ()>,
) -> io::Result<()> {
    // Vit aussi longtemps que les connexions, qui peuvent survivre à la boucle.
    let state: &'static ServerState = Box::leak(Box::default());
    let mut connections = JoinSet::new();
    let mut shutdown = pin!(shutdown);

    loop {
        let (stream, peer) = tokio::select! {
            biased;
            _ = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
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
        let _ = stream.set_nodelay(true);
        connections.spawn(serve_connection(stream, peer, app, state));
        // Libère les connexions terminées.
        while connections.try_join_next().is_some() {}
    }

    // Arrêt propre : on n'accepte plus rien, on laisse les requêtes en cours
    // se terminer, puis on ferme les connexions inactives.
    drop(listener);
    state.shutdown.store(true, Ordering::Relaxed);
    let deadline = tokio::time::Instant::now() + SHUTDOWN_GRACE;
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
        serve(listener, app, shutdown_signal()).await
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

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(shutdown_signal());
    let _ = stop_tx.send(true);

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
