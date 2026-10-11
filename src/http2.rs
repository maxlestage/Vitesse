//! HTTP/2, with the [h2](https://crates.io/crates/h2) crate (the one hyper
//! uses): over TLS when the client picks `h2` through ALPN (see
//! [`tls`](crate::tls)), and in clear text when the client opens the
//! connection with the HTTP/2 preface ("prior knowledge", h2c: gRPC
//! clients, proxies such as Envoy or Nginx's `grpc_pass`,
//! `curl --http2-prior-knowledge`).
//!
//! Each request is a stream of the connection, handled in its own task: the
//! same routes, middlewares and handlers as HTTP/1.1, with `req.version()`
//! set to `HTTP/2.0`.

use std::future::poll_fn;
use std::io;
use std::net::SocketAddr;
use std::pin::{Pin, pin};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime};

use bytes::{Buf, Bytes};
use h2::server::SendResponse;
use h2::{RecvStream, SendStream};
use http::{HeaderValue, Method, StatusCode, header};
use http_body::{Body as _, Frame, SizeHint};
use http_body_util::BodyExt;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::Notify;
use tokio::task::JoinSet;
use tokio::time::Instant;

use crate::app::AppService;
use crate::body::Body;
use crate::http1::{Busy, ServerState};
use crate::request::{ReqBody, Request};
use crate::response::Response;
use crate::server::ResponseFuture;
use crate::upgrade::Io;
use crate::util::http_date;

/// Requests a client may have in flight at once on a connection.
const MAX_STREAMS: u32 = 256;
/// Flow-control windows: how much of a request body a client may send
/// before the handler reads it.
const STREAM_WINDOW: u32 = 256 * 1024;
const CONNECTION_WINDOW: u32 = 1024 * 1024;
/// Maximum size of the headers of a request (like HTTP/1.1).
const MAX_HEADER_LIST: u32 = 64 * 1024;
/// Time given to the client to send its preface and settings.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Time given to a client to acknowledge the `GOAWAY` of an idle
/// connection, before it is closed anyway.
const GOAWAY_TIMEOUT: Duration = Duration::from_secs(5);

/// Serves an HTTP/2 connection until it is closed.
pub(crate) async fn serve<IO: Io>(
    io: IO,
    peer: Option<SocketAddr>,
    secure: bool,
    app: &'static AppService,
    state: &'static ServerState,
) {
    let mut builder = h2::server::Builder::new();
    builder
        .max_concurrent_streams(MAX_STREAMS)
        .initial_window_size(STREAM_WINDOW)
        .initial_connection_window_size(CONNECTION_WINDOW)
        .max_header_list_size(MAX_HEADER_LIST);
    // WebSocket sur HTTP/2 (RFC 8441) : les navigateurs l'utilisent quand le
    // serveur l'annonce.
    #[cfg(feature = "ws")]
    builder.enable_connect_protocol();
    let handshake = builder.handshake::<_, Bytes>(io);
    let Ok(Ok(mut conn)) = tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake).await else {
        return;
    };

    let mut streams = JoinSet::new();
    let requests = Arc::new(Requests::default());
    let mut stopped = pin!(state.stopped());
    // Une connexion sans requête en cours est fermée après le délai de keep-alive.
    let idle_timeout = match app.config.keep_alive {
        d if d.is_zero() => app.config.header_timeout,
        d => d,
    };
    let mut idle = pin!(tokio::time::sleep(idle_timeout));
    // Après le GOAWAY, la connexion est fermée de force passé ce délai (un
    // client qui ne répond plus ne la garde pas ouverte).
    let mut close_by = pin!(tokio::time::sleep(Duration::ZERO));
    let mut closing = false;
    // Pendant l'arrêt propre, la connexion compte comme occupée tant que des
    // requêtes HTTP sont en cours (pas les WebSockets), et un instant de plus
    // pour que leurs dernières trames partent.
    let mut hold: Option<Busy> = None;
    let mut release = pin!(tokio::time::sleep(Duration::ZERO));
    let mut releasing = false;
    loop {
        let next = tokio::select! {
            next = conn.accept() => next,
            () = &mut stopped, if !closing => {
                // GOAWAY : le client n'ouvrira plus de flux ici.
                closing = true;
                conn.graceful_shutdown();
                if requests.active() > 0 {
                    hold = Some(Busy::new(state));
                }
                close_by.as_mut().reset(Instant::now() + app.config.shutdown_timeout);
                continue;
            }
            () = &mut idle, if !closing => {
                while streams.try_join_next().is_some() {}
                if streams.is_empty() {
                    closing = true;
                    conn.graceful_shutdown();
                    close_by.as_mut().reset(Instant::now() + GOAWAY_TIMEOUT);
                } else {
                    idle.as_mut().reset(Instant::now() + idle_timeout);
                }
                continue;
            }
            () = requests.done.notified(), if hold.is_some() && !releasing => {
                if requests.active() == 0 {
                    releasing = true;
                    release.as_mut().reset(Instant::now() + Duration::from_millis(50));
                }
                continue;
            }
            () = &mut release, if releasing => {
                hold = None;
                releasing = false;
                continue;
            }
            () = &mut close_by, if closing => break,
        };
        match next {
            Some(Ok((req, respond))) => {
                idle.as_mut().reset(Instant::now() + idle_timeout);
                let guard = RequestGuard::new(&requests, state);
                streams.spawn(respond_to(req, respond, peer, secure, app, guard));
                while streams.try_join_next().is_some() {}
            }
            Some(Err(_)) | None => break,
        }
    }
    drop(hold);
}

