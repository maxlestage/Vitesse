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

`res::file` guesses the `Content-Type` from the extension, streams large files, and answers `404 {"error":"Not Found"}` if the file doesn't exist. `res::download` also adds `Content-Disposition: attachment` so the browser saves the file under the given name (non-ASCII names are supported). `.attachment("export.csv")` does the same on any response, which is handy for generated content. Relative paths start from the directory the server was launched from. To serve a whole folder, see [Static files](static-files.md).

## Streaming

`Body::from_stream(stream)` sends each item of a stream as soon as it is produced, using `Transfer-Encoding: chunked`. The stream yields `Result<D, E>` values, where `D` converts into bytes (`String`, `&'static str`, `Vec<u8>`, `Bytes`) and `E` is an error type. Streams usually come from crates such as `tokio-stream` or `futures-util`. Here are Server-Sent Events fed by a channel:

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
```

```rust
use std::time::Duration;
use tokio_stream::wrappers::ReceiverStream;
use vitesse::prelude::*;

app.get("/events", |_| async {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<String, std::io::Error>>(16);
    tokio::spawn(async move {
        for i in 1..=5 {
            if tx.send(Ok(format!("data: tick {i}\n\n"))).await.is_err() {
                break; // the client is gone
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    Response::new()
        .content_type("text/event-stream")
        .header("cache-control", "no-cache")
        .send(Body::from_stream(ReceiverStream::new(rx)))
});
```

`Body::wrap(body)` accepts any [`http_body::Body`](https://docs.rs/http-body), and `req.take_body()` hands you the request body as a `Body` that you can send straight back:

```rust
app.post("/echo", |req: Request| async move { req.take_body() });
```

> [!TIP]
> If you know the total size of a stream in advance, set it with `.header("content-length", size)`: the response is then sent with that length instead of chunked encoding.

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
