//! Publish/subscribe inside the application (Phoenix's `PubSub` and
//! channels, Socket.IO's rooms, Laravel's broadcasting): topics, or rooms,
//! that any part of the application publishes to, and that WebSocket
//! connections, Server-Sent Events streams or tasks subscribe to.
//!
//! A [`PubSub`] is cheap to clone (all the clones share the topics): put it
//! in the [state](crate::App::state) of the application. A topic exists as
//! long as it has subscribers, and costs nothing once they are gone.
//!
//! ```no_run
//! use vitesse::prelude::*;
//! use vitesse::pubsub::PubSub;
//!
//! let mut app = App::new();
//! app.state(PubSub::<String>::new());
//!
//! // A chat room per URL: every message is sent to everyone in the room.
//! # #[cfg(feature = "ws")]
//! app.ws("/rooms/:room", |req, socket| async move {
//!     let pubsub = req.state::<PubSub>();
//!     let room = format!("room:{}", req.param("room").unwrap_or("lobby"));
//!     pubsub.relay(socket, &room).await;
//! });
//!
//! // The same room, read-only, as Server-Sent Events.
//! app.get("/rooms/:room/events", |req: Request| async move {
//!     let room = format!("room:{}", req.param("room").unwrap_or("lobby"));
//!     req.state::<PubSub>()
//!         .subscribe(&room)
//!         .sse(|message| sse::Event::new().data(message))
//! });
//!
//! // Anything can publish: a route, a background task, a webhook…
//! app.post("/rooms/:room/announce", |req: Request| async move {
//!     let room = format!("room:{}", req.param("room").unwrap_or("lobby"));
//!     let text = req.text().await?;
//!     let reached = req.state::<PubSub>().publish(&room, text);
//!     Ok::<_, Error>(format!("sent to {reached} subscribers"))
//! });
//! # app.run(3000).unwrap();
//! ```
//!
//! Messages are kept in memory, in the process: with several instances of
//! the application behind a load balancer, each one has its own topics.
//! Messages of any `Clone + Send` type work: `String` by default, or
//! `serde_json::Value`, `Bytes`, an enum of your events…

use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll};

use futures_core::Stream;
use tokio::sync::broadcast;

use crate::handler::BoxFuture;
use crate::sse::{Event, Sse};

/// The number of messages that can wait for a slow subscriber by default:
/// beyond it, it misses the oldest.
pub const DEFAULT_CAPACITY: usize = 256;

/// A message, with the subscription that sent it (`0`: none).
#[derive(Clone)]
struct Envelope<T> {
    from: u64,
    message: T,
}

struct Inner<T> {
    topics: Mutex<HashMap<String, broadcast::Sender<Envelope<T>>>>,
    capacity: usize,
    next_id: AtomicU64,
}

impl<T> Inner<T> {
    fn topics(&self) -> MutexGuard<'_, HashMap<String, broadcast::Sender<Envelope<T>>>> {
        self.topics.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Topics that subscribers listen to and that anyone can publish to. See
/// the [module](self).
pub struct PubSub<T = String> {
    inner: Arc<Inner<T>>,
}

impl<T> Clone for PubSub<T> {
    fn clone(&self) -> Self {
        PubSub {
            inner: self.inner.clone(),
        }
    }
}

impl<T> fmt::Debug for PubSub<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PubSub")
            .field("topics", &self.inner.topics().len())
            .field("capacity", &self.inner.capacity)
            .finish()
    }
}

impl<T: Clone + Send + 'static> Default for PubSub<T> {
    fn default() -> Self {
        PubSub::new()
    }
}

