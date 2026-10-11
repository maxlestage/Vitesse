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
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime};

use bytes::{Buf, Bytes};
use h2::server::SendResponse;
use h2::{RecvStream, SendStream};
use http::{HeaderValue, Method, StatusCode, header};
use http_body::{Body as _, Frame, SizeHint};
use http_body_util::BodyExt;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::task::JoinSet;
use tokio::time::Instant;

use crate::app::AppService;
use crate::body::Body;
use crate::http1::{Busy, ServerState};
use crate::request::{ReqBody, Request};
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

/// Serves an HTTP/2 connection until it is closed.
pub(crate) async fn serve<IO: Io>(
    io: IO,
    peer: SocketAddr,
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
    let handshake = builder.handshake::<_, Bytes>(io);
    let Ok(Ok(mut conn)) = tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake).await else {
        return;
    };

    let mut streams = JoinSet::new();
    let mut stopped = pin!(state.stopped());
    // Une connexion sans requête en cours est fermée après le délai de keep-alive.
    let idle_timeout = match app.config.keep_alive {
        d if d.is_zero() => app.config.header_timeout,
        d => d,
    };
    let mut idle = pin!(tokio::time::sleep(idle_timeout));
    // Pendant l'arrêt propre, la connexion compte comme occupée jusqu'à ce
    // que ses dernières réponses soient parties.
    let mut stopping: Option<Busy> = None;
    loop {
        let next = tokio::select! {
            next = conn.accept() => next,
            () = &mut stopped, if stopping.is_none() => {
                // GOAWAY : le client n'ouvrira plus de flux ici.
                stopping = Some(Busy::new(state));
                conn.graceful_shutdown();
                continue;
            }
            () = &mut idle, if stopping.is_none() => {
                while streams.try_join_next().is_some() {}
                if streams.is_empty() {
                    stopping = Some(Busy::new(state));
                    conn.graceful_shutdown();
                } else {
                    idle.as_mut().reset(Instant::now() + idle_timeout);
                }
                continue;
            }
        };
        match next {
            Some(Ok((req, respond))) => {
                idle.as_mut().reset(Instant::now() + idle_timeout);
                streams.spawn(respond_to(req, respond, peer, secure, app, state));
                while streams.try_join_next().is_some() {}
            }
            Some(Err(_)) | None => break,
        }
    }
    drop(stopping);
}

/// Answers one request (one stream).
async fn respond_to(
    req: http::Request<RecvStream>,
    mut respond: SendResponse<Bytes>,
    peer: SocketAddr,
    secure: bool,
    app: &'static AppService,
    state: &'static ServerState,
) {
    let _busy = Busy::new(state);
    let (mut parts, body) = req.into_parts();
    // En HTTP/2, l'hôte est dans `:authority` : on le remet dans `Host` pour
    // que `req.header("host")` fonctionne comme en HTTP/1.1.
    let missing_host = !parts.headers.contains_key(header::HOST);
    if let Some(host) = parts
        .uri
        .authority()
        .filter(|_| missing_host)
        .and_then(|a| HeaderValue::from_str(a.as_str()).ok())
    {
        parts.headers.insert(header::HOST, host);
    }
    let is_head = parts.method == Method::HEAD;
    let body = if body.is_end_stream() {
        ReqBody::Empty
    } else {
        let length = parts
            .headers
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());
        ReqBody::Stream(Body::wrap(IncomingBody {
            stream: body,
            remaining: length,
        }))
    };
    let req = Request::from_parts(parts, body, Some(peer), secure, &app.shared);
    let res = ResponseFuture::new(app, req).await;

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
        // Pas de changement de protocole en HTTP/2 (WebSocket passe par HTTP/1.1).
        head.status = StatusCode::HTTP_VERSION_NOT_SUPPORTED;
    }
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
