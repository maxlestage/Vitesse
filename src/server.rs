//! Le serveur HTTP : sockets, boucle d'acceptation et intégration hyper.

use std::convert::Infallible;
use std::future::{Future, pending};
use std::io;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::{Pin, pin};
use std::task::{Context, Poll};
use std::time::Duration;

use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::server::graceful::GracefulShutdown;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::TcpListener;

use crate::app::AppService;
use crate::body::Body;
use crate::handler::{BoxFuture, panic_response};
use crate::request::{ReqBody, Request};
use crate::response::Response;

/// Délai laissé aux requêtes en cours lors d'un arrêt propre.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

/// Une adresse d'écoute : `3000`, `"127.0.0.1:8080"`, `([0, 0, 0, 0], 80)`…
///
/// Un simple numéro de port écoute sur toutes les interfaces IPv4.
pub trait ListenAddr {
    /// Résout l'adresse.
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
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "port invalide"))?
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

/// Crée un socket d'écoute non bloquant.
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

/// Ouvre la première adresse qui fonctionne.
pub(crate) fn bind(addrs: &[SocketAddr], reuse_port: bool) -> io::Result<std::net::TcpListener> {
    let mut last = io::Error::new(io::ErrorKind::InvalidInput, "aucune adresse d'écoute");
    for &addr in addrs {
        match listener(addr, reuse_port) {
            Ok(l) => return Ok(l),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Un serveur dont le port est ouvert, prêt à démarrer.
///
/// ```no_run
/// # use vitesse::prelude::*;
/// # async fn demo() -> std::io::Result<()> {
/// let mut app = App::new();
/// app.get("/", |_| async { "ok" });
///
/// let server = app.bind("127.0.0.1:0").await?;
/// println!("écoute sur http://{}", server.local_addr());
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

    /// L'adresse réellement écoutée.
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Sert les requêtes indéfiniment.
    pub async fn run(self) -> io::Result<()> {
        serve(self.listener, self.app, pending()).await
    }

    /// Sert les requêtes jusqu'à ce que `signal` se termine, puis laisse aux
    /// requêtes en cours le temps de finir (10 s maximum).
    pub async fn with_graceful_shutdown<F>(self, signal: F) -> io::Result<()>
    where
        F: Future<Output = ()>,
    {
        serve(self.listener, self.app, signal).await
    }
}

/// La boucle d'acceptation.
pub(crate) async fn serve(
    listener: TcpListener,
    app: &'static AppService,
    shutdown: impl Future<Output = ()>,
) -> io::Result<()> {
    let mut builder = http1::Builder::new();
    builder
        .timer(TokioTimer::new())
        .header_read_timeout(Duration::from_secs(30))
        .keep_alive(true)
        // Regroupe les réponses des requêtes « pipelinées » en un seul write.
        .pipeline_flush(true);

    let graceful = GracefulShutdown::new();
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
        let conn = builder.serve_connection(TokioIo::new(stream), ConnService { app, peer });
        let conn = graceful.watch(conn);
        tokio::spawn(async move {
            let _ = conn.await;
        });
    }

    drop(listener);
    tokio::select! {
        _ = graceful.shutdown() => {}
        _ = tokio::time::sleep(SHUTDOWN_GRACE) => {}
    }
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

/// Attend `Ctrl+C` ou `SIGTERM`.
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

/// Lance le serveur avec son propre runtime (voir [`App::run`](crate::App::run)).
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

/// Un runtime mono-thread par cœur, chacun avec son socket `SO_REUSEPORT` :
/// le noyau répartit les connexions, et une requête ne quitte jamais son
/// thread.
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
            Err(_) => result = Err(io::Error::other("un thread du serveur a paniqué")),
        }
    }
    result
}

/// Le service hyper d'une connexion.
#[derive(Clone, Copy)]
struct ConnService {
    app: &'static AppService,
    peer: SocketAddr,
}

impl hyper::service::Service<http::Request<Incoming>> for ConnService {
    type Response = http::Response<Body>;
    type Error = Infallible;
    type Future = ResponseFuture;

    #[inline]
    fn call(&self, req: http::Request<Incoming>) -> ResponseFuture {
        let (head, body) = req.into_parts();
        let req = Request::new(
            head,
            ReqBody::Incoming(body),
            Some(self.peer),
            &self.app.shared,
        );
        ResponseFuture::new(self.app, req)
    }
}

/// Le `Future` d'une réponse, qui transforme une panique en `500`.
pub(crate) enum ResponseFuture {
    Pending(BoxFuture<Response>),
    Ready(Option<Response>),
}

impl ResponseFuture {
    #[inline]
    pub(crate) fn new(app: &'static AppService, req: Request) -> Self {
        match catch_unwind(AssertUnwindSafe(|| app.handle(req))) {
            Ok(fut) => ResponseFuture::Pending(fut),
            Err(_) => ResponseFuture::Ready(Some(panic_response())),
        }
    }
}

impl Future for ResponseFuture {
    type Output = Result<http::Response<Body>, Infallible>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.get_mut() {
            ResponseFuture::Pending(fut) => {
                match catch_unwind(AssertUnwindSafe(|| fut.as_mut().poll(cx))) {
                    Ok(Poll::Ready(res)) => Poll::Ready(Ok(res.into_http())),
                    Ok(Poll::Pending) => Poll::Pending,
                    Err(_) => Poll::Ready(Ok(panic_response().into_http())),
                }
            }
            ResponseFuture::Ready(res) => {
                Poll::Ready(Ok(res.take().unwrap_or_else(panic_response).into_http()))
            }
        }
    }
}
