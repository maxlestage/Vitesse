//! WebSocket (RFC 6455), in the style of the `ws` / `express-ws` packages.
//!
//! The simplest way is [`App::ws`](crate::App::ws) (also available on
//! [`Router`](crate::Router)): the handler receives the request (parameters,
//! headers, state…) and the socket.
//!
//! ```no_run
//! use vitesse::prelude::*;
//! use vitesse::ws::Message;
//!
//! fn main() -> std::io::Result<()> {
//!     let mut app = App::new();
//!
//!     // An echo server: every message is sent back.
//!     app.ws("/echo", |_req, mut socket| async move {
//!         while let Some(Ok(message)) = socket.recv().await {
//!             if let Message::Text(text) = message {
//!                 if socket.send(format!("echo: {text}")).await.is_err() {
//!                     break;
//!                 }
//!             }
//!         }
//!     });
//!
//!     app.run(3000)
//! }
//! ```
//!
//! For more control (subprotocols, message size, checking the `Origin`
//! before accepting), use [`Upgrade`] in a regular `GET` route.
//!
//! Pings are answered automatically. Messages are limited to 16 MiB by
//! default ([`Upgrade::max_message_size`]).

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use http::{HeaderValue, Method, StatusCode, header};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::{Role, WebSocketConfig};

use crate::handler::BoxFuture;
use crate::request::Request;
use crate::response::Response;
use crate::upgrade::{UpgradeHandle, Upgraded};

/// Default maximum size of a message (and of a frame): 16 MiB.
pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

// ----- Messages ------------------------------------------------------------------

/// A WebSocket message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// A text message (valid UTF-8).
    Text(String),
    /// A binary message.
    Binary(Bytes),
    /// A ping (answered automatically with a pong).
    Ping(Bytes),
    /// A pong.
    Pong(Bytes),
    /// The other side closes the connection.
    Close(Option<CloseFrame>),
}

/// The code and reason of a closing handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseFrame {
    /// The close code (`1000` for a normal closure, `1001` when going away…).
    pub code: u16,
    /// A short, human-readable reason.
    pub reason: String,
}

impl Message {
    /// The text of a text message.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Message::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The payload of a text or binary message.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Message::Text(text) => text.as_bytes(),
            Message::Binary(data) | Message::Ping(data) | Message::Pong(data) => data,
            Message::Close(_) => &[],
        }
    }

    /// `true` for [`Message::Close`].
    pub fn is_close(&self) -> bool {
        matches!(self, Message::Close(_))
    }

    fn from_tungstenite(message: tungstenite::Message) -> Message {
        match message {
            tungstenite::Message::Text(text) => Message::Text(text.as_str().to_owned()),
            tungstenite::Message::Binary(data) => Message::Binary(data),
            tungstenite::Message::Ping(data) => Message::Ping(data),
            tungstenite::Message::Pong(data) => Message::Pong(data),
            tungstenite::Message::Close(frame) => Message::Close(frame.map(|f| CloseFrame {
                code: f.code.into(),
                reason: f.reason.as_str().to_owned(),
            })),
            // Jamais renvoyé en lecture ; par prudence, on le traite comme du binaire.
            tungstenite::Message::Frame(frame) => Message::Binary(frame.into_payload()),
        }
    }

    fn into_tungstenite(self) -> tungstenite::Message {
        match self {
            Message::Text(text) => tungstenite::Message::Text(text.into()),
            Message::Binary(data) => tungstenite::Message::Binary(data),
            Message::Ping(data) => tungstenite::Message::Ping(data),
            Message::Pong(data) => tungstenite::Message::Pong(data),
            Message::Close(frame) => {
                tungstenite::Message::Close(frame.map(|f| tungstenite::protocol::CloseFrame {
                    code: f.code.into(),
                    reason: f.reason.into(),
                }))
            }
        }
    }
}

impl From<String> for Message {
    fn from(text: String) -> Self {
        Message::Text(text)
    }
}

impl From<&str> for Message {
    fn from(text: &str) -> Self {
        Message::Text(text.to_owned())
    }
}

impl From<Bytes> for Message {
    fn from(data: Bytes) -> Self {
        Message::Binary(data)
    }
}

impl From<Vec<u8>> for Message {
    fn from(data: Vec<u8>) -> Self {
        Message::Binary(data.into())
    }
}

impl From<&[u8]> for Message {
    fn from(data: &[u8]) -> Self {
        Message::Binary(Bytes::copy_from_slice(data))
    }
}

// ----- Errors ----------------------------------------------------------------------

/// A WebSocket error: connection closed or reset, protocol violation,
/// message too large…
pub struct Error(tungstenite::Error);