/// The HTTP requests in flight on a connection (WebSockets excluded).
#[derive(Default)]
struct Requests {
    active: AtomicUsize,
    /// Signaled when the last request in flight finishes.
    done: Notify,
}

impl Requests {
    fn active(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }
}

/// A request in flight: counted by its connection and by the server (for
/// the graceful shutdown) until it is dropped.
struct RequestGuard {
    requests: Arc<Requests>,
    _busy: Busy,
}

impl RequestGuard {
    fn new(requests: &Arc<Requests>, state: &'static ServerState) -> Self {
        requests.active.fetch_add(1, Ordering::AcqRel);
        RequestGuard {
            requests: requests.clone(),
            _busy: Busy::new(state),
        }
    }
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        if self.requests.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.requests.done.notify_one();
        }
    }
}

/// Answers one request (one stream).
async fn respond_to(
    req: http::Request<RecvStream>,
    respond: SendResponse<Bytes>,
    peer: Option<SocketAddr>,
    secure: bool,
    app: &'static AppService,
    busy: RequestGuard,
) {
    #[cfg_attr(not(feature = "ws"), allow(unused_mut))]
    let (mut parts, body) = req.into_parts();
    let is_head = parts.method == Method::HEAD;
    // Une poignée de main WebSocket (CONNECT étendu, `:protocol websocket`)
    // devient un `GET` pour le routage ; le flux servira ensuite au WebSocket.
    #[cfg_attr(not(feature = "ws"), allow(unused_mut))]
    let mut tunnel = None;
    #[cfg_attr(not(feature = "ws"), allow(unused_mut))]
    let mut body = Some(body);
    #[cfg(feature = "ws")]
    if parts.method == Method::CONNECT
        && parts
            .extensions
            .get::<h2::ext::Protocol>()
            .is_some_and(|p| p.as_str().eq_ignore_ascii_case("websocket"))
    {
        parts.method = Method::GET;
        parts.extensions.insert(crate::upgrade::ExtendedConnect);
        tunnel = body.take();
    }
    let body = match body {
        Some(body) if !body.is_end_stream() => {
            let length = parts
                .headers
                .get(header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok());
            ReqBody::Stream(Body::wrap(IncomingBody {
                stream: body,
                remaining: length,
            }))
        }
        _ => ReqBody::Empty,
    };
    let req = Request::from_parts(parts, body, peer, secure, &app.shared);
    let res = ResponseFuture::new(app, req).await;
    respond_with(res, respond, is_head, app, busy, tunnel).await;
}

