# PubSub and channels

`vitesse::pubsub` publishes messages to topics, or rooms, that any part of the application can send to, and that WebSocket connections, Server-Sent Events streams and background tasks subscribe to: Phoenix's `PubSub` and channels, Socket.IO's rooms, Laravel's broadcasting, built in.

## Creating a PubSub

A `PubSub<T>` carries messages of type `T` (`String` by default; `serde_json::Value`, `Bytes` or an enum of your events work too). It is cheap to clone, and all its clones share the topics: put it in the [shared state](state.md) of the application.

```rust
use vitesse::pubsub::PubSub;

let mut app = App::new();
app.state(PubSub::<String>::new());
```

A topic is just a name (`"room:rust"`, `"user:42"`, `"news"`). It exists as long as it has subscribers, and costs nothing once they are gone.

## Publishing and subscribing

```rust
let pubsub = PubSub::<String>::new();

let mut subscription = pubsub.subscribe("news");
let reached = pubsub.publish("news", "Vitesse 1.0 is out"); // 1 subscriber
let message = subscription.recv().await; // Some("Vitesse 1.0 is out")
```

- `publish(topic, message)` returns the number of subscribers reached; with none, the message is dropped.
- `subscription.recv()` waits for the next message. A subscriber too slow to keep up misses the oldest messages, then goes on with the next ones: 256 messages can wait for each subscriber (`PubSub::with_capacity(n)` to change it).
- `subscription.publish(message)` sends to the **other** subscribers of the topic, not to itself (Phoenix's `broadcast_from`).
- `pubsub.subscribers(topic)` counts the subscribers (who is in the room), and `pubsub.topics()` lists the topics.
- Dropping the subscription unsubscribes.

## Chat rooms over WebSocket

`pubsub.relay(socket, topic)` connects a WebSocket to a topic: every text message the socket sends is published to the topic, and every message of the topic is sent to the socket, its own included, until it closes. A complete chat with rooms takes a few lines:

```rust
app.ws("/rooms/:room", |req, socket| async move {
    let pubsub = req.state::<PubSub>();
    let room = format!("room:{}", req.param("room").unwrap_or("lobby"));
    pubsub.publish(&room, "someone joined");
    pubsub.relay(socket, &room).await;
    pubsub.publish(&room, "someone left");
});
```

For more control (JSON messages, authentication, several topics per connection), [split the socket](websocket.md) and combine `socket.recv()` with `subscription.recv()` in a `tokio::select!`.

## Live updates with Server-Sent Events

`subscription.sse(…)` turns a subscription into a [Server-Sent Events](responses.md) response, each message becoming an event. The subscription ends when the browser goes away:

```rust
use vitesse::sse::Event;

app.get("/rooms/:room/events", |req: Request| async move {
    let room = format!("room:{}", req.param("room").unwrap_or("lobby"));
    req.state::<PubSub>()
        .subscribe(&room)
        .sse(|message| Event::new().event("message").data(message))
});
```

`subscription.into_stream()` gives the messages as a `Stream`, for anything else.

## Publishing from anywhere

Routes, background tasks, webhooks and [scheduled tasks](background.md) publish with the same `PubSub`:

```rust
app.post("/rooms/:room/announce", |req: Request| async move {
    let room = format!("room:{}", req.param("room").unwrap_or("lobby"));
    let text = req.text().await?;
    let reached = req.state::<PubSub>().publish(&room, text);
    Ok::<_, Error>(format!("sent to {reached} subscribers"))
});
```

## Limits

Topics and messages live in the memory of the process. With several instances of the application behind a load balancer, each one has its own topics: relay the messages between instances through a broker (Redis, NATS, PostgreSQL's `LISTEN/NOTIFY`) that publishes them on every instance.
