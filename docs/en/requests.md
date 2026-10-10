# Reading requests

Every handler receives a `Request`. From it you can read the method, the path, route parameters, the query string, headers, cookies, the body and the client's address. This page covers each of them, with the Express equivalent where there is one.

## The `Request` object

A handler receives the request by value. Almost every method takes `&self`, body reading included, so you can hold on to a parameter while you wait for the body:

```rust
app.put("/users/:id", |req: Request| async move {
    let id: u64 = req.param_as("id")?;
    let agent = req.header("user-agent").unwrap_or("unknown");
    let body: vitesse::serde_json::Value = req.json().await?;
    Ok::<_, Error>(format!("user {id} updated by {agent}: {body}"))
});
```

The few methods that modify the request (`set`, `set_body`, `set_uri`, `headers_mut`, `extensions_mut`) need a `mut req`. You'll mostly use them in [middleware](middleware.md).

## Method, path and URL

```rust
app.all("/debug", |req: Request| async move {
    format!(
        "{} {} query={:?} version={:?} host={:?}",
        req.method(),       // GET
        req.path(),         // /debug
        req.query_string(), // Some("a=1&b=2")
        req.version(),      // HTTP/1.1
        req.hostname(),     // Some("localhost")
    )
});
```

- `req.method()` returns a `&Method`. Compare it with `*req.method() == Method::POST`.
- `req.path()` returns the path without the query string, exactly as received (not decoded), like `req.path` in Express.
- `req.uri()` returns the full `http::Uri`, path and query string, like `req.originalUrl`.
- `req.hostname()` returns the `Host` header without the port: `example.com` for `example.com:8080`.
- `req.version()` returns the HTTP version (`vitesse::http::Version`).

### Rewriting the URL

`req.set_uri(uri)` replaces the path and query string. Global middleware runs before routing, so a rewrite there changes which route answers:

```rust
app.middleware(|mut req: Request, next: Next| async move {
    if req.path() == "/old-page" {
        req.set_uri(vitesse::http::Uri::from_static("/new-page"));
    }
    next.run(req).await
});
```

The browser doesn't see this rewrite. To send it to the new address instead, return a `Redirect::to(...)` (see [Sending responses](responses.md)).

## Route parameters