impl<T: Clone + Send + 'static> PubSub<T> {
    /// No topic yet; [`DEFAULT_CAPACITY`] messages can wait for each
    /// subscriber.
    pub fn new() -> Self {
        PubSub::with_capacity(DEFAULT_CAPACITY)
    }

    /// `capacity` messages can wait for each subscriber: a subscriber that
    /// falls further behind misses the oldest ones.
    pub fn with_capacity(capacity: usize) -> Self {
        PubSub {
            inner: Arc::new(Inner {
                topics: Mutex::new(HashMap::new()),
                capacity: capacity.max(1),
                next_id: AtomicU64::new(1),
            }),
        }
    }

    /// Subscribes to `topic`: the [`Subscription`] receives every message
    /// published to it from now on, until it is dropped.
    pub fn subscribe(&self, topic: &str) -> Subscription<T> {
        let rx = {
            let mut topics = self.inner.topics();
            match topics.get(topic) {
                Some(tx) => tx.subscribe(),
                None => {
                    let (tx, rx) = broadcast::channel(self.inner.capacity);
                    topics.insert(topic.to_owned(), tx);
                    rx
                }
            }
        };
        Subscription {
            id: self.inner.next_id.fetch_add(1, Ordering::Relaxed),
            topic: topic.into(),
            rx,
            pubsub: self.inner.clone(),
        }
    }

    /// Sends `message` to every subscriber of `topic`; returns how many
    /// there are (`0`: nobody listens, the message is dropped).
    pub fn publish(&self, topic: &str, message: impl Into<T>) -> usize {
        self.send(topic, 0, message.into())
    }

    fn send(&self, topic: &str, from: u64, message: T) -> usize {
        let topics = self.inner.topics();
        match topics.get(topic) {
            Some(tx) => {
                let subscribers = tx.receiver_count();
                let _ = tx.send(Envelope { from, message });
                subscribers
            }
            None => 0,
        }
    }

    /// The number of subscribers of `topic` (Phoenix's presence count).
    pub fn subscribers(&self, topic: &str) -> usize {
        self.inner
            .topics()
            .get(topic)
            .map_or(0, broadcast::Sender::receiver_count)
    }

    /// The topics that have subscribers, sorted.
    pub fn topics(&self) -> Vec<String> {
        let mut topics: Vec<String> = self.inner.topics().keys().cloned().collect();
        topics.sort();
        topics
    }
}

/// A subscription to a topic (see [`PubSub::subscribe`]): read the messages
/// with [`recv`](Self::recv), as a [`Stream`], or as Server-Sent Events
/// with [`sse`](Self::sse). Dropping it unsubscribes.
pub struct Subscription<T = String> {
    id: u64,
    topic: Box<str>,
    rx: broadcast::Receiver<Envelope<T>>,
    pubsub: Arc<Inner<T>>,
}

impl<T> fmt::Debug for Subscription<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Subscription")
            .field("id", &self.id)
            .field("topic", &self.topic)
            .finish()
    }
}

impl<T: Clone + Send + 'static> Subscription<T> {
    /// The topic.
    pub fn topic(&self) -> &str {
        &self.topic
    }

    /// A number that identifies this subscription.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The next message of the topic, except those this subscription
    /// published itself with [`publish`](Self::publish). A subscriber too
    /// slow to keep up misses the oldest messages and goes on with the
    /// next ones.
    pub async fn recv(&mut self) -> Option<T> {
        loop {
            match self.rx.recv().await {
                Ok(envelope) if envelope.from == self.id => {}
                Ok(envelope) => return Some(envelope.message),
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }

    /// Sends `message` to the **other** subscribers of the topic (Phoenix's
    /// `broadcast_from`); returns how many there are.
    pub fn publish(&self, message: impl Into<T>) -> usize {
        let pubsub = PubSub {
            inner: self.pubsub.clone(),
        };
        pubsub
            .send(&self.topic, self.id, message.into())
            .saturating_sub(1)
    }

    /// The messages as a [`Stream`].
    pub fn into_stream(self) -> Messages<T> {
        Messages {
            subscription: Some(self),
            pending: None,
        }
    }

    /// The messages as a Server-Sent Events response, each one turned into
    /// an [`Event`] by `event`.
    ///
    /// ```
    /// use vitesse::prelude::*;
    /// use vitesse::pubsub::PubSub;
    ///
    /// let mut app = App::new();
    /// app.state(PubSub::<String>::new());
    /// app.get("/news", |req: Request| async move {
    ///     req.state::<PubSub>()
    ///         .subscribe("news")
    ///         .sse(|title| sse::Event::new().event("news").data(title))
    /// });
    /// ```
    pub fn sse<F>(
        self,
        event: F,
    ) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static>
    where
        F: FnMut(T) -> Event + Send + Unpin + 'static,
    {
        Sse::new(EventStream {
            messages: self.into_stream(),
            event,
        })
    }
}

impl<T> Drop for Subscription<T> {
    fn drop(&mut self) {
        let mut topics = self.pubsub.topics();
        // The last subscriber leaves: the topic goes away.
        if topics
            .get(&*self.topic)
            .is_some_and(|tx| tx.receiver_count() <= 1)
        {
            topics.remove(&*self.topic);
        }
    }
}

/// The messages of a [`Subscription`], as a [`Stream`] (see
/// [`Subscription::into_stream`]).
pub struct Messages<T = String> {
    subscription: Option<Subscription<T>>,
    pending: Option<BoxFuture<(Option<T>, Subscription<T>)>>,
}

impl<T> fmt::Debug for Messages<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Messages").finish_non_exhaustive()
    }
}

