# Sending responses

In Vitesse, a handler doesn't write to a `res` object: it **returns** its response. Anything that implements the `IntoResponse` trait can be returned: a string, `Json(...)`, a `(status, body)` tuple, a `Result`, or a complete `Response` built with the Express-style `res` helpers.

## Return your response

In Express you call a method on `res`. In Vitesse, the value returned by the handler *is* the response:

```js
app.get('/', (req, res) => res.send('Hello World!'));
app.post('/users', (req, res) => res.status(201).json({ id: 1 }));
```

```rust
use vitesse::prelude::*;

let mut app = App::new();
app.get("/", |_| async { "Hello World!" });
app.post("/users", |_| async { (201, Json(json!({ "id": 1 }))) });
```

The compiler checks that every code path produces a response: you can't forget to answer, and you can't get Express's "headers already sent" error.

## What a handler can return

| Returned type | Status | `Content-Type` |
|---|---|---|
| `&'static str`, `String`, `Cow<'static, str>` | `200` | `text/plain; charset=utf-8` |
| `Json(value)` (any `Serialize` type) | `200` | `application/json` |
| `serde_json::Value` (the `json!` macro) | `200` | `application/json` |
| `Html(body)` | `200` | `text/html; charset=utf-8` |
| `Bytes`, `Vec<u8>`, `&'static [u8]` | `200` | `application/octet-stream` |
| `()` | `200` | none (empty body) |
| `StatusCode` | that status | `text/plain`, the reason phrase as body (`Not Found`) |
| `(status, T)` | `status` | the one of `T` |
| `Option<T>` | `T`, or `404` if `None` | |
| `Result<T, E>` | `T` or `E` (both implement `IntoResponse`) | |
| `Error` | the error's status | `application/json`: `{"error": "..."}` |
| `Redirect` | `301`, `302`, `303` or `307` | none, `Location` header |
| `Body` | `200` | none (see [Streaming](#streaming)) |
| `Sse` (from `sse::channel` or `Sse::new`) | `200` | `text/event-stream` (see [Server-Sent Events](#server-sent-events)) |
| `Response` | whatever you built | |
| `http::Response<Body>` | unchanged | |

```rust
use serde::Serialize;
use vitesse::prelude::*;

#[derive(Serialize)]
struct User {
    id: u32,
    name: String,
}

let mut app = App::new();
app.get("/text", |_| async { "Hello!" });
app.get("/sum", |_| async { format!("1 + 1 = {}", 1 + 1) });
app.get("/user", |_| async { Json(User { id: 1, name: "Ada".into() }) });
app.get("/stats", |_| async { json!({ "users": 42, "online": true }) });
app.get("/page", |_| async { Html("<h1>Welcome</h1>") });
app.get("/ping", |_| async { StatusCode::NO_CONTENT });
```

> [!NOTE]
> The `json!` macro and `serde_json` are re-exported by Vitesse (`vitesse::json`, `vitesse::serde_json`): you don't need to add `serde_json` to your `Cargo.toml`. To derive `Serialize` on your own types, add `serde = { version = "1", features = ["derive"] }`.

### Status codes

A `(status, body)` tuple changes the status of any response. The status can be a number or a `StatusCode` constant:

```rust
app.post("/users", |_| async { (201, Json(json!({ "id": 2 }))) });
app.post("/jobs", |_| async { (StatusCode::ACCEPTED, "queued") });
```

Returning a `StatusCode` on its own sends the status with its reason phrase as text, like Express's `res.sendStatus(404)`. An invalid code (such as `1000`) becomes a `500`.

### `Option` and `Result`

`None` becomes a `404 {"error":"Not Found"}`. A `Result` sends either the success value or the error: most handlers return `vitesse::Result<T>` and use `?`, as explained in [Error handling](errors.md).

```rust
// `find_user` is your own function returning an `Option<User>`.
async fn show_user(req: Request) -> vitesse::Result<Json<User>> {
    let id: u32 = req.param_as("id")?; // 400 if it isn't a number
    let user = find_user(id).ok_or_else(|| Error::not_found("user not found"))?;
    Ok(Json(user))
}
```

## The `res` builder

When you need full control (status, headers, cookies and body together), build a `Response`. The `res` module provides Express-like entry points, and each one returns a `Response` you can keep chaining:

```rust
app.get("/custom", |_| async {
    res::status(202)
        .header("x-powered-by", "Vitesse")
        .cookie(Cookie::new("seen", "1").http_only(true))
        .json(json!({ "ok": true }))
});
```

| Express | Vitesse |
|---|---|
| `res.status(201)` | `res::status(201)` or `.status(201)` |
| `res.send(body)` | `res::send(body)` or `.send(body)` |
| `res.json(obj)` | `res::json(obj)` or `.json(obj)` |
| `res.type('text/csv')` | `.content_type("text/csv")` |
| `res.set(name, value)` | `.header(name, value)` |
| `res.append(name, value)` | `.append_header(name, value)` |
| `res.cookie(...)` / `res.clearCookie(name)` | `.cookie(Cookie::new(...))` / `.clear_cookie(name)` |
| `res.attachment(name)` | `.attachment(name)` |
| `res.redirect(url)` | `res::redirect(url)` |
| `res.sendStatus(404)` | `res::send_status(404)` |
| `res.sendFile(path)` | `res::file(path).await` |
| `res.download(path, name)` | `res::download(path, name).await` |

There are also `res::text(...)` / `.text(...)` and `res::html(...)` / `.html(...)`, and `Response::new()` gives you an empty `200` to start from. Each method takes the response and returns it, so the order is up to you. Just remember that `.text()`, `.html()` and `.json()` set the `Content-Type` (replacing an earlier `.content_type(...)`), while `.send()` only sets the body.

> [!WARNING]
> Unlike Express's `res.send()`, `send` never guesses a `Content-Type`: `res::send("hello")` goes out without one. Use `text`, `html` or `json`, or add `.content_type(...)`.

### Headers

`.header(name, value)` sets a header (replacing any previous value) and `.append_header(name, value)` adds one more value. Names can be strings or the constants of `vitesse::header`; values can be `&str`, `String` or integers. An invalid name or value (a newline, for example) is silently ignored.

```rust
use vitesse::header;

app.get("/report", |_| async {
    Response::new()
        .header(header::CACHE_CONTROL, "no-store")
        .header("x-total-count", 42)
        .append_header("vary", "accept")
        .append_header("vary", "accept-language")
        .text("...")
});
```

To change a response you already have (typically in a [middleware](middleware.md)), use the in-place versions: `set_status`, `set_header`, `headers_mut()`, and to read it, `status_code()` and `get_header(name)`.

### Content type

`text`, `html` and `json` cover the common cases. For anything else, set it yourself:

```rust
app.get("/export.csv", |_| async {
    res::send("id,name\n1,Ada\n").content_type("text/csv; charset=utf-8")
});
```

### Cookies

```rust
use std::time::Duration;
use vitesse::SameSite;

app.post("/login", |_| async {
    let session = Cookie::new("session", "abc123")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::from_secs(7 * 24 * 3600));
    Response::new().cookie(session).json(json!({ "ok": true }))
});

app.post("/logout", |_| async { res::redirect("/").clear_cookie("session") });
```

The login route sends `Set-Cookie: session=abc123; Path=/; Max-Age=604800; HttpOnly; Secure; SameSite=Lax`.

| `Cookie` method | Attribute |
|---|---|
| `Cookie::new(name, value)` | `name=value; Path=/` |
| `.path("/admin")` | `Path` |
| `.domain("example.com")` | `Domain` |
| `.max_age(Duration)` | `Max-Age` (in seconds) |
| `.secure(true)` | `Secure` |
| `.http_only(true)` | `HttpOnly` |
| `.same_site(SameSite::Strict / Lax / None)` | `SameSite` |

Each `.cookie(...)` call adds its own `Set-Cookie` header, so you can send several. `.clear_cookie(name)` sends `name=; Path=/; Max-Age=0`: if the cookie was created with another path or domain, build the deletion cookie yourself with the same attributes and `.max_age(Duration::ZERO)`. To read cookies sent by the browser, use `req.cookie("session")` (see [Reading requests](requests.md)).

> [!IMPORTANT]
> The value is written as is: it is not signed, encrypted or encoded. Keep it URL-safe (a random session token, for instance) and store sensitive data on the server. Browsers only accept `SameSite::None` together with `.secure(true)`.

### Redirects

| Helper | Status |
|---|---|
| `Redirect::to(url)` or `res::redirect(url)` | `302 Found` (Express's default) |
| `Redirect::permanent(url)` | `301 Moved Permanently` |
| `Redirect::see_other(url)` | `303 See Other` (after a form `POST`) |
| `Redirect::temporary(url)` | `307 Temporary Redirect` (keeps the method and body) |

```rust
app.get("/old-page", |_| async { Redirect::permanent("/new-page") });
app.post("/contact", |_| async { Redirect::see_other("/thanks") });
```

For another status, set the header yourself: `res::status(308).header("location", "/v2")`. A URL containing invalid characters (such as a newline) produces a `500`.

## Files and downloads

```rust
app.get("/terms", |_| async { res::file("legal/terms.pdf").await });
app.get("/invoice", |_| async {
    res::download("files/invoice-42.pdf", "invoice.pdf").await
});
```

`res::file` guesses the `Content-Type` from the extension, streams large files, and answers `404 {"error":"Not Found"}` if the file doesn't exist. Like Express, it also sends `ETag` and `Last-Modified`, answers `304 Not Modified` when the browser already has the file, and `206 Partial Content` to `Range` requests from video players and download managers (see [Static files](static-files.md#range-requests)). `res::download` also adds `Content-Disposition: attachment` so the browser saves the file under the given name (non-ASCII names are supported). `.attachment("export.csv")` does the same on any response, which is handy for generated content. Relative paths start from the directory the server was launched from. To serve a whole folder, see [Static files](static-files.md).

## Streaming

`Body::from_stream(stream)` sends each item of a stream as soon as it is produced, using `Transfer-Encoding: chunked` over HTTP/1.1. The stream yields `Result<D, E>` values, where `D` converts into bytes (`String`, `&'static str`, `Vec<u8>`, `Bytes`) and `E` is an error type. Streams usually come from crates such as `tokio-stream` or `futures-util`. Here is a large CSV export, produced by a task and sent as it goes:

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
```

```rust
use tokio_stream::wrappers::ReceiverStream;
use vitesse::prelude::*;

app.get("/export.csv", |_| async {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<String, std::io::Error>>(16);
    tokio::spawn(async move {
        let _ = tx.send(Ok("id,name\n".to_string())).await;
        for id in 1..=100_000 {
            // In a real app: rows read from the database, page by page.
            if tx.send(Ok(format!("{id},user {id}\n"))).await.is_err() {
                break; // the client is gone
            }
        }
    });
    Response::new()
        .content_type("text/csv; charset=utf-8")
        .attachment("users.csv")
        .send(Body::from_stream(ReceiverStream::new(rx)))
});
```

To push events to a browser page (notifications, progress, live data), don't write the `text/event-stream` format by hand: use [Server-Sent Events](#server-sent-events), just below.

`Body::wrap(body)` accepts any [`http_body::Body`](https://docs.rs/http-body), and `req.take_body()` hands you the request body as a `Body` that you can send straight back:

```rust
app.post("/echo", |req: Request| async move { req.take_body() });
```

> [!TIP]
> If you know the total size of a stream in advance, set it with `.header("content-length", size)`: the response is then sent with that length instead of chunked encoding.

## Server-Sent Events

Server-Sent Events (SSE) keep a response open and push events to the browser as they happen: notifications, a progress bar, a live dashboard, the words of a chatbot's answer. When only the server talks, it's simpler than a [WebSocket](websocket.md): it's plain HTTP, and the browser receives it with `EventSource`, which reconnects by itself. The `sse` module (in the prelude, no feature needed) takes care of the format and the headers.

### With a channel

The simplest way is `sse::channel(buffer)`: it returns a sender and the response. The handler returns the response right away, and a task sends the events:

```rust
use std::time::Duration;
use vitesse::prelude::*;
use vitesse::sse::Event;

app.get("/clock", |_| async {
    let (tx, stream) = sse::channel(16);
    tokio::spawn(async move {
        for seconds in 0.. {
            let event = Event::new().event("tick").data(format!("{seconds} s"));
            if tx.send(event).await.is_err() {
                break; // the browser has gone away
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    stream
});
```

The response goes out with `content-type: text/event-stream` and `cache-control: no-cache`, and each event leaves as soon as it is sent:

```text
event: tick
data: 0 s

event: tick
data: 1 s

```

- `buffer` is the number of events that can wait while the connection is slow; when it's full, `send` waits.
- The response ends when every `Sender` is dropped: here, when the task ends. `Sender` is `Clone`, so several tasks can feed the same response.
- `tx.try_send(event)` sends without waiting. It fails with `sse::TrySendError::Full` when the buffer is full (a slow connection: skip the event, or send it later) and with `TrySendError::Closed` once the browser has gone away; both hand the event back. `tx.is_closed()` tells you whether the browser is still there.

### From a stream

`Sse::new(stream)` accepts any `Stream` of `Result<Event, E>`, from tokio-stream, futures-util or your own code. The response ends with the stream:

```rust
use std::convert::Infallible;
use vitesse::sse::{Event, Sse};

app.get("/countdown", |_| async {
    let events = (1..=3).rev().map(|n| Ok::<_, Infallible>(Event::new().data(n.to_string())));
    Sse::new(futures_util::stream::iter(events))
});
```

An `Err` from the stream cuts the connection: the browser sees a network error and reconnects.

### Events

| `Event` method | Line sent | In the browser |
|---|---|---|
| `Event::new().data(text)` | `data: …` (one line per line of `text`, joined back by the browser) | `e.data` |
| `.json_data(&value)?` | `data: …`, `value` serialised to JSON | `JSON.parse(e.data)` |
| `.event("tick")` | `event: tick` | the listener that receives it: `addEventListener("tick", …)` (`message` without it) |
| `.id("42")` | `id: 42` | sent back in `Last-Event-ID` when the browser reconnects |
| `.retry(Duration::from_secs(5))` | `retry: 5000` | how long the browser waits before reconnecting |
| `.comment("…")` | `: …` | ignored |

`json_data` returns a `Result` because serialisation can fail (a `500` with `?`). `event` and `id` panic if they contain a line break, which would break the format. `event.to_string()` returns the text that will be sent, handy in tests.

### Keep-alive

Proxies and load balancers close connections that stay silent too long, often after a minute. So when no event has been sent for 15 seconds, both `sse::channel` and `Sse::new` send a comment (`:`), which the browser ignores. To change it, or turn it off with `.no_keep_alive()`:

```rust
use vitesse::sse::KeepAlive;

let (tx, stream) = sse::channel(16);
// … spawn the task that uses `tx`
stream.keep_alive(KeepAlive::new().interval(Duration::from_secs(30)).text("ping"))
```

### When the browser leaves

When the user closes the tab, the connection closes and Vitesse drops the response. From then on, `tx.send(...)` returns an error: stop the task there, as in the clock above. Vitesse notices the departure when it next writes to the connection: an event or, at the latest, the next keep-alive comment.

A task that rarely sends would wait a long time before finding out. Make it wait for `tx.closed()` at the same time, with `tokio::select!`. Here, each page receives the news published on a `broadcast` channel kept in the [state](state.md):

```rust
use tokio::sync::broadcast;

let (news, _) = broadcast::channel::<String>(100);
app.state(news);

app.get("/news", |req: Request| async move {
    let mut inbox = req.state::<broadcast::Sender<String>>().subscribe();
    let (tx, stream) = sse::channel(16);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tx.closed() => break, // the browser has gone away
                message = inbox.recv() => match message {
                    Ok(text) => { let _ = tx.send(Event::new().data(text)).await; }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                },
            }
        }
    });
    stream
});
```

### In the browser

```js
const source = new EventSource("/clock");
source.addEventListener("tick", (e) => console.log("tick", e.data));
source.onmessage = (e) => console.log(e.data); // events without an `event` type
```

`EventSource` reconnects by itself when the connection drops, and also when the response ends. On reconnection, it sends the `id` of the last event it received in a `Last-Event-ID` header: read it to send what the page missed.

```rust
app.get("/feed", |req: Request| async move {
    let last: u64 = req.header("last-event-id").and_then(|id| id.parse().ok()).unwrap_or(0);
    let (tx, stream) = sse::channel(16);
    tokio::spawn(async move {
        for n in last + 1.. {
            let event = Event::new().id(n.to_string()).data(format!("message {n}"));
            if tx.send(event).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    stream
});
```

To stop for good, call `source.close()` in JavaScript (after a last event that tells the page it's over, for instance), or answer the reconnection with `204 No Content`.

### Good to know

- SSE works the same over HTTP/1.1, [HTTP/2](https.md) and [HTTP/3](http3.md). Over HTTP/1.1, browsers open at most 6 connections per site, and each `EventSource` keeps one busy: with many tabs open, HTTP/2 or HTTP/3, which share one connection, avoid that limit.
- `middleware::compress()` leaves event streams alone, and `middleware::timeout(...)` only limits the time taken to return the response, not how long the stream lasts.
- Nginx buffers responses by default, which holds the events back: add `proxy_buffering off;` to the `location`, or send the header from Vitesse with `stream.into_response().header("x-accel-buffering", "no")`.
- `examples/live.rs`, in the repository, shows a live clock in a page (`cargo run --example live --features multipart`).

## `HEAD`, `204` and `304`

You don't have to handle these cases: a `HEAD` request uses the `GET` route and the engine sends the headers (including `Content-Length`) without the body, and `204 No Content`, `304 Not Modified` and `1xx` responses never carry a body.

## Your own types

Implement `IntoResponse` to return your own types directly from handlers:

```rust
struct Csv(String);

impl IntoResponse for Csv {
    fn into_response(self) -> Response {
        res::send(self.0).content_type("text/csv; charset=utf-8")
    }
}

app.get("/export", |_| async { Csv("id,name\n1,Ada\n".into()) });
```

The same technique turns your own error types into responses (see [Error handling](errors.md)). Finally, if you work with the [`http`](https://docs.rs/http) crate, `Response::from_http` and `Response::into_http` convert in both directions, and a handler can return an `http::Response<Body>` directly.
