//! HTTP/3 over QUIC: the same application, served over UDP next to
//! HTTP/1.1, with [quinn](https://crates.io/crates/quinn) and
//! [h3](https://crates.io/crates/h3) (TLS 1.3 by rustls, all in Rust).
//!
//! ```no_run
//! use vitesse::prelude::*;
//! use vitesse::http3::Http3;
//!
//! fn main() -> std::io::Result<()> {
//!     let mut app = App::new();
//!     app.get("/", |_| async { "Hello over HTTP/1.1 and HTTP/3!" });
//!
//!     // QUIC needs TLS: a certificate and its private key, in PEM.
//!     app.http3(Http3::from_pem_files("cert.pem", "key.pem")?);
//!
//!     // TCP 443 (HTTP/1.1) and UDP 443 (HTTP/3).
//!     app.run(443)
//! }
//! ```
//!
//! Browsers discover HTTP/3 through the `Alt-Svc` header: Vitesse adds
//! `alt-svc: h3=":PORT"; ma=86400` to its HTTP/1.1 and HTTP/2 responses.
//! They only trust it on an HTTPS page: serve HTTPS with
//! [`App::tls`](crate::App::tls) (`tls` feature), or, behind a reverse
//! proxy that handles TLS for TCP (Caddy, Nginx…), set the public port with
//! [`Http3::alt_svc_port`] and open the UDP port to Vitesse.
//!
//! The request body is streamed to the handler as it arrives, like over
//! HTTP/1.1 and HTTP/2, within [`Request::body_limit`](crate::Request::body_limit).

use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::SystemTime;

use bytes::{Buf, Bytes};
use h3::server::RequestStream;
use http::{HeaderValue, Method, StatusCode, header};
use http_body::{Body as _, Frame, SizeHint};
use http_body_util::BodyExt;
use tokio::sync::watch;
use tokio::task::JoinSet;

pub use rustls;

use crate::app::AppService;
use crate::body::{Body, BoxError};
use crate::pem::invalid;
use crate::request::{ReqBody, Request};
use crate::server::ResponseFuture;
use crate::util::http_date;

/// How long `Alt-Svc` remains valid in the browser: one day.
const ALT_SVC_MAX_AGE: u32 = 86_400;

/// The HTTP/3 configuration of an application (see [`App::http3`](crate::App::http3)).
#[derive(Clone)]
pub struct Http3 {
    tls: Arc<rustls::ServerConfig>,
    port: Option<u16>,
    alt_svc: bool,
    alt_svc_port: Option<u16>,
}

impl std::fmt::Debug for Http3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Http3")
            .field("port", &self.port)
            .field("alt_svc", &self.alt_svc)
            .field("alt_svc_port", &self.alt_svc_port)
            .finish_non_exhaustive()
    }
}