impl<T: Clone + Send + 'static> Stream for Messages<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let this = &mut *self;
        if this.pending.is_none() {
            let Some(mut subscription) = this.subscription.take() else {
                return Poll::Ready(None);
            };
            this.pending = Some(Box::pin(async move {
                let message = subscription.recv().await;
                (message, subscription)
            }));
        }
        let Some(pending) = this.pending.as_mut() else {
            return Poll::Ready(None);
        };
        match pending.as_mut().poll(cx) {
            Poll::Ready((message, subscription)) => {
                this.pending = None;
                if message.is_some() {
                    this.subscription = Some(subscription);
                }
                Poll::Ready(message)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// The messages turned into Server-Sent Events.
struct EventStream<T, F> {
    messages: Messages<T>,
    event: F,
}

impl<T, F> Stream for EventStream<T, F>
where
    T: Clone + Send + 'static,
    F: FnMut(T) -> Event + Unpin,
{
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        match Pin::new(&mut this.messages).poll_next(cx) {
            Poll::Ready(Some(message)) => Poll::Ready(Some(Ok((this.event)(message)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(feature = "ws")]
impl PubSub<String> {
    /// Connects a WebSocket to `topic` (a Phoenix channel, a Socket.IO
    /// room): every text message the socket sends is published to the
    /// topic, and every message of the topic, its own included, is sent to
    /// the socket, until it closes. Publish before or after it to announce
    /// arrivals and departures.
    ///
    /// ```no_run
    /// use vitesse::prelude::*;
    /// use vitesse::pubsub::PubSub;
    ///
    /// let mut app = App::new();
    /// app.state(PubSub::<String>::new());
    /// app.ws("/chat/:room", |req, socket| async move {
    ///     let pubsub = req.state::<PubSub>();
    ///     let room = req.param("room").unwrap_or("lobby").to_owned();
    ///     pubsub.publish(&room, "someone joined");
    ///     pubsub.relay(socket, &room).await;
    ///     pubsub.publish(&room, "someone left");
    /// });
    /// ```
    pub async fn relay(&self, mut socket: crate::ws::WebSocket, topic: &str) {
        use crate::ws::Message;
        let mut subscription = self.subscribe(topic);
        loop {
            tokio::select! {
                incoming = socket.recv() => match incoming {
                    Some(Ok(Message::Text(text))) => {
                        self.publish(topic, text);
                    }
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                    Some(Ok(_)) => {}
                },
                outgoing = subscription.recv() => match outgoing {
                    Some(message) => {
                        if socket.send(message).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_and_subscribe() {
        let pubsub = PubSub::<String>::new();
        assert_eq!(pubsub.publish("t", "nobody"), 0);
        let mut a = pubsub.subscribe("t");
        let mut b = pubsub.subscribe("t");
        assert_eq!(pubsub.subscribers("t"), 2);
        assert_eq!(pubsub.topics(), ["t"]);
        assert_eq!(pubsub.publish("t", "hello"), 2);
        assert_eq!(a.recv().await.as_deref(), Some("hello"));
        assert_eq!(b.recv().await.as_deref(), Some("hello"));
        // `broadcast_from`: a does not get its own message.
        assert_eq!(a.publish("from a"), 1);
        assert_eq!(b.recv().await.as_deref(), Some("from a"));
        pubsub.publish("t", "again");
        assert_eq!(a.recv().await.as_deref(), Some("again"));
        drop(a);
        assert_eq!(pubsub.subscribers("t"), 1);
        drop(b);
        assert!(pubsub.topics().is_empty());
    }

    #[tokio::test]
    async fn slow_subscribers_skip_ahead() {
        let pubsub = PubSub::<u32>::with_capacity(2);
        let mut sub = pubsub.subscribe("n");
        for n in 0..5u32 {
            pubsub.publish("n", n);
        }
        assert_eq!(sub.recv().await, Some(3));
        assert_eq!(sub.recv().await, Some(4));
    }

    #[tokio::test]
    async fn stream() {
        let pubsub = PubSub::<String>::new();
        let mut messages = pubsub.subscribe("s").into_stream();
        pubsub.publish("s", "one");
        let next = std::future::poll_fn(|cx| Pin::new(&mut messages).poll_next(cx)).await;
        assert_eq!(next.as_deref(), Some("one"));
    }
}