/// Sends the response of a stream (or, for an accepted WebSocket
/// handshake, opens the WebSocket on it).
async fn respond_with(
    res: Response,
    mut respond: SendResponse<Bytes>,
    is_head: bool,
    #[cfg_attr(not(feature = "http3"), allow(unused_variables))] app: &'static AppService,
    busy: RequestGuard,
    tunnel: Option<RecvStream>,
) {
    let (mut head, body) = res.into_http().into_parts();
    // En-têtes propres à une connexion HTTP/1.1, interdits en HTTP/2.
    for name in [
        header::CONNECTION,
        header::TRANSFER_ENCODING,
        header::UPGRADE,
        header::HeaderName::from_static("keep-alive"),
        header::HeaderName::from_static("proxy-connection"),
    ] {
        head.headers.remove(name);
    }
    if head.status == StatusCode::SWITCHING_PROTOCOLS {
        let upgrade = head
            .extensions
            .get::<crate::upgrade::UpgradeHandle>()
            .and_then(crate::upgrade::UpgradeHandle::take);
        match (upgrade, tunnel) {
            // Poignée de main WebSocket acceptée : `200`, puis le WebSocket
            // sur ce flux.
            (Some(on_upgrade), Some(recv)) => {
                head.status = StatusCode::OK;
                head.headers.remove(header::SEC_WEBSOCKET_ACCEPT);
                let Ok(send) = respond.send_response(http::Response::from_parts(head, ()), false)
                else {
                    return;
                };
                // Le WebSocket ne retarde pas l'arrêt propre, comme en HTTP/1.1.
                drop(busy);
                let io = H2Io {
                    send,
                    recv,
                    pending: Bytes::new(),
                };
                on_upgrade(crate::upgrade::Upgraded {
                    io: Box::new(io),
                    read: Vec::new(),
                })
                .await;
                return;
            }
            // Pas d'autre changement de protocole en HTTP/2.
            _ => head.status = StatusCode::HTTP_VERSION_NOT_SUPPORTED,
        }
    }
    drop(busy);
    if let Ok(date) = HeaderValue::from_str(&http_date(SystemTime::now())) {
        head.headers.insert(header::DATE, date);
    }
    #[cfg(feature = "http3")]
    if let Some(value) = app
        .alt_svc
        .get()
        .and_then(|line| line.strip_prefix(b"alt-svc: "))
        .and_then(|line| line.strip_suffix(b"\r\n"))
        .and_then(|value| HeaderValue::from_bytes(value).ok())
    {
        head.headers.insert(header::ALT_SVC, value);
    }
    let bodyless = head.status.is_informational()
        || head.status == StatusCode::NO_CONTENT
        || head.status == StatusCode::NOT_MODIFIED;
    let needs_length = !bodyless && !head.headers.contains_key(header::CONTENT_LENGTH);
    if let Some(len) = body.size_hint().exact().filter(|_| needs_length) {
        head.headers
            .insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    }
    let end = is_head || bodyless || body.is_end_stream();
    let Ok(mut send) = respond.send_response(http::Response::from_parts(head, ()), end) else {
        return;
    };
    if end {
        return;
    }
    let mut body = body;
    while let Some(frame) = body.frame().await {
        let Ok(frame) = frame else {
            send.send_reset(h2::Reason::INTERNAL_ERROR);
            return;
        };
        if let Ok(data) = frame.into_data() {
            if send_data(&mut send, data).await.is_err() {
                return;
            }
        }
    }
    let _ = send.send_data(Bytes::new(), true);
}

/// Sends a chunk of the body, as fast as the client's flow-control window
/// allows.
async fn send_data(send: &mut SendStream<Bytes>, mut data: Bytes) -> Result<(), h2::Error> {
    while !data.is_empty() {
        send.reserve_capacity(data.len());
        let capacity = poll_fn(|cx| send.poll_capacity(cx))
            .await
            .ok_or_else(|| h2::Error::from(h2::Reason::CANCEL))??;
        if capacity > 0 {
            let chunk = data.split_to(capacity.min(data.len()));
            send.send_data(chunk, false)?;
        }
    }
    Ok(())
}