impl Http3 {
    /// From a certificate chain and a private key in PEM (the server
    /// certificate first, then the intermediates).
    pub fn from_pem(cert_chain: &[u8], private_key: &[u8]) -> io::Result<Self> {
        let (certs, key) = crate::pem::load(cert_chain, private_key)?;
        let tls = rustls::ServerConfig::builder_with_provider(crate::pem::provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(invalid)?
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(invalid)?;
        Ok(Self::from_rustls(tls))
    }

    /// From PEM files (`fullchain.pem` and `privkey.pem` with Let's Encrypt).
    pub fn from_pem_files(
        cert_chain: impl AsRef<Path>,
        private_key: impl AsRef<Path>,
    ) -> io::Result<Self> {
        Self::from_pem(&std::fs::read(cert_chain)?, &std::fs::read(private_key)?)
    }

    /// From a rustls configuration of your own (TLS 1.3 only; the `h3`
    /// ALPN protocol is added if missing). `rustls` is re-exported as
    /// [`vitesse::http3::rustls`](rustls).
    pub fn from_rustls(mut config: rustls::ServerConfig) -> Self {
        if !config.alpn_protocols.iter().any(|p| p == b"h3") {
            config.alpn_protocols.push(b"h3".to_vec());
        }
        Http3 {
            tls: Arc::new(config),
            port: None,
            alt_svc: true,
            alt_svc_port: None,
        }
    }

    /// The UDP port to listen on (by default, the same as the TCP port).
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Adds `Alt-Svc` to HTTP/1.1 responses so that browsers switch to
    /// HTTP/3 (enabled by default).
    pub fn alt_svc(mut self, enabled: bool) -> Self {
        self.alt_svc = enabled;
        self
    }

    /// The port announced in `Alt-Svc`, when clients reach the server on
    /// another port than the one it listens on (behind a proxy or NAT: `443`).
    pub fn alt_svc_port(mut self, port: u16) -> Self {
        self.alt_svc_port = Some(port);
        self
    }
}

/// Opens the UDP socket if the application has an HTTP/3 configuration.
/// Must be called from a tokio runtime.
pub(crate) fn bind(
    app: &'static AppService,
    tcp: SocketAddr,
) -> io::Result<Option<quinn::Endpoint>> {
    let Some(config) = &app.http3 else {
        return Ok(None);
    };
    let addr = SocketAddr::new(tcp.ip(), config.port.unwrap_or(tcp.port()));
    let crypto =
        quinn::crypto::rustls::QuicServerConfig::try_from(config.tls.clone()).map_err(invalid)?;
    let server = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    let endpoint = quinn::Endpoint::server(server, addr)?;
    if config.alt_svc {
        let port = config.alt_svc_port.unwrap_or(endpoint.local_addr()?.port());
        let line = format!("alt-svc: h3=\":{port}\"; ma={ALT_SVC_MAX_AGE}\r\n");
        let _ = app.alt_svc.set(line.into_bytes().into_boxed_slice());
    }
    Ok(Some(endpoint))
}

/// Accepts QUIC connections until `stop` turns `true`, then lets the
/// requests in progress finish.
pub(crate) async fn serve(
    endpoint: quinn::Endpoint,
    app: &'static AppService,
    mut stop: watch::Receiver<bool>,
) {
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            incoming = endpoint.accept() => match incoming {
                Some(incoming) => {
                    connections.spawn(connection(incoming, app, stop.clone()));
                    while connections.try_join_next().is_some() {}
                }
                None => break,
            },
            () = stopped(&mut stop) => break,
        }
    }
    // Plus de nouvelles connexions ; les connexions ouvertes ont reçu un
    // GOAWAY et se terminent avec leurs requêtes en cours.
    endpoint.set_server_config(None);
    let grace = app.config.shutdown_timeout;
    let _ = tokio::time::timeout(grace, async {
        while connections.join_next().await.is_some() {}
    })
    .await;
    endpoint.close(0u32.into(), b"server shutting down");
    let _ = tokio::time::timeout(grace, endpoint.wait_idle()).await;
}

/// Waits until the server is stopping (the guard of `watch` is released
/// before returning, so that the future stays `Send`).
async fn stopped(stop: &mut watch::Receiver<bool>) {
    let _ = stop.wait_for(|stopped| *stopped).await;
}

async fn connection(
    incoming: quinn::Incoming,
    app: &'static AppService,
    mut stop: watch::Receiver<bool>,
) {
    let Ok(conn) = incoming.await else {
        return;
    };
    let peer = conn.remote_address();
    let Ok(mut h3) = h3::server::Connection::<_, Bytes>::new(h3_quinn::Connection::new(conn)).await
    else {
        return;
    };
    let mut requests = JoinSet::new();
    let mut stopping = false;
    loop {
        let accepted = if stopping {
            h3.accept().await
        } else {
            tokio::select! {
                accepted = h3.accept() => accepted,
                () = stopped(&mut stop) => {
                    // GOAWAY : le client n'ouvrira plus de requêtes ici.
                    stopping = true;
                    let _ = h3.shutdown(0).await;
                    continue;
                }
            }
        };
        match accepted {
            Ok(Some(resolver)) => {
                requests.spawn(async move {
                    if let Ok((req, stream)) = resolver.resolve_request().await {
                        respond(app, req, stream, peer).await;
                    }
                });
                while requests.try_join_next().is_some() {}
            }
            Ok(None) | Err(_) => break,
        }
    }
    while requests.join_next().await.is_some() {}
}