These are covered in detail in [Routing](routing.md#path-parameters). In short:

```rust
app.get("/users/:id/posts/:post", |req: Request| async move {
    let user: u64 = req.param_as("id")?;              // typed, 400 if invalid
    let post = req.param("post").unwrap_or_default(); // Option<&str>
    let all: Vec<String> = req.params().map(|(k, v)| format!("{k}={v}")).collect();
    Ok::<_, Error>(format!("{user} {post} {}", all.join("&")))
});
```

## Query string

```rust
app.get("/search", |req: Request| async move {
    let q = req.query("q").unwrap_or_default();
    let page: u32 = req.query("page").and_then(|p| p.parse().ok()).unwrap_or(1);
    format!("searching {q:?}, page {page}")
});
```

`GET /search?q=caf%C3%A9+cr%C3%A8me&page=2` answers `searching "café crème", page 2`.

- `req.query(name)` returns the decoded value of the first parameter with that name, as an `Option<Cow<str>>`. A `+` becomes a space. A `Cow<str>` works like a `&str` (and `.into_owned()` gives you a `String`). It only allocates memory when the value contained encoded characters.
- `req.query_pairs()` iterates over every `(name, value)` pair, repeated names included. That's how you read `?tag=a&tag=b`:

```rust
app.get("/tags", |req: Request| async move {
    let tags: Vec<String> = req
        .query_pairs()
        .filter(|(key, _)| key == "tag")
        .map(|(_, value)| value.into_owned())
        .collect();
    format!("tags: {}", tags.join(", "))
});
```

- `req.query_string()` returns the raw string (`q=caf%C3%A9+cr%C3%A8me&page=2`), or `None` when there's no query string.

### Typed query strings with `query_as`

Once you have more than a couple of parameters, deserialise the whole query string into a struct with `req.query_as::<T>()`. Use `Option` for optional parameters and `#[serde(default)]` for default values:

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Search {
    q: String,
    page: Option<u32>,
    #[serde(default)]
    exact: bool,
}

async fn search(req: Request) -> vitesse::Result<String> {
    let s: Search = req.query_as()?;
    Ok(format!("{} (page {}, exact: {})", s.q, s.page.unwrap_or(1), s.exact))
}
```

If a required field is missing or a value doesn't convert, `query_as` returns a `400 Bad Request`:

```text
GET /search               → 400 {"error":"invalid query string: missing field `q`"}
GET /search?q=x&page=abc  → 400 {"error":"invalid query string: invalid digit found in string"}
```

> [!NOTE]
> `query_as` doesn't handle repeated keys: `?q=a&q=b` is rejected with a `400` (`duplicate field`). Use `req.query_pairs()` for those.

## Headers

```rust
app.get("/whoami", |req: Request| async move {
    let agent = req.header("user-agent").unwrap_or("unknown");
    format!("You are using {agent}")
});
```

- `req.header(name)` returns the value as an `Option<&str>`, like `req.get('User-Agent')` in Express. The name is case-insensitive. You can also pass a constant from `vitesse::header`, such as `req.header(header::AUTHORIZATION)`.
- `req.header_all(name)` iterates over every value of a repeated header.
- `req.headers()` returns the full `HeaderMap`, for example to loop over every header. It's built the first time you call it, so when you only need a few headers, `req.header(...)` is cheaper: it reads the raw request directly.
- `req.headers_mut()` lets a middleware add, change or remove headers before the handler sees them.

> [!NOTE]
> A header value that isn't plain visible ASCII (accented letters, for example) can't always be returned as a `&str`. In that case, read the raw bytes with `req.headers().get("x-name").map(|v| v.as_bytes())`.

### Content type

- `req.content_type()` is a shortcut for `req.header("content-type")`.
- `req.is(type)` checks the body's media type, like Express's `req.is()`, but returns a `bool`. It accepts a full type (`"application/json"`), a subtype (`"json"`, which also matches suffixes such as `application/ld+json`) or a wildcard (`"text/*"`). Parameters like `; charset=utf-8` are ignored.

```rust
app.post("/import", |req: Request| async move {
    if !req.is("json") {
        return Err(Error::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, "expected a JSON body"));
    }
    let items: Vec<vitesse::serde_json::Value> = req.json().await?;
    Ok(format!("{} items imported", items.len()))
});
```

## Cookies

`req.cookie(name)` reads a cookie from the `Cookie` header. In Express you'd need `cookie-parser` for `req.cookies.name`; here nothing needs to be installed:

```rust
app.get("/", |req: Request| async move {
    match req.cookie("session") {
        Some(id) => format!("Welcome back (session {id})"),
        None => "Hello, stranger".to_string(),
    }
});
```

The value comes back as the browser sent it (surrounding quotes removed). It is neither URL-decoded nor signature-checked. To set or delete cookies, see [Sending responses](responses.md).

## Body

Vitesse has no body-parsing middleware. You read the body by calling a method, which collects and parses it on the spot. Every method is `async` and returns a `vitesse::Result`, so `?` turns a bad body into the right HTTP error:

| Method | Express equivalent | Gives you | Error |
|---|---|---|---|
| `req.json::<T>().await` | `express.json()` | `T` (deserialised) | `400` if the JSON is invalid |
| `req.form::<T>().await` | `express.urlencoded()` | `T` (deserialised) | `400` if the form is invalid |
| `req.text().await` | `express.text()` | `String` | `400` if it isn't UTF-8 |
| `req.bytes().await` | `express.raw()` | `Bytes` | |
| `req.take_body()` | reading `req` as a stream | `Body` (a stream) | |

The first four read the whole body into memory, up to the [size limit](#body-size-limit-and-413).

### JSON

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct NewPost {
    title: String,
    tags: Vec<String>,
    draft: Option<bool>,
}

async fn create_post(req: Request) -> vitesse::Result<String> {
    let post: NewPost = req.json().await?;
    Ok(format!(
        "{} ({} tags, draft: {})",
        post.title,
        post.tags.len(),
        post.draft.unwrap_or(false)
    ))
}
```

- Invalid JSON, a missing field or a wrong type gives a `400` that explains why (example below). An empty body is invalid JSON too.
- Unknown fields are ignored. Add `#[serde(deny_unknown_fields)]` to the struct to reject them.
- For JSON with no fixed shape, ask for a `vitesse::serde_json::Value`.
- Unlike `express.json()`, `req.json()` doesn't look at the `Content-Type`: it parses whatever was sent. If you want to require it, check `req.is("json")` first (example above).

For example, sending `{"tags": []}` to this handler gives:

```json
{"error":"invalid JSON: missing field `title` at line 1 column 12"}
```