/// The body of a request, as seen by the handler.
struct IncomingBody {
    stream: RecvStream,
    remaining: Option<u64>,
}

impl http_body::Body for IncomingBody {
    type Data = Bytes;
    type Error = h2::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, h2::Error>>> {
        let this = &mut *self;
        match std::task::ready!(this.stream.poll_data(cx)) {
            Some(Ok(data)) => {
                // Le handler a lu ces octets : le client peut en envoyer d'autres.
                let _ = this.stream.flow_control().release_capacity(data.len());
                if let Some(remaining) = &mut this.remaining {
                    *remaining = remaining.saturating_sub(data.len() as u64);
                }
                Poll::Ready(Some(Ok(Frame::data(data))))
            }
            Some(Err(e)) => Poll::Ready(Some(Err(e))),
            None => Poll::Ready(None),
        }
    }

    fn is_end_stream(&self) -> bool {
        self.stream.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        match self.remaining {
            Some(n) => SizeHint::with_exact(n),
            None => SizeHint::default(),
        }
    }
}

/// A WebSocket over an HTTP/2 stream (RFC 8441): the DATA frames carry the
/// WebSocket frames in both directions.
struct H2Io {
    send: SendStream<Bytes>,
    recv: RecvStream,
    /// Received bytes not read yet.
    pending: Bytes,
}

impl AsyncRead for H2Io {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = &mut *self;
        if this.pending.is_empty() {
            match std::task::ready!(this.recv.poll_data(cx)) {
                Some(Ok(data)) => {
                    let _ = this.recv.flow_control().release_capacity(data.len());
                    this.pending = data;
                }
                Some(Err(e)) => return Poll::Ready(Err(h2_to_io(e))),
                // Fin du flux : fin de lecture.
                None => return Poll::Ready(Ok(())),
            }
        }
        let n = this.pending.len().min(buf.remaining());
        buf.put_slice(&this.pending[..n]);
        this.pending.advance(n);
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for H2Io {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let send = &mut self.send;
        loop {
            let capacity = send.capacity();
            if capacity > 0 {
                let n = capacity.min(buf.len());
                send.send_data(Bytes::copy_from_slice(&buf[..n]), false)
                    .map_err(h2_to_io)?;
                return Poll::Ready(Ok(n));
            }
            send.reserve_capacity(buf.len());
            match std::task::ready!(send.poll_capacity(cx)) {
                Some(Ok(_)) => continue,
                Some(Err(e)) => return Poll::Ready(Err(h2_to_io(e))),
                None => return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // La connexion HTTP/2 envoie les trames d'elle-même.
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let _ = self.send.send_data(Bytes::new(), true);
        Poll::Ready(Ok(()))
    }
}

fn h2_to_io(e: h2::Error) -> io::Error {
    if e.is_io() {
        e.into_io().unwrap_or_else(|| io::ErrorKind::Other.into())
    } else {
        io::Error::other(e)
    }
}

/// A connection whose first bytes have already been read (by the HTTP/1.1
/// engine, which spotted the preface): they are read again first.
pub(crate) struct Rewind<IO> {
    pre: Bytes,
    io: IO,
}

impl<IO> Rewind<IO> {
    pub(crate) fn new(pre: Bytes, io: IO) -> Self {
        Rewind { pre, io }
    }
}

impl<IO: AsyncRead + Unpin> AsyncRead for Rewind<IO> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if !self.pre.is_empty() {
            let n = self.pre.len().min(buf.remaining());
            buf.put_slice(&self.pre[..n]);
            self.pre.advance(n);
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.io).poll_read(cx, buf)
    }
}

impl<IO: AsyncWrite + Unpin> AsyncWrite for Rewind<IO> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.io.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }
}
