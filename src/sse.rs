//! Server-Sent Events: a response that stays open and pushes events to the
//! browser (`EventSource` in JavaScript), like axum's `Sse`.
//!
//! The simplest way is a channel: the handler returns the response right
//! away, and a task sends the events.
//!
//! ```no_run
//! use std::time::Duration;
//! use vitesse::prelude::*;
//! use vitesse::sse::{self, Event};
//!
//! let mut app = App::new();
//! app.get("/clock", |_| async {
//!     let (tx, stream) = sse::channel(16);
//!     tokio::spawn(async move {
//!         for i in 0.. {
//!             let event = Event::new().event("tick").data(format!("{i}"));
//!             // `Err` once the browser has gone away.
//!             if tx.send(event).await.is_err() {
//!                 break;
//!             }
//!             tokio::time::sleep(Duration::from_secs(1)).await;
//!         }
//!     });
//!     stream
//! });
//! ```
//!
//! Any [`Stream`] of `Result<Event, E>` works too, with [`Sse::new`]. Both
//! send a comment every 15 s by default ([`KeepAlive`]), so that proxies do
//! not close a quiet connection.

use std::convert::Infallible;
use std::fmt::{self, Write as _};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use futures_core::Stream;
use http::header::{self, HeaderValue};
use http_body::{Frame, SizeHint};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio::time::{Instant, Sleep};

use crate::body::{Body, BoxError};
use crate::error::Error;
use crate::response::{IntoResponse, Response};

/// A Server-Sent Events response, built from a stream of events.
///
/// ```
/// use std::convert::Infallible;
/// use vitesse::prelude::*;
/// use vitesse::sse::{Event, KeepAlive, Sse};
///
/// async fn countdown(_req: Request) -> impl IntoResponse {
///     let events = (1..=3).rev().map(|n| Ok::<_, Infallible>(Event::new().data(n.to_string())));
///     Sse::new(futures_util::stream::iter(events)).keep_alive(KeepAlive::new())
/// }
/// ```
#[must_use]
pub struct Sse<S> {
    stream: S,
    keep_alive: Option<KeepAlive>,
}

impl<S> fmt::Debug for Sse<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sse")
            .field("keep_alive", &self.keep_alive)
            .finish_non_exhaustive()
    }
}

impl<S, E> Sse<S>
where
    S: Stream<Item = Result<Event, E>> + Send + 'static,
    E: Into<BoxError>,
{
    /// A response that sends the events of `stream`, then ends with it.
    /// Keep-alive comments are on ([`KeepAlive::new`]).
    pub fn new(stream: S) -> Self {
        Sse {
            stream,
            keep_alive: Some(KeepAlive::new()),
        }
    }

    /// Changes the keep-alive comments.
    pub fn keep_alive(mut self, keep_alive: KeepAlive) -> Self {
        self.keep_alive = Some(keep_alive);
        self
    }

    /// Sends no keep-alive comments.
    pub fn no_keep_alive(mut self) -> Self {
        self.keep_alive = None;
        self
    }
}

impl<S, E> IntoResponse for Sse<S>
where
    S: Stream<Item = Result<Event, E>> + Send + 'static,
    E: Into<BoxError>,
{
    fn into_response(self) -> Response {
        let body = SseBody {
            stream: Box::pin(self.stream),
            keep_alive: self.keep_alive.map(|k| {
                let sleep = Box::pin(tokio::time::sleep(k.interval));
                (k, sleep)
            }),
        };
        Response::new()
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            )
            .header(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))
            .send(Body::wrap(body))
    }
}

/// An event: `data`, and optionally an `event` type, an `id`, a `retry`
/// delay and comments.
///
/// ```
/// use vitesse::sse::Event;
///
/// let event = Event::new().event("message").id("42").data("line 1\nline 2");
/// assert_eq!(
///     event.to_string(),
///     "event: message\nid: 42\ndata: line 1\ndata: line 2\n\n"
/// );
/// ```
#[derive(Clone, Default)]
pub struct Event {
    text: String,
}

impl Event {
    /// An empty event.
    pub fn new() -> Self {
        Event::default()
    }

    /// The data of the event (`event.data` in the browser). Can be called
    /// several times: the browser joins the parts with line breaks.
    pub fn data(mut self, data: impl AsRef<str>) -> Self {
        let data = data.as_ref();
        let data = data
            .strip_suffix("\r\n")
            .or_else(|| data.strip_suffix(['\n', '\r']))
            .unwrap_or(data);
        for line in lines(data) {
            self.field("data", line);
        }
        self
    }