impl Error {
    /// The connection is already closed (normally or not).
    pub fn is_closed(&self) -> bool {
        matches!(
            self.0,
            tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed
        )
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

// ----- The socket ----------------------------------------------------------------

/// An open WebSocket connection.
///
/// Read with [`recv`](Self::recv), write with [`send`](Self::send); to read
/// and write from two tasks at once, [`split`](Self::split) it. It also
/// implements [`Stream`] and [`Sink`].
pub struct WebSocket {
    inner: WebSocketStream<TcpStream>,
    protocol: Option<String>,
}

impl fmt::Debug for WebSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebSocket")
            .field("protocol", &self.protocol)
            .finish_non_exhaustive()
    }
}

impl WebSocket {
    /// The next message; `None` once the connection is closed.
    pub async fn recv(&mut self) -> Option<Result<Message, Error>> {
        self.inner
            .next()
            .await
            .map(|m| m.map(Message::from_tungstenite).map_err(Error))
    }

    /// Sends a message: a `String` / `&str` is sent as text, `Vec<u8>` /
    /// `Bytes` as binary.
    pub async fn send(&mut self, message: impl Into<Message>) -> Result<(), Error> {
        self.inner
            .send(message.into().into_tungstenite())
            .await
            .map_err(Error)
    }

    /// Starts the closing handshake with a code (`1000`: normal closure)
    /// and a reason.
    pub async fn close(&mut self, code: u16, reason: &str) -> Result<(), Error> {
        self.send(Message::Close(Some(CloseFrame {
            code,
            reason: reason.to_owned(),
        })))
        .await
    }

    /// The subprotocol chosen with [`Upgrade::protocols`], if any.
    pub fn protocol(&self) -> Option<&str> {
        self.protocol.as_deref()
    }

    /// Splits the socket in two halves, to send from one task while
    /// receiving in another.
    pub fn split(self) -> (WebSocketSender, WebSocketReceiver) {
        let (sink, stream) = self.inner.split();
        (
            WebSocketSender { inner: sink },
            WebSocketReceiver { inner: stream },
        )
    }
}

impl Stream for WebSocket {
    type Item = Result<Message, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner)
            .poll_next(cx)
            .map(|m| m.map(|m| m.map(Message::from_tungstenite).map_err(Error)))
    }
}

impl Sink<Message> for WebSocket {
    type Error = Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.inner).poll_ready(cx).map_err(Error)
    }

    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Error> {
        Pin::new(&mut self.inner)
            .start_send(item.into_tungstenite())
            .map_err(Error)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.inner).poll_flush(cx).map_err(Error)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.inner).poll_close(cx).map_err(Error)
    }
}

/// The sending half of a [`WebSocket`] (see [`WebSocket::split`]).
pub struct WebSocketSender {
    inner: SplitSink<WebSocketStream<TcpStream>, tungstenite::Message>,
}

impl WebSocketSender {
    /// Sends a message.
    pub async fn send(&mut self, message: impl Into<Message>) -> Result<(), Error> {
        self.inner
            .send(message.into().into_tungstenite())
            .await
            .map_err(Error)
    }

    /// Starts the closing handshake.
    pub async fn close(&mut self, code: u16, reason: &str) -> Result<(), Error> {
        self.send(Message::Close(Some(CloseFrame {
            code,
            reason: reason.to_owned(),
        })))
        .await
    }
}

/// The receiving half of a [`WebSocket`] (see [`WebSocket::split`]).
pub struct WebSocketReceiver {
    inner: SplitStream<WebSocketStream<TcpStream>>,
}

impl WebSocketReceiver {
    /// The next message; `None` once the connection is closed.
    pub async fn recv(&mut self) -> Option<Result<Message, Error>> {
        self.inner
            .next()
            .await
            .map(|m| m.map(Message::from_tungstenite).map_err(Error))
    }
}

impl Stream for WebSocketReceiver {
    type Item = Result<Message, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner)
            .poll_next(cx)
            .map(|m| m.map(|m| m.map(Message::from_tungstenite).map_err(Error)))
    }
}

// ----- The handshake ---------------------------------------------------------------

/// A WebSocket handshake to accept, with its options.
///
/// ```no_run
/// use vitesse::prelude::*;
/// use vitesse::ws::Upgrade;
///
/// let mut app = App::new();
/// app.get("/chat", |req: Request| async move {
///     // Refuse connections opened by other websites.
///     if req.header("origin") != Some("https://chat.example.com") {
///         return Err(Error::forbidden("unknown origin"));
///     }
///     let upgrade = Upgrade::new(&req)?
///         .protocols(["chat.v2", "chat.v1"])
///         .max_message_size(64 * 1024);
///     Ok(upgrade.on_upgrade(req, |_req, mut socket| async move {
///         let _ = socket.send("welcome").await;
///     }))
/// });
/// ```
pub struct Upgrade {
    accept: HeaderValue,
    offered: Vec<String>,
    protocol: Option<String>,
    config: WebSocketConfig,
}

