# WebSocket

A WebSocket keeps a connection open in both directions, so the server can push messages to the client at any time: chat, notifications, live dashboards, multiplayer games. Vitesse supports it out of the box, in the style of `express-ws`: `app.ws(path, handler)`, then a loop that reads and writes messages.

## Setup

WebSocket support is the `ws` Cargo feature, **enabled by default**: `vitesse = "0.1"` is all you need. It is built on [tokio-tungstenite](https://github.com/snapview/tokio-tungstenite), the Rust implementation tested against the Autobahn test suite. The handshake (RFC 6455) and the handover of the TCP connection after the `101 Switching Protocols` response are done by Vitesse's own HTTP/1.1 engine.

If you don't need WebSocket, turn off the default features to compile a little less:

```toml
[dependencies]
vitesse = { version = "0.1", default-features = false }
```

## A first WebSocket route

An echo server, which sends every text message back:

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.ws("/echo", |_req, mut socket| async move {
        while let Some(Ok(message)) = socket.recv().await {
            if let ws::Message::Text(text) = message {
                if socket.send(format!("echo: {text}")).await.is_err() {
                    break; // the client is gone
                }
            }
        }
        // The loop ends when the client closes the connection.
    });

    app.run(3000)
}
```

The handler receives the request and the open socket, and the connection lives as long as the handler runs. `ws` is in the prelude, hence `ws::Message`. To try it, use [websocat](https://github.com/vi/websocat), a WebSocket client for the terminal written in Rust (`cargo install websocat`):

```sh
websocat ws://localhost:3000/echo
```

Type a line, and the server answers `echo: …`.

### What `app.ws` does

`app.ws(path, handler)` registers a `GET` route, with the same patterns as any route (`/rooms/:room`, `/files/*path`…). Requests that are not a valid WebSocket handshake get an error, which goes through [`app.on_error`](errors.md) like any other:

| Request | Response |
|---|---|
| A WebSocket handshake (`GET` with `Upgrade: websocket`, `Sec-WebSocket-Version: 13` and a valid key) | `101 Switching Protocols`, then the handler takes over the connection |
| A plain HTTP request (a browser opening the URL, `curl`) | `426 Upgrade Required`: "this route only accepts WebSocket connections" |
| Another version of the protocol (`Sec-WebSocket-Version` other than `13`) | `426 Upgrade Required` |
| An invalid `Sec-WebSocket-Key` | `400 Bad Request` |
| Another method, `HEAD` included | `405 Method Not Allowed` |

`ws` also exists on `Router`, so a WebSocket route can live in a router with its middleware and prefix (see [Middleware and authentication](#middleware-and-authentication)).

## Reading and writing messages

| `WebSocket` method | Effect |
|---|---|
| `socket.recv().await` | The next message: `Some(Ok(message))`, `Some(Err(error))`, or `None` once the connection is closed |
| `socket.send(value).await` | Sends a message: a `String` or `&str` as text, a `Vec<u8>`, `Bytes` or `&[u8]` as binary, or a `ws::Message` |
| `socket.close(code, reason).await` | Starts the closing handshake (`1000`: normal closure) |
| `socket.protocol()` | The subprotocol chosen with [`Upgrade::protocols`](#more-control-wsupgrade), if any |
| `socket.split()` | Two halves, to send from one task while receiving in another (see [below](#sending-and-receiving-at-the-same-time-split)) |

A message is a `ws::Message`:

| Variant | Content |
|---|---|
| `Message::Text(String)` | A text message (valid UTF-8) |
| `Message::Binary(Bytes)` | A binary message |
| `Message::Ping(Bytes)`, `Message::Pong(Bytes)` | Control messages: pings are answered automatically |
| `Message::Close(Option<CloseFrame>)` | The other side closes the connection; `CloseFrame { code: u16, reason: String }` |

Three helpers save a `match`: `message.as_text()` (an `Option<&str>`), `message.as_bytes()` (the payload of a text or binary message) and `message.is_close()`.

`recv` returns `Some(Err(…))` when the connection is reset, when the client violates the protocol or sends a message that is too large; there is nothing left to read after that, so `while let Some(Ok(message))` simply stops at the first error. `send` fails once the connection is closed: `error.is_closed()` tells you so.

### JSON messages

Messages are often JSON. Parse them with serde and answer with `json!`:

```rust
use serde::Deserialize;
use vitesse::prelude::*;

#[derive(Deserialize)]
struct Move {
    x: i32,
    y: i32,
}

let mut app = App::new();
app.ws("/game", |_req, mut socket| async move {
    while let Some(Ok(message)) = socket.recv().await {
        let Some(text) = message.as_text() else {
            continue; // binary, ping, pong...
        };
        let reply = match vitesse::serde_json::from_str::<Move>(text) {
            Ok(m) => json!({ "ok": true, "x": m.x, "y": m.y }),
            Err(e) => json!({ "ok": false, "error": e.to_string() }),
        };
        if socket.send(reply.to_string()).await.is_err() {
            break;
        }
    }
});
```

## Parameters, headers and state

The handler receives the request first, and owns it for the whole connection: route parameters, query string, headers, cookies and [state](state.md) are read as in any handler.

```rust
app.ws("/rooms/:room", |req, mut socket| async move {
    let room = req.param("room").unwrap_or("lobby").to_owned();
    let nickname = req.query("nickname").unwrap_or("anonymous".into()).into_owned();

    let _ = socket.send(format!("{nickname} joined {room}")).await;
    while let Some(Ok(message)) = socket.recv().await {
        if message.is_close() {
            break;
        }
    }
});
```

> [!NOTE]
> Browsers can't add headers such as `Authorization` to a WebSocket connection. They do send the site's cookies with the handshake: authenticate with a session cookie, or with a short-lived token in the query string (`ws://…/live?token=…`).

## Middleware and authentication

Global middleware and router middleware run on the handshake request, like on any request: logging, authentication, rate limiting… A middleware that answers `401` refuses the connection before it is opened, and the headers a middleware adds appear on the `101` response.

```rust
async fn require_session(req: Request, next: Next) -> Response {
    match req.cookie("session") {
        Some(_) => next.run(req).await,
        None => Error::unauthorized("please log in").into_response(),
    }
}

let mut live = Router::new();
live.middleware(require_session);
live.ws("/notifications", |_req, mut socket| async move {
    let _ = socket.send("you are logged in").await;
});

let mut app = App::new();
app.mount("/live", live); // ws://example.com/live/notifications
```

Middleware sees the handshake, not the conversation: `middleware::logger()` logs one line, `GET /live/notifications 101`, as soon as the connection is accepted, and `middleware::timeout(…)` only limits the time it takes to answer the handshake, never the life of the connection.

## More control: `ws::Upgrade`

For subprotocols, a size limit or a check before accepting the connection, use `vitesse::ws::Upgrade` in a regular `GET` route:

```rust
use vitesse::prelude::*;
use vitesse::ws::Upgrade;

let mut app = App::new();
app.get("/chat", |req: Request| async move {
    // Refuse connections opened by pages of other websites.
    if req.header("origin") != Some("https://chat.example.com") {
        return Err(Error::forbidden("unknown origin"));
    }
    let upgrade = Upgrade::new(&req)? // 426, 400 or 405 if this is not a handshake
        .protocols(["chat.v2", "chat.v1"])
        .max_message_size(64 * 1024); // 64 KiB instead of 16 MiB
    Ok(upgrade.on_upgrade(req, |_req, mut socket| async move {
        let protocol = socket.protocol().unwrap_or("none").to_owned();
        let _ = socket.send(format!("welcome, protocol: {protocol}")).await;
        // ... the conversation, as with app.ws
    }))
});
```

| Usage | Effect |
|---|---|
| `Upgrade::new(&req)?` | Checks that the request is a WebSocket handshake; otherwise `426`, `400` or `405`, as with `app.ws` |
| `.offered_protocols()` | The subprotocols offered by the client (`Sec-WebSocket-Protocol`), as a `&[String]` |
| `.protocols([...])` | Chooses the first subprotocol offered by the client that the server supports; `socket.protocol()` returns it |
| `.max_message_size(bytes)` | Maximum size of a received message, and of a frame (default `ws::DEFAULT_MAX_MESSAGE_SIZE`, 16 MiB); beyond it, the connection is closed |
| `.on_upgrade(req, handler)` | Returns the `101 Switching Protocols` response; `handler(req, socket)` then takes over the connection |

When no subprotocol matches, the connection is accepted without one. To refuse it instead, check `offered_protocols()` and return an error before `on_upgrade`.

> [!IMPORTANT]
> Check the `Origin` header before accepting a connection when you authenticate with cookies. Browsers send cookies with the handshake, but the same-origin policy and CORS don't apply to WebSocket: without this check, a page on any other website could open a connection in the name of your logged-in users (cross-site WebSocket hijacking). Clients outside a browser (mobile apps, `websocat`, other servers) usually send no `Origin`.

## Sending and receiving at the same time: `split`

`recv` and `send` both borrow the socket, so a single loop can't wait for the client while it pushes messages of its own. `socket.split()` returns two halves: a `WebSocketSender` (`send`, `close`) and a `WebSocketReceiver` (`recv`), which can live in two tasks.

```rust
use std::time::Duration;

app.ws("/clock", |_req, socket| async move {
    let (mut tx, mut rx) = socket.split();

    // A task that pushes a message every second...
    let ticker = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        for n in 1.. {
            interval.tick().await;
            if tx.send(format!("tick {n}")).await.is_err() {
                break; // the client is gone
            }
        }
    });

    // ...while this one reads until the client leaves.
    while let Some(Ok(message)) = rx.recv().await {
        if message.is_close() {
            break;
        }
    }
    ticker.abort();
});
```

This example uses tokio directly: add it to your dependencies, or go through the re-export, `vitesse::tokio::spawn`.

### `Stream` and `Sink`

`WebSocket` implements `futures::Stream<Item = Result<Message, ws::Error>>` and `Sink<Message>`, and `WebSocketReceiver` implements `Stream`. All the combinators of [futures-util](https://docs.rs/futures-util) (`StreamExt`, `SinkExt`) therefore work, for example to send a whole stream of messages:

```rust
use futures_util::{StreamExt, stream};

app.ws("/countdown", |_req, socket| async move {
    let messages = ["3", "2", "1", "liftoff!"].map(|text| Ok(ws::Message::from(text)));
    // Sends each message, then closes the connection.
    let _ = stream::iter(messages).forward(socket).await;
});
```

## A chat room

The repository's [`examples/chat.rs`](https://github.com/maxlestage/Vitesse/blob/master/examples/chat.rs) is a complete chat room: every message sent by one client is broadcast to all the others. Here is its WebSocket route:

```rust
use tokio::sync::broadcast;
use vitesse::prelude::*;
use vitesse::ws::Message;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    // Every connected client listens to this channel.
    let (room, _) = broadcast::channel::<String>(256);
    app.state(room);

    app.ws("/chat/:name", |req, socket| async move {
        let name = req.param("name").unwrap_or("anonymous").to_owned();
        let room = req.state::<broadcast::Sender<String>>();
        let mut inbox = room.subscribe();
        let (mut tx, mut rx) = socket.split();

        let _ = room.send(format!("* {name} joined"));
        // Forward the room's messages to this client...
        let forward = tokio::spawn(async move {
            while let Ok(line) = inbox.recv().await {
                if tx.send(line).await.is_err() {
                    break;
                }
            }
        });
        // ...and broadcast what this client says.
        while let Some(Ok(message)) = rx.recv().await {
            match message {
                Message::Text(text) => {
                    let _ = room.send(format!("{name}: {}", text.trim_end()));
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        forward.abort();
        let _ = room.send(format!("* {name} left"));
    });

    app.run(3000)
}
```

How it works:

- a tokio [`broadcast`](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html) channel is the room: it is registered as [state](state.md), and every message sent into it reaches every subscriber;
- each connection subscribes (`room.subscribe()`) and splits its socket: a task forwards the room's messages to the client, while the handler's loop broadcasts what the client says;
- when the client leaves, the loop ends: the handler stops the forwarding task and announces the departure.

Try it from a clone of the repository, with one terminal per participant:

```sh
cargo run --release --example chat

# In two other terminals:
websocat ws://localhost:3000/chat/ada
websocat ws://localhost:3000/chat/grace
```

> [!TIP]
> A `broadcast` channel keeps the last 256 messages for slow subscribers. A client that falls further behind gets a `Lagged` error, which ends its forwarding loop in this example. In a real application, handle `RecvError::Lagged` (skip the lost messages, or disconnect the client) so a slow client never silently stops receiving.

## Testing a WebSocket route

[`TestClient`](testing.md) has no real socket: for a WebSocket route, it only sees the handshake response. It is enough to test refusals (`426` for plain HTTP, `401` from your middleware, `403` for an unknown origin). To test a conversation, start the server on a real port and connect with the tokio-tungstenite client:

```toml
[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt"] }
tokio-tungstenite = { version = "0.30", features = ["connect"] }
futures-util = "0.3"
```

```rust
// tests/ws.rs
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn echoes_text_messages() {
    let server = my_api::app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());

    let (mut socket, response) = connect_async(format!("ws://{addr}/echo")).await.unwrap();
    assert_eq!(response.status(), 101);

    socket.send(Message::text("hello")).await.unwrap();
    let reply = socket.next().await.unwrap().unwrap();
    assert_eq!(reply, Message::text("echo: hello"));

    socket.close(None).await.unwrap();
}
```

More examples in [Testing](testing.md#testing-websockets).

## Behind a reverse proxy

The proxy must let the `Upgrade` through, then keep the connection open:

- **Nginx** needs it explicitly, in the `location` that serves your WebSocket routes:

  ```nginx
  location /chat/ {
      proxy_pass http://127.0.0.1:3000;
      proxy_http_version 1.1;
      proxy_set_header Upgrade $http_upgrade;
      proxy_set_header Connection "upgrade";
      proxy_set_header Host $host;
      proxy_read_timeout 1h;   # Nginx closes a silent connection after 60 s by default
  }
  ```

- **Caddy** handles WebSocket automatically with `reverse_proxy`: nothing to add.
- **Heroku**: its router supports WebSocket (see [Heroku](heroku-mobile.md#other-surprises)).

Proxies and platforms close connections that stay silent too long. For a connection that can stay idle (notifications), send a `Message::Ping` every 30 seconds or so from the server: clients, browsers included, answer with a pong automatically. The complete Nginx configuration is in [Going to production](production.md#websocket-behind-nginx).

## Limits

- **HTTP/1.1 only.** WebSocket is not available over [HTTP/3](http3.md): clients open their WebSocket connections over a regular TCP connection, even when the rest of the site uses HTTP/3.
- **Graceful shutdown.** On `Ctrl+C` or `SIGTERM`, open WebSocket connections are not waited for: they are cut as soon as the HTTP requests in progress have finished, and at the latest at the end of the 10-second grace period (see [Server configuration](server.md#graceful-shutdown)). They are cut without a closing handshake, and clients see an abnormal closure (code `1006`). Make your clients reconnect automatically, which they need anyway for network drops.
- **Message size.** 16 MiB per message by default; lower it with `Upgrade::max_message_size` for public endpoints.
- **Threads.** In thread-per-core mode, a connection stays on the thread that accepted it: as in any handler, don't block it (see [Performance](performance.md)).

## Coming from Express

With [express-ws](https://github.com/HenningM/express-ws):

```js
const expressWs = require('express-ws');
expressWs(app);

app.ws('/echo', (ws, req) => {
  ws.on('message', (msg) => ws.send(msg));
  ws.on('close', () => console.log('bye'));
});
```

With Vitesse:

```rust
app.ws("/echo", |_req, mut socket| async move {
    while let Some(Ok(message)) = socket.recv().await {
        match message {
            ws::Message::Text(_) | ws::Message::Binary(_) => {
                if socket.send(message).await.is_err() {
                    break;
                }
            }
            _ => {}
        }
    }
    println!("bye");
});
```

| express-ws | Vitesse |
|---|---|
| `app.ws('/echo', (ws, req) => …)` | `app.ws("/echo", \|req, socket\| async move { … })`: the request comes **first** |
| `ws.on('message', (msg) => …)` | `while let Some(Ok(msg)) = socket.recv().await { … }` |
| `ws.on('close', …)` | The code after the loop |
| `ws.send(data)` | `socket.send(data).await` |
| `ws.close(1000, 'bye')` | `socket.close(1000, "bye").await` |
| `req.params.room` | `req.param("room")` |
| `router.ws(…)` | `router.ws(…)` |
| `wss.clients.forEach(…)` (broadcast) | A `broadcast` channel in the state (see [A chat room](#a-chat-room)) |

Instead of callbacks, a WebSocket is a loop: each connection runs in its own task, and `.await` waits for the next message without blocking anything.