    /// The data of the event, as JSON (`JSON.parse(event.data)` in the
    /// browser).
    pub fn json_data<T: Serialize>(self, value: &T) -> Result<Self, Error> {
        let json = serde_json::to_string(value)
            .map_err(|e| Error::internal("JSON serialization failed").with_source(e))?;
        Ok(self.data(json))
    }

    /// The type of the event (`addEventListener(type, …)` in the browser;
    /// `message` by default).
    ///
    /// # Panics
    /// If it contains a line break.
    pub fn event(mut self, event: impl AsRef<str>) -> Self {
        self.field("event", single_line("event", event.as_ref()));
        self
    }

    /// The id of the event: the browser sends the last one back in
    /// `Last-Event-ID` when it reconnects.
    ///
    /// # Panics
    /// If it contains a line break or a NUL character.
    pub fn id(mut self, id: impl AsRef<str>) -> Self {
        let id = single_line("id", id.as_ref());
        assert!(
            !id.contains('\0'),
            "an SSE id cannot contain a NUL character"
        );
        self.field("id", id);
        self
    }

    /// How long the browser waits before reconnecting after a disconnection.
    pub fn retry(mut self, delay: Duration) -> Self {
        let _ = writeln!(self.text, "retry: {}", delay.as_millis());
        self
    }

    /// A comment, ignored by the browser.
    pub fn comment(mut self, comment: impl AsRef<str>) -> Self {
        for line in lines(comment.as_ref()) {
            self.field("", line);
        }
        self
    }

    fn field(&mut self, name: &str, value: &str) {
        self.text.push_str(name);
        self.text.push(':');
        if !value.is_empty() {
            self.text.push(' ');
            self.text.push_str(value);
        }
        self.text.push('\n');
    }

    fn into_bytes(self) -> Bytes {
        let mut text = self.text;
        text.push('\n');
        Bytes::from(text)
    }
}

/// The lines of a text: the browser ends a line at `\r\n`, `\n` or `\r`
/// alike, so all three must start a new `data:` line (or a client's text
/// could add fields of its own).
fn lines(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = Some(text);
    std::iter::from_fn(move || {
        let text = rest?;
        match text.find(['\r', '\n']) {
            Some(i) => {
                let skip = if text[i..].starts_with("\r\n") { 2 } else { 1 };
                rest = Some(&text[i + skip..]);
                Some(&text[..i])
            }
            None => {
                rest = None;
                Some(text)
            }
        }
    })
}

fn single_line<'a>(field: &str, value: &'a str) -> &'a str {
    assert!(
        !value.contains(['\r', '\n']),
        "an SSE {field} cannot contain a line break"
    );
    value
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)?;
        f.write_str("\n")
    }
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Event").field(&self.text).finish()
    }
}

/// Comments sent on a quiet connection, so that proxies and browsers keep
/// it open.
#[derive(Debug, Clone)]
pub struct KeepAlive {
    interval: Duration,
    comment: Bytes,
}

impl Default for KeepAlive {
    fn default() -> Self {
        KeepAlive::new()
    }
}

impl KeepAlive {
    /// A `:` comment after 15 s without any event.
    pub fn new() -> Self {
        KeepAlive {
            interval: Duration::from_secs(15),
            comment: Bytes::from_static(b":\n\n"),
        }
    }

    /// The delay without any event after which a comment is sent.
    pub fn interval(mut self, interval: Duration) -> Self {
        self.interval = interval.min(crate::util::MAX_DELAY);
        self
    }

    /// The text of the comment.
    pub fn text(mut self, text: impl AsRef<str>) -> Self {
        self.comment = Event::new().comment(text).into_bytes();
        self
    }
}

/// The body of an [`Sse`] response.
struct SseBody<S> {
    stream: Pin<Box<S>>,
    keep_alive: Option<(KeepAlive, Pin<Box<Sleep>>)>,
}

impl<S, E> http_body::Body for SseBody<S>
where
    S: Stream<Item = Result<Event, E>>,
    E: Into<BoxError>,
{
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, BoxError>>> {
        let this = self.get_mut();
        match this.stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(event))) => {
                if let Some((keep_alive, sleep)) = &mut this.keep_alive {
                    sleep.as_mut().reset(Instant::now() + keep_alive.interval);
                }
                Poll::Ready(Some(Ok(Frame::data(event.into_bytes()))))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e.into()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => match &mut this.keep_alive {
                Some((keep_alive, sleep)) => {
                    std::task::ready!(sleep.as_mut().poll(cx));
                    sleep.as_mut().reset(Instant::now() + keep_alive.interval);
                    Poll::Ready(Some(Ok(Frame::data(keep_alive.comment.clone()))))
                }
                None => Poll::Pending,
            },
        }
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::default()
    }
}