/// The body of a request, read from the QUIC stream as the handler asks
/// for it.
struct IncomingBody<S: h3::quic::RecvStream> {
    stream: RequestStream<S, Bytes>,
    remaining: Option<u64>,
    done: bool,
}

impl<S: h3::quic::RecvStream> http_body::Body for IncomingBody<S> {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, BoxError>>> {
        if self.done {
            return Poll::Ready(None);
        }
        match std::task::ready!(self.stream.poll_recv_data(cx)) {
            Ok(Some(mut chunk)) => {
                let data = chunk.copy_to_bytes(chunk.remaining());
                if let Some(remaining) = &mut self.remaining {
                    *remaining = remaining.saturating_sub(data.len() as u64);
                }
                Poll::Ready(Some(Ok(Frame::data(data))))
            }
            Ok(None) => {
                self.done = true;
                Poll::Ready(None)
            }
            Err(e) => {
                self.done = true;
                Poll::Ready(Some(Err(Box::new(e))))
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.done
    }

    fn size_hint(&self) -> SizeHint {
        match self.remaining {
            Some(n) => SizeHint::with_exact(n),
            None => SizeHint::default(),
        }
    }
}

async fn respond<S>(
    app: &'static AppService,
    req: http::Request<()>,
    stream: RequestStream<S, Bytes>,
    peer: SocketAddr,
) where
    S: h3::quic::BidiStream<Bytes>,
    S::RecvStream: Send + 'static,
{
    let (parts, ()) = req.into_parts();
    let is_head = parts.method == Method::HEAD;

    // Le corps arrive au fil de l'eau : le handler le lit (dans la limite
    // de sa route) pendant que la réponse part sur l'autre moitié du flux.
    let (mut stream, recv) = stream.split();
    let length = parts
        .headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let body = ReqBody::Stream(Body::wrap(IncomingBody {
        stream: recv,
        remaining: length,
        done: false,
    }));
    let req = Request::from_parts(parts, body, Some(peer), true, &app.shared);
    let res = ResponseFuture::new(app, req).await;

    let (mut head, body) = res.into_http().into_parts();
    // En-têtes propres à une connexion HTTP/1.1, interdits en HTTP/3.
    for name in [
        header::CONNECTION,
        header::TRANSFER_ENCODING,
        header::UPGRADE,
        header::HeaderName::from_static("keep-alive"),
        header::HeaderName::from_static("proxy-connection"),
    ] {
        head.headers.remove(name);
    }
    if let Ok(date) = HeaderValue::from_str(&http_date(SystemTime::now())) {
        head.headers.insert(header::DATE, date);
    }
    let bodyless = head.status.is_informational()
        || head.status == StatusCode::NO_CONTENT
        || head.status == StatusCode::NOT_MODIFIED;
    let needs_length = !bodyless && !head.headers.contains_key(header::CONTENT_LENGTH);
    if let Some(len) = body.size_hint().exact().filter(|_| needs_length) {
        head.headers
            .insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    }
    if stream
        .send_response(http::Response::from_parts(head, ()))
        .await
        .is_err()
    {
        return;
    }
    if !is_head && !bodyless {
        let mut body = body;
        while let Some(frame) = body.frame().await {
            let Ok(frame) = frame else {
                return;
            };
            match frame.into_data() {
                Ok(data) if !data.is_empty() => {
                    if stream.send_data(data).await.is_err() {
                        return;
                    }
                }
                _ => {}
            }
        }
    }
    let _ = stream.finish().await;
}