### Forms

HTML forms (`application/x-www-form-urlencoded`) work the same way, with `req.form()`:

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Login {
    email: String,
    password: String,
    // An unchecked checkbox isn't sent at all: default to `false`
    #[serde(default)]
    remember: bool,
}

async fn login(req: Request) -> vitesse::Result<Redirect> {
    let form: Login = req.form().await?;
    if form.email.is_empty() || form.password.is_empty() {
        return Err(Error::bad_request("email and password are required"));
    }
    // … check the credentials, open a session…
    Ok(Redirect::see_other("/dashboard"))
}
```

Vitesse doesn't parse `multipart/form-data` (file upload forms). Read the raw stream with [`take_body()`](#streaming-with-take_body) and hand it to a multipart parsing crate.

### Text and raw bytes

```rust
app.post("/shout", |req: Request| async move {
    let text = req.text().await?; // String, 400 if it isn't valid UTF-8
    Ok::<_, Error>(text.to_uppercase())
});

app.post("/size", |req: Request| async move {
    let bytes = req.bytes().await?; // vitesse::Bytes
    Ok::<_, Error>(format!("{} bytes received", bytes.len()))
});
```

The body is read once, then kept in memory, so you can call these methods several times. For example, a webhook can check a signature on the raw bytes and then parse the same body as JSON:

```rust
app.post("/webhook", |req: Request| async move {
    let raw = req.bytes().await?;          // check a signature on the raw bytes…
    let event: vitesse::serde_json::Value = req.json().await?; // …then parse them
    Ok::<_, Error>(StatusCode::NO_CONTENT)
});
```

### Body size limit and 413

`json`, `form`, `text` and `bytes` read the whole body into memory, so its size is limited: **1 MiB** by default (`vitesse::DEFAULT_BODY_LIMIT`). A larger body gets a `413 Payload Too Large`. If the client announced the size with `Content-Length`, the request is turned away without reading the body at all. To change the limit:

```rust
let mut app = App::new();
app.body_limit(10 * 1024 * 1024); // 10 MiB
```

- The limit applies to the whole app, like `express.json({ limit: '10mb' })`.
- It only kicks in when you read the body. A handler that never reads it never produces a `413`.
- If only one route needs more (an upload, say), keep the global limit low and stream that route's body with `take_body()`, enforcing your own limit.

### Streaming with `take_body`

For large uploads, or to relay a body elsewhere, `req.take_body()` hands you the raw body as a stream (`vitesse::Body`) without loading it into memory. Data comes in as the client sends it.

> [!WARNING]
> `take_body()` bypasses `app.body_limit`, so limiting the size is up to you. Afterwards, `json()`, `text()` and the other body methods can no longer read the body: they fail with a `500`.

`Body` implements the standard `http_body::Body` trait. The easiest way to read it chunk by chunk is the `frame()` method from [http-body-util](https://docs.rs/http-body-util) (`cargo add http-body-util`). This example saves an upload to a file, capped at 100 MiB:

```rust
use http_body_util::BodyExt; // for .frame()
use vitesse::prelude::*;
use vitesse::tokio::{fs::File, io::AsyncWriteExt};

const MAX_UPLOAD: usize = 100 * 1024 * 1024; // 100 MiB

async fn upload(req: Request) -> vitesse::Result<String> {
    let mut body = req.take_body();
    let mut file = File::create("upload.bin").await?;
    let mut total = 0;

    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|e| Error::bad_request("upload interrupted").with_source(e))?;
        if let Ok(chunk) = frame.into_data() {
            total += chunk.len();
            if total > MAX_UPLOAD {
                return Err(Error::payload_too_large());
            }
            file.write_all(&chunk).await?;
        }
    }
    file.flush().await?;
    Ok(format!("{total} bytes saved"))
}
```

`?` turns file errors (`std::io::Error`) into a `500`. A read error from the stream, though, is a generic `BoxError` that `?` can't convert by itself, which is why the example wraps it with `map_err`.

A response can also be a `Body`, so returning the stream as it is makes a streaming echo:

```rust
app.post("/echo", |req: Request| async move { req.take_body() });
```

### Replacing the body

`req.set_body(body)` replaces the body before the handler reads it. A middleware can use it to decompress or decrypt the body. It accepts anything that converts into a `Body`: `String`, `Vec<u8>`, `Bytes`, `&'static str` and so on. The new body is subject to `body_limit` like the original.

## Client address

