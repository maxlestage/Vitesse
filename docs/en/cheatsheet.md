# API cheat sheet

The whole public API of Vitesse on one page, grouped by theme: each line gives the usage and what it does. For exact signatures and every detail, see the reference on [docs.rs/vitesse](https://docs.rs/vitesse).

## Skeleton

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());

    app.get("/", |_| async { "Hello World!" });
    app.get("/users/:id", |req: Request| async move {
        let id: u64 = req.param_as("id")?;
        Ok::<_, Error>(Json(json!({ "id": id })))
    });

    app.run(3000)
}
```

`use vitesse::prelude::*;` imports `App`, `Router`, `Request`, `Response`, `Next`, `Error`, `Json`, `Html`, `Redirect`, `Cookie`, `Body`, `ServeDir`, `StatusCode`, `Method`, `IntoResponse`, `HandlerExt`, `json!`, the `res` module, the `middleware` module and the `ws` module (WebSocket).

## Cargo features

| Feature | Default | Adds |
|---|---|---|
| `ws` | Enabled | `app.ws`, `router.ws` and the `vitesse::ws` module |
| `http3` | Disabled | `app.http3`, `server.http3_addr` and the `vitesse::http3` module |

```toml
[dependencies]
vitesse = { version = "0.1", features = ["http3"] }        # adds HTTP/3
# vitesse = { version = "0.1", default-features = false } # without WebSocket
```

See [Installation](installation.md#cargo-features).

## App

| Usage | Description |
|---|---|
| `App::new()` | An empty application (`express()`) |
| `app.run(addr)` | Starts the server with its own runtime; blocks; stops cleanly on `Ctrl+C` / `SIGTERM` |
| `app.listen(addr).await` | Starts the server on the current tokio runtime; stops cleanly on `Ctrl+C` / `SIGTERM` |
| `app.bind(addr).await?` | Opens the port and returns a `Server`, without serving yet |
| `app.middleware(mw)` | Global middleware (`app.use`): every request, in order, before routing |
| `app.fallback(handler)` | Handler called when no route matches (default: `404 {"error":"Cannot GET /x"}`) |
| `app.on_error(f)` | Customises every error response; `f` is a `Fn(Error) -> impl IntoResponse` |
| `app.state(value)` | Registers a global state value, one per type (`app.locals`) |
| `app.body_limit(bytes)` | Maximum size of a body read in memory (default `DEFAULT_BODY_LIMIT`, 1 MiB) |
| `app.workers(n)` | Number of threads for `run` (default: one per CPU) |
| `app.thread_per_core(bool)` | Thread-per-core mode for `run` (default `true`, Linux only) |
| `app.http3(config)` | Also serves the app over HTTP/3 (QUIC, UDP), next to HTTP/1.1 (`http3` feature) |

All these setters return `&mut App`, so calls can be chained. See [Server configuration](server.md).

## Routing

These methods exist on both `App` and `Router`, and can be chained.

| Usage | Description |
|---|---|
| `app.get(path, handler)` | `GET` route (also used for `HEAD`) |
| `app.post(…)`, `.put(…)`, `.patch(…)`, `.delete(…)` | Routes for these methods |
| `app.head(path, handler)` | Explicit `HEAD` route |
| `app.options(path, handler)` | Explicit `OPTIONS` route (default: `204` with `Allow`) |
| `app.all(path, handler)` | Every method |
| `app.route(method, path, handler)` | Any method, e.g. `Method::from_bytes(b"PURGE").unwrap()` |
| `app.mount(prefix, router)` | Mounts a `Router` under a prefix (`app.use('/api', router)`) |
| `app.static_dir(prefix, dir)` | Serves a folder under `prefix` |
| `app.serve_dir(prefix, serve_dir)` | Same, with a configured `ServeDir` |
| `app.ws(path, handler)` | WebSocket route (`GET`): `handler(req, socket)` takes over the connection (express-ws) |

| Pattern | Matches |
|---|---|
| `/users` | Exactly `/users` (and `/users/`) |
| `/users/:id` | One segment, read with `req.param("id")`; names use letters, digits and `_` |
| `/files/*path` | The rest of the path, read with `req.param("path")`; also matches `/files` |
| `/*` | Every path; the parameter is named `*` |

Priority: static > parameter > wildcard. Parameters are percent-decoded (`%C3%A9` → `é`), matching is case-sensitive, a wrong method answers `405` with `Allow`, and an invalid or duplicate route panics at startup. See [Routing](routing.md).

## Handlers

| Usage | Description |
|---|---|
| `async fn show(req: Request) -> impl IntoResponse` | A named handler |
| `\|req: Request\| async move { … }` | A closure handler that uses the request |
| `\|_\| async { … }` | A closure handler that ignores the request |
| `Ok::<_, Error>(value)` | Last expression of a closure that uses `?` |
| `handler.with(mw)` | Adds a middleware to one route (`HandlerExt`); returns a `Chained` |
| `handler.with(a).with(b)` | Several route middleware, run in that order |
| `impl Handler for MyType` | Custom handler: `fn call(&'static self, req: Request) -> BoxFuture<Response>` |

## Request

| Usage | Description |
|---|---|
| `req.method()` | The HTTP method (`&Method`) |
| `req.path()` | The path, without the query string |
| `req.uri()` | The full URI (`&Uri`) |
| `req.version()` | The HTTP version |
| `req.set_uri(uri)` | Rewrites the URI (URL rewriting in a global middleware) |
| `req.header(name)` | A header as `Option<&str>`, case-insensitive (`req.get('host')`) |
| `req.header_all(name)` | Every value of a repeated header |
| `req.headers()`, `req.headers_mut()` | All headers (`HeaderMap`) |
| `req.content_type()` | The `Content-Type` |
| `req.is("json")` | Checks the body type: full type, subtype or `text/*` |
| `req.hostname()` | The requested host, without the port |
| `req.cookie(name)` | A cookie value (`req.cookies.name`) |
| `req.param(name)` | A route parameter, `Option<&str>` |
| `req.param_as::<T>(name)?` | A parameter converted to `T`; `400` on failure |
| `req.params()` | Every `(name, value)` parameter |
| `req.query(name)` | A decoded query string value, `Option<Cow<str>>` |
| `req.query_as::<T>()?` | The query string deserialised into a struct; `400` on failure |
| `req.query_pairs()` | Every query string pair |
| `req.query_string()` | The raw query string |
| `req.json::<T>().await?` | JSON body (`express.json()`); `400` if invalid, `413` if too large |
| `req.form::<T>().await?` | URL-encoded form (`express.urlencoded()`) |
| `req.text().await?` | Body as UTF-8 text; `400` if invalid |
| `req.bytes().await?` | Raw body (`Bytes`) |
| `req.take_body()` | The body as a stream (`Body`), not limited by `body_limit` |
| `req.set_body(body)` | Replaces the body |
| `req.state::<T>()` | Global state (`&'static T`); panics, hence `500`, if missing |
| `req.try_state::<T>()` | Same, as an `Option` |
| `req.set(value)` | Attaches data to the request (`res.locals`); the type must be `Clone` |
| `req.get::<T>()` | Reads data attached by a middleware |
| `req.extensions()`, `req.extensions_mut()` | The raw `http::Extensions` |
| `req.ip()`, `req.remote_addr()` | Client IP, and IP with port |

The body is read on demand and cached: the reading methods can be called more than once. See [Requests](requests.md).

## Responses

### What a handler can return

| Returned value | Response |
|---|---|
| `&'static str`, `String`, `Cow<'static, str>` | `200`, `text/plain; charset=utf-8` |
| `Json(value)`, `serde_json::Value` (`json!`) | `200`, `application/json` |
| `Html(body)` | `200`, `text/html; charset=utf-8` |
| `Bytes`, `Vec<u8>`, `&'static [u8]` | `200`, `application/octet-stream` |
| `()` | `200`, empty body |
| `StatusCode::NOT_FOUND` | That status, with its reason as text |
| `(status, value)` | `value` with that status (`u16`, `i32` or `StatusCode`) |
| `Option<T>` | `T`, or `404` if `None` |
| `Result<T, E>` | `T` or `E` (both must be responses) |
| `Error` | Its status and `{"error": "message"}` |
| `Redirect::to(url)` | `302` with `Location` |
| `Body` | `200` with that body (e.g. a stream) |
| `Response`, `http::Response<Body>` | As built |

### Building a `Response`

| Usage | Description |
|---|---|
| `Response::new()` | An empty `200` response |
| `.status(code)` | Status (`res.status()`) |
| `.header(name, value)` | Sets a header, replacing it (invalid names or values are ignored) |
| `.append_header(name, value)` | Adds a header without replacing |
| `.content_type(value)` | `Content-Type` (`res.type()`) |
| `.text(body)`, `.html(body)`, `.json(value)` | Body with the matching content type |
| `.send(body)` | Body, without touching the content type |
| `.cookie(cookie)` | Adds a `Set-Cookie` (`res.cookie()`) |
| `.clear_cookie(name)` | Deletes a cookie in the browser (`res.clearCookie()`) |
| `.attachment(filename)` | Download under that name (`res.attachment()`) |
| `res.status_code()`, `res.get_header(name)` | Read the status, a header |
| `res.set_status(code)`, `res.set_header(name, value)` | Modify in place (`&mut`) |
| `res.headers()`, `res.headers_mut()` | All headers |
| `res.body()`, `res.body_mut()`, `res.into_body()` | The body |
| `res.error()`, `res.take_error()` | The `Error` this response comes from, if any |
| `res.extensions()`, `res.extensions_mut()` | Typed data attached to the response |
| `res.into_http()`, `Response::from_http(r)` | Converts to and from `http::Response<Body>` |

### The `res` shortcuts

| Usage | Description |
|---|---|
| `res::status(code)` | A `Response` with this status, to chain (`res::status(201).json(v)`) |
| `res::send(body)`, `res::text(t)`, `res::html(h)`, `res::json(v)` | A `200` response with that body |
| `res::redirect(url)` | `302` redirect |
| `res::send_status(code)` | The status and its reason as text (`res.sendStatus()`) |
| `res::file(path).await` | Sends a file with `ETag`, `304` and `Range` (`206`); `404` if it does not exist (`res.sendFile()`) |
| `res::download(path, name).await` | Same, as an attachment (`res.download()`) |

### Redirects, cookies and bodies

| Usage | Description |
|---|---|
| `Redirect::to(url)` | `302 Found` |
| `Redirect::permanent(url)` | `301 Moved Permanently` |
| `Redirect::see_other(url)` | `303 See Other` (after a POST) |
| `Redirect::temporary(url)` | `307 Temporary Redirect` (keeps the method) |
| `Cookie::new(name, value)` | A cookie with `Path=/` |
| `.path(p)`, `.domain(d)`, `.max_age(duration)` | Cookie attributes |
| `.secure(bool)`, `.http_only(bool)`, `.same_site(SameSite::Lax)` | Cookie attributes (`vitesse::SameSite`: `Strict`, `Lax`, `None`) |
| `Body::empty()`, `Body::from(x)` | Empty body, or from `&'static str`, `String`, `Vec<u8>`, `Bytes`… |
| `Body::from_stream(stream)` | Streamed (`chunked`) body from a `Stream` of `Result<impl Into<Bytes>, E>` |
| `Body::wrap(body)` | Wraps any `http_body::Body` (proxies) |
| `body.size()`, `body.to_bytes().await` | Size if known; reads everything into memory |

See [Responses](responses.md).

## Errors

| Usage | Description |
|---|---|
| `Error::new(status, message)` | An error with a status and a message for the client |
| `Error::from_status(status)` | The message is the standard reason (`Not Found`…) |
| `Error::bad_request(msg)` | `400` |
| `Error::unauthorized(msg)` | `401` |
| `Error::forbidden(msg)` | `403` |
| `Error::not_found(msg)` | `404` |
| `Error::conflict(msg)` | `409` |
| `Error::payload_too_large()` | `413` |
| `Error::unprocessable(msg)` | `422` |
| `Error::internal(msg)` | `500` |
| `.with_source(err)` | Attaches the original cause (logged for 5xx errors) |
| `err.status()`, `err.message()`, `err.source()` | Read the error |
| `vitesse::Result<T>` | Alias for `Result<T, vitesse::Error>` |
| `?` on any standard error | `500 {"error":"Internal Server Error"}`; the cause is only logged |
| Panic in a handler or a middleware | `500`; the server keeps running |

See [Errors](errors.md).

## Middleware

| Usage | Description |
|---|---|
| `async fn mw(req: Request, next: Next) -> Response` | A middleware as a function |
| `\|req: Request, next: Next\| async move { … }` | A middleware as a closure |
| `next.run(req).await` | Continues the chain and returns the `Response` (`next()`) |
| Return a response without calling `next` | Stops the chain (authentication, cache…) |
| `app.middleware(mw)` | For every request |
| `router.middleware(mw)` | For that router's routes, and the `404`, `405` and `OPTIONS` responses under its prefix |
| `handler.with(mw)` | For a single route |
| `impl Middleware for MyType` | `fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response>` |

| Built-in middleware | Equivalent | Notes |
|---|---|---|
| `middleware::logger()` | `morgan('dev')` | One line per request on standard output |
| `middleware::cors()` | `cors()` | Every origin (`*`) by default |
| `.allow_origin("https://…")` | `origin` | Only these origins (can be repeated) |
| `.allow_methods([Method::GET, …])` | `methods` | Default: `GET, HEAD, PUT, PATCH, POST, DELETE` |
| `.allow_headers("…")`, `.expose_headers("…")` | `allowedHeaders`, `exposedHeaders` | By default, the headers the browser asks for are allowed |
| `.allow_credentials(true)` | `credentials` | Combine with `allow_origin`: with every origin allowed, the response says `*` and browsers refuse cookies |
| `.max_age(duration)` | `maxAge` | Cache duration of preflight requests |
| `middleware::helmet()` | `helmet()` | Security headers |
| `middleware::timeout(duration)` | `connect-timeout` | `503` when the duration is exceeded |
| `middleware::serve_static(dir)` | `express.static()` | Same as `ServeDir::new(dir)` |

See [Middleware](middleware.md).

## Routers

| Usage | Description |
|---|---|
| `Router::new()` | An empty router (`express.Router()`) |
| `router.get(…)`, `.post(…)`, … `.route(…)`, `.ws(…)` | Same routing methods as `App` |
| `router.mount(prefix, other)` | Nested routers |
| `router.static_dir(…)`, `router.serve_dir(…)` | Static files inside a router |
| `router.middleware(mw)` | Middleware for this router: its routes and every other request under its prefix (`router.use`) |
| `app.mount("/api", router)` | Mounts it; the prefix can contain parameters (`/users/:id/posts`) |

See [Routers](routers.md).

## Static files

| Usage | Description |
|---|---|
| `ServeDir::new(dir)` | Serves the `dir` folder |
| `.index(Some("home.html"))`, `.index(None)` | File served for a folder (default `index.html`), or none |
| `.max_age(duration)` | Browser cache (`Cache-Control: max-age`) |
| `.dotfiles(true)` | Allows hidden files (refused by default) |
| `app.static_dir("/assets", "public")` | As a route: `GET /assets/*` |
| `app.serve_dir("/assets", ServeDir::new("public").max_age(d))` | As a route, with options |
| `app.middleware(ServeDir::new("public"))` | As a middleware: missing files fall through to the routes |

Built in: MIME types, `index.html`, `ETag` and `Last-Modified` (`304`), `Range` requests (`206`), streaming of large files, protection against `../` and hidden files. See [Static files](static-files.md).

## WebSocket

| Usage | Description |
|---|---|
| `app.ws("/chat/:room", \|req, mut socket\| async move { … })` | WebSocket route; `426` for plain HTTP, `400` for an invalid key, `405` for another method |
| `socket.recv().await` | Next message: `Some(Ok(msg))`, `Some(Err(e))`, or `None` once closed |
| `socket.send(value).await` | `String` / `&str` as text, `Vec<u8>` / `Bytes` / `&[u8]` as binary, or a `ws::Message` |
| `socket.close(1000, "bye").await` | Closing handshake with a code and a reason |
| `socket.protocol()` | The chosen subprotocol, if any |
| `socket.split()` | `(WebSocketSender, WebSocketReceiver)`, to send and receive from two tasks |
| `ws::Message::Text(String)`, `Binary(Bytes)`, `Ping(Bytes)`, `Pong(Bytes)`, `Close(Option<CloseFrame>)` | The messages; pings are answered automatically |
| `msg.as_text()`, `msg.as_bytes()`, `msg.is_close()` | Helpers |
| `err.is_closed()` | The connection is already closed (`ws::Error`) |
| `ws::Upgrade::new(&req)?` | In a `GET` route: checks the handshake (`426`, `400`, `405`) |
| `.protocols(["v2", "v1"])`, `.offered_protocols()` | Chooses a subprotocol offered by the client; the offered list |
| `.max_message_size(bytes)` | Size limit of messages and frames (default `ws::DEFAULT_MAX_MESSAGE_SIZE`, 16 MiB) |
| `.on_upgrade(req, \|req, socket\| async move { … })` | Returns the `101 Switching Protocols` response, then runs the handler |

`WebSocket` implements `Stream` and `Sink`; global and router middleware run on the handshake. See [WebSocket](websocket.md).

## Server

| Usage | Description |
|---|---|
| `3000`, `"3000"` | Listens on `0.0.0.0:3000` |
| `"127.0.0.1:8080"`, `"[::]:3000"`, `"localhost:3000"` | Listens on that address |
| `String`, `SocketAddr`, `([127, 0, 0, 1], 8080)` | Other accepted forms (`ListenAddr`) |
| `server.local_addr()` | The address actually used (port `0`) |
| `server.http3_addr()` | The UDP address of HTTP/3, if configured |
| `server.run().await` | Serves until `Ctrl+C` / `SIGTERM`, then stops cleanly |
| `server.with_graceful_shutdown(signal).await` | Serves until `signal` completes, instead of `Ctrl+C` / `SIGTERM`; in-progress requests get 10 s to finish |

See [Server configuration](server.md).

## HTTP/3

| Usage | Description |
|---|---|
| `app.http3(Http3::from_pem_files("fullchain.pem", "privkey.pem")?)` | HTTP/3 on UDP, next to HTTP/1.1 (`vitesse::http3::Http3`) |
| `Http3::from_pem(chain, key)` | Certificate chain and private key from memory (PEM) |
| `Http3::from_rustls(config)` | Your own `rustls::ServerConfig` (TLS 1.3; `h3` ALPN added if missing) |
| `.port(443)` | UDP port (default: the same number as the TCP port) |
| `.alt_svc(false)` | No `alt-svc` header on HTTP/1.1 responses (enabled by default) |
| `.alt_svc_port(443)` | Public port announced in `alt-svc` (behind a proxy, Docker or NAT) |
| `req.version()` | `HTTP/3.0` for a request received over HTTP/3 |

Same routes, middleware and handlers in both protocols; request bodies are read in full before the handler. See [HTTP/3 and QUIC](http3.md).

## Testing

| Usage | Description |
|---|---|
| `TestClient::new(app)` | An in-memory client (`vitesse::test::TestClient`) |
| `client.get(uri)`, `.post(uri)`, `.put(uri)`, `.patch(uri)`, `.delete(uri)` | Starts a request |
| `client.request(Method::HEAD, uri)` | Any method |
| `.header(name, value)`, `.body(data)` | Header, raw body |
| `.json(&value)`, `.form(&value)` | JSON body, URL-encoded form |
| `.await`, `.send().await` | Sends; returns a `TestResponse` |
| `res.status()`, `res.header(name)`, `res.headers()` | Status and headers |
| `res.text()`, `res.bytes()`, `res.json::<T>()` | The body |
| `client.get("/ws-route")` | On a WebSocket route: only the handshake response (`426` for plain HTTP); test conversations on a real port |

See [Testing](testing.md).

## Re-exports

| Item | Description |
|---|---|
| `vitesse::Bytes` | The `bytes::Bytes` type |
| `vitesse::http`, `HeaderMap`, `Method`, `StatusCode`, `header` | The `http` crate and its common types |
| `vitesse::serde_json`, `json!` | The `serde_json` crate and its macro |
| `vitesse::tokio` | The `tokio` crate, with the features Vitesse uses |
| `vitesse::http3::rustls` | The `rustls` crate used by HTTP/3 (`http3` feature) |
| `vitesse::DEFAULT_BODY_LIMIT` | `1024 * 1024` bytes |
| `Handler`, `Middleware`, `IntoResponse`, `IntoStatus`, `HandlerExt`, `ListenAddr` | The public traits |
| `BoxFuture<T>`, `BoxError`, `Chained`, `Next`, `Server`, `SameSite`, `Cookie`, `Body` | The other public types |