impl fmt::Debug for Upgrade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Upgrade")
            .field("offered", &self.offered)
            .field("protocol", &self.protocol)
            .finish_non_exhaustive()
    }
}

/// `name` contains `token` (comma-separated list, case-insensitive).
fn has_token(req: &Request, name: &str, token: &str) -> bool {
    req.header_all(name)
        .any(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case(token)))
}

impl Upgrade {
    /// Checks that `req` is a WebSocket handshake (`GET` with
    /// `Upgrade: websocket`, `Sec-WebSocket-Version: 13` and a valid key).
    ///
    /// Otherwise the error is `426 Upgrade Required` (a plain HTTP request,
    /// or another version of the protocol), `400` (invalid key) or `405`
    /// (not a `GET`).
    pub fn new(req: &Request) -> Result<Upgrade, crate::Error> {
        if *req.method() != Method::GET {
            return Err(crate::Error::new(
                StatusCode::METHOD_NOT_ALLOWED,
                "a WebSocket handshake must use GET",
            ));
        }
        if !has_token(req, "upgrade", "websocket") || !has_token(req, "connection", "upgrade") {
            return Err(crate::Error::new(
                StatusCode::UPGRADE_REQUIRED,
                "this route only accepts WebSocket connections",
            ));
        }
        if req.header("sec-websocket-version").map(str::trim) != Some("13") {
            return Err(crate::Error::new(
                StatusCode::UPGRADE_REQUIRED,
                "unsupported WebSocket version (expected 13)",
            ));
        }
        // La clé : 16 octets aléatoires encodés en base64 (24 caractères).
        let key = req
            .header("sec-websocket-key")
            .map(str::trim)
            .filter(|k| k.len() == 24 && k.ends_with("=="))
            .ok_or_else(|| crate::Error::bad_request("invalid Sec-WebSocket-Key"))?;
        let accept = HeaderValue::from_str(&derive_accept_key(key.as_bytes()))
            .map_err(|_| crate::Error::bad_request("invalid Sec-WebSocket-Key"))?;
        let offered = req
            .header_all("sec-websocket-protocol")
            .flat_map(|v| v.split(','))
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect();
        Ok(Upgrade {
            accept,
            offered,
            protocol: None,
            config: WebSocketConfig::default()
                .max_message_size(Some(DEFAULT_MAX_MESSAGE_SIZE))
                .max_frame_size(Some(DEFAULT_MAX_MESSAGE_SIZE)),
        })
    }

    /// The subprotocols offered by the client (`Sec-WebSocket-Protocol`).
    pub fn offered_protocols(&self) -> &[String] {
        &self.offered
    }

    /// Chooses a subprotocol among those the server supports: the first one
    /// the client offered that is in `supported`. With no match, no
    /// subprotocol is chosen.
    pub fn protocols<I, S>(mut self, supported: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let supported: Vec<S> = supported.into_iter().collect();
        self.protocol = self
            .offered
            .iter()
            .find(|offered| supported.iter().any(|s| s.as_ref() == offered.as_str()))
            .cloned();
        self
    }

    /// Maximum size of a message (and of a frame) received, in bytes
    /// (16 MiB by default). Larger messages close the connection.
    pub fn max_message_size(mut self, bytes: usize) -> Self {
        self.config = self
            .config
            .max_message_size(Some(bytes))
            .max_frame_size(Some(bytes));
        self
    }

    /// Accepts the handshake: returns the `101 Switching Protocols` response,
    /// after which `handler` takes over the connection with the request and
    /// the socket.
    pub fn on_upgrade<F, Fut>(self, req: Request, handler: F) -> Response
    where
        F: FnOnce(Request, WebSocket) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let Upgrade {
            accept,
            protocol,
            config,
            ..
        } = self;
        let mut res = Response::new()
            .status(StatusCode::SWITCHING_PROTOCOLS)
            .header(header::UPGRADE, HeaderValue::from_static("websocket"))
            .header(header::CONNECTION, HeaderValue::from_static("upgrade"))
            .header(header::SEC_WEBSOCKET_ACCEPT, accept);
        if let Some(value) = protocol
            .as_deref()
            .and_then(|p| HeaderValue::from_str(p).ok())
        {
            res = res.header(header::SEC_WEBSOCKET_PROTOCOL, value);
        }
        let handle = UpgradeHandle::new(move |upgraded: Upgraded| -> BoxFuture<()> {
            Box::pin(async move {
                let inner = WebSocketStream::from_partially_read(
                    upgraded.io,
                    upgraded.read,
                    Role::Server,
                    Some(config),
                )
                .await;
                handler(req, WebSocket { inner, protocol }).await;
            })
        });
        res.extensions_mut().insert(handle);
        res
    }
}