```rust
app.get("/ip", |req: Request| async move {
    match req.ip() {
        Some(ip) => format!("Your IP: {ip}"),
        None => "unknown".to_string(),
    }
});
```

- `req.ip()` returns the IP address of the connected client as an `Option<IpAddr>`, like `req.ip`. `req.remote_addr()` returns the IP and the port as an `Option<SocketAddr>`.
- Behind a reverse proxy or a load balancer (Nginx, Heroku, a cloud load balancer and so on), that address is the proxy's. The original client is usually listed in the `X-Forwarded-For` header. Vitesse has no equivalent of Express's `trust proxy` setting, so read the header yourself (see also [Going to production](production.md)):

```rust
fn client_ip(req: &Request) -> Option<String> {
    req.header("x-forwarded-for")
        .and_then(|list| list.split(',').next())
        .map(|ip| ip.trim().to_string())
        .or_else(|| req.ip().map(|ip| ip.to_string()))
}
```

> [!WARNING]
> Clients can send `X-Forwarded-For` themselves. Only rely on it when your proxy sets it, and never base a security decision on it.

## State and per-request data

- `req.state::<T>()` returns the global state registered with `app.state(value)`, the equivalent of `app.locals`. It panics (and so answers `500`) if no state of that type exists. `req.try_state::<T>()` returns an `Option` instead. See [Shared state](state.md).
- `req.set(value)` attaches a value to this request and `req.get::<T>()` reads it back, the equivalent of `res.locals` or `req.user`. The type must implement `Clone`. This is how a middleware passes the logged-in user to the handler (see [Middleware](middleware.md)).
- `req.extensions()` and `req.extensions_mut()` give direct access to the underlying `http::Extensions` storage.

```rust
#[derive(Clone)]
struct User {
    name: String,
}

app.middleware(|mut req: Request, next: Next| async move {
    if req.header("authorization") == Some("Bearer secret") {
        req.set(User { name: "ada".into() });
    }
    next.run(req).await
});

app.get("/me", |req: Request| async move {
    match req.get::<User>() {
        Some(user) => format!("Hello, {}", user.name),
        None => "Not logged in".to_string(),
    }
});
```

## Method reference

| Method | Express | Returns |
|---|---|---|
| `method()` | `req.method` | `&Method` |
| `path()` | `req.path` | `&str` |
| `uri()` | `req.originalUrl` | `&Uri` |
| `version()` | `req.httpVersion` | `Version` |
| `hostname()` | `req.hostname` | `Option<&str>` |
| `set_uri(uri)` | assign `req.url` | |
| `param(name)` | `req.params.name` | `Option<&str>` |
| `param_as::<T>(name)` | | `Result<T, Error>` (`400`) |
| `params()` | `req.params` | iterator of `(&str, &str)` |
| `query(name)` | `req.query.name` | `Option<Cow<str>>` |
| `query_as::<T>()` | | `Result<T, Error>` (`400`) |
| `query_pairs()` | | iterator of `(Cow<str>, Cow<str>)` |
| `query_string()` | | `Option<&str>` |
| `header(name)` | `req.get(name)` | `Option<&str>` |
| `header_all(name)` | | iterator of `&str` |
| `headers()` / `headers_mut()` | `req.headers` | `&HeaderMap` / `&mut HeaderMap` |
| `content_type()` | `req.get('content-type')` | `Option<&str>` |
| `is(type)` | `req.is(type)` | `bool` |
| `cookie(name)` | `req.cookies.name` | `Option<&str>` |
| `json::<T>().await` | `req.body` + `express.json()` | `Result<T, Error>` |
| `form::<T>().await` | `req.body` + `express.urlencoded()` | `Result<T, Error>` |
| `text().await` | `req.body` + `express.text()` | `Result<String, Error>` |
| `bytes().await` | `req.body` + `express.raw()` | `Result<Bytes, Error>` |
| `take_body()` | `req` (stream) | `Body` |
| `set_body(body)` | | |
| `ip()` | `req.ip` | `Option<IpAddr>` |
| `remote_addr()` | `req.socket.remoteAddress` | `Option<SocketAddr>` |
| `state::<T>()` / `try_state::<T>()` | `req.app.locals` | `&T` / `Option<&T>` |
| `get::<T>()` / `set(value)` | `res.locals` | `Option<&T>` / `Option<T>` (the previous value) |
| `extensions()` / `extensions_mut()` | | `&Extensions` / `&mut Extensions` |