/// Creates a channel whose events are sent to the browser: return the
/// [`Sse`] response from the handler, and send events from another task with
/// the [`Sender`]. `buffer` events can wait while the connection is slow.
///
/// The response ends when every `Sender` is dropped; `send` fails once the
/// browser has gone away.
pub fn channel(buffer: usize) -> (Sender, Sse<Receiver>) {
    let (tx, rx) = mpsc::channel(buffer.max(1));
    (Sender { tx }, Sse::new(Receiver { rx }))
}

/// Sends events to an [`Sse`] response (see [`channel`]).
#[derive(Debug, Clone)]
pub struct Sender {
    tx: mpsc::Sender<Event>,
}

/// The browser has gone away: the event was not sent.
#[derive(Debug)]
pub struct Closed(pub Event);

impl fmt::Display for Closed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the SSE connection is closed")
    }
}

impl std::error::Error for Closed {}

/// Why [`Sender::try_send`] did not send an event (given back).
#[derive(Debug)]
pub enum TrySendError {
    /// `buffer` events are already waiting: the connection is slow.
    Full(Event),
    /// The browser has gone away.
    Closed(Event),
}

impl fmt::Display for TrySendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrySendError::Full(_) => f.write_str("the SSE buffer is full"),
            TrySendError::Closed(_) => f.write_str("the SSE connection is closed"),
        }
    }
}

impl std::error::Error for TrySendError {}

impl Sender {
    /// Sends an event, waiting if `buffer` events are already waiting.
    pub async fn send(&self, event: Event) -> Result<(), Closed> {
        self.tx.send(event).await.map_err(|e| Closed(e.0))
    }

    /// Sends an event without waiting; fails if `buffer` events are already
    /// waiting (a slow connection) or if the browser has gone away.
    pub fn try_send(&self, event: Event) -> Result<(), TrySendError> {
        self.tx.try_send(event).map_err(|e| match e {
            mpsc::error::TrySendError::Full(e) => TrySendError::Full(e),
            mpsc::error::TrySendError::Closed(e) => TrySendError::Closed(e),
        })
    }

    /// Whether the browser has gone away.
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    /// Completes when the browser has gone away.
    pub async fn closed(&self) {
        self.tx.closed().await;
    }
}

/// The events of a [`channel`], as a [`Stream`].
#[derive(Debug)]
pub struct Receiver {
    rx: mpsc::Receiver<Event>,
}

impl Stream for Receiver {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx).map(|e| e.map(Ok))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_format() {
        assert_eq!(Event::new().data("hello").to_string(), "data: hello\n\n");
        assert_eq!(
            Event::new().data("a\nb").to_string(),
            "data: a\ndata: b\n\n"
        );
        assert_eq!(
            Event::new().data("a\r\nb\n").to_string(),
            "data: a\ndata: b\n\n"
        );
        assert_eq!(Event::new().data("").to_string(), "data:\n\n");
        assert_eq!(
            Event::new().data("a\n\nb").to_string(),
            "data: a\ndata:\ndata: b\n\n"
        );
        // Un `\r` seul est aussi une fin de ligne pour le navigateur.
        assert_eq!(
            Event::new().data("hi\revent: admin\rid: 9").to_string(),
            "data: hi\ndata: event: admin\ndata: id: 9\n\n"
        );
        assert_eq!(Event::new().comment("a\rb").to_string(), ": a\n: b\n\n");
        assert_eq!(
            Event::new()
                .comment("hi")
                .retry(Duration::from_secs(3))
                .data("x")
                .to_string(),
            ": hi\nretry: 3000\ndata: x\n\n"
        );
        assert_eq!(
            Event::new().json_data(&[1, 2]).unwrap().to_string(),
            "data: [1,2]\n\n"
        );
        assert_eq!(&KeepAlive::new().text("ping").comment[..], b": ping\n\n");
    }

    #[test]
    #[should_panic(expected = "line break")]
    fn event_names_are_single_lines() {
        let _ = Event::new().event("a\nb");
    }
}
