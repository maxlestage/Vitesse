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

`use vitesse::prelude::*;` imports `App`, `Router`, `Request`, `Response`, `Next`, `Error`, `Json`, `Html`, `Redirect`, `Cookie`, `Body`, `ServeDir`, `StatusCode`, `Method`, `IntoResponse`, `HandlerExt`, `json!`, the `res` module, the `middleware` module, the `sse` module (Server-Sent Events) and the `ws` module (WebSocket).

## Cargo features

| Feature | Default | Adds |
|---|---|---|
| `ws` | Enabled | `app.ws`, `router.ws` and the `vitesse::ws` module |
| `tls` | Disabled | `app.tls` and the `vitesse::tls` module (HTTPS) |
| `http2` | Disabled | HTTP/2: over TLS (ALPN) with `tls`, and in clear text (h2c, prior knowledge) |
| `http3` | Disabled | `app.http3`, `server.http3_addr` and the `vitesse::http3` module |
| `multipart` | Disabled | `req.multipart()` and the `vitesse::multipart` module |
| `compress` | Disabled | `middleware::compress()` and `middleware::decompress()` |
| `session` | Disabled | `app.cookie_secret`, signed cookies, `middleware::session` and the `vitesse::session` module |
| `templates` | Disabled | `app.views`, `app.templates`, `req.render` and the `vitesse::templates` module (minijinja) |
| `tower` | Disabled | `middleware::layer`, `app.into_service()` and the `vitesse::tower` module |

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tls", "http2"] } # adds HTTPS and HTTP/2
# vitesse = { git = "https://github.com/maxlestage/Vitesse", default-features = false } # without WebSocket
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
| `app.on_request(\|req: &mut Request\| …)` | Hook before the middleware and the routing (`onRequest`); `Err(error)` stops the request |
| `app.pre_handler(\|req: &mut Request\| …)` | Hook after routing, before the route's middleware and handler (`preHandler`) |
| `app.on_response(\|res: &mut Response\| …)` | Hook on every response before it is sent (`onSend`) |
| `app.register(plugin)` | Adds a plugin: a `FnOnce(&mut App)` or an `impl Plugin` (Fastify's `register`) |
| `app.openapi("/openapi.json", OpenApi::new(title, version))` | Serves the OpenAPI 3.1 document of the routes; `.swagger_ui("/docs")` adds Swagger UI |
| `app.openapi_document(&spec)` | The same document as a `serde_json::Value`, without serving it |
| `app.state(value)` | Registers a global state value, one per type (`app.locals`) |
| `app.body_limit(bytes)` | Maximum size of a body read in memory (default `DEFAULT_BODY_LIMIT`, 1 MiB); per route: `middleware::body_limit` |
| `app.trust_proxy(value)` | Trusted reverse proxies (`app.set('trust proxy')`): `false` (default), `true`, a number of hops, or `"loopback, 10.0.0.0/8"` |
| `app.case_sensitive(false)` | Routes match whatever the case, like Express's default (Vitesse's default: `true`) |
| `app.cookie_secret(secret)` | Secret that signs cookies and the session cookie, 32 bytes at least (`cookieParser(secret)`, `session` feature) |
| `app.views(dir)` | Folder of HTML templates for `req.render` (`app.set('views')`, `templates` feature) |
| `app.templates(env)` | Your own minijinja `Environment`: filters, functions, embedded templates (`templates` feature) |
| `app.workers(n)` | Number of threads for `run` (default: one per CPU) |
| `app.thread_per_core(bool)` | Thread-per-core mode for `run` (default `true`, Linux only) |
| `app.keep_alive(duration)` | Idle keep-alive connections closed after it (default 60 s); `Duration::ZERO` disables keep-alive |
| `app.header_timeout(duration)` | Time to send a complete request head, then `408` (default 30 s) |
| `app.shutdown_timeout(duration)` | Grace period for in-flight requests at graceful shutdown (default 10 s) |
| `app.max_connections(n)` | Open connections per accept loop; the others wait (default: unlimited) |
| `app.tls(config)` | Serves HTTPS, and HTTP/2 to the clients that offer it (`tls` feature) |
| `app.http3(config)` | Also serves the app over HTTP/3 (QUIC, UDP), next to HTTP/1.1 (`http3` feature) |
| `app.into_service()` | The app as a tower `Service` (`vitesse::tower::AppService`, `tower` feature) |
| `app.run_unix(path)`, `app.listen_unix(path).await` | Serve on a Unix domain socket instead of a TCP port (Unix only) |
| `app.bind_unix(path).await?` | Opens the Unix socket and returns a `UnixServer` (Unix only) |

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
| `.name("user")` | Names the route added just before, for `req.path_for` and `req.url_for`; names are global to the app, mount prefixes included |
| `.doc(Operation::new().summary(…))` | Describes the route added just before in the OpenAPI document (`vitesse::openapi`) |
| `app.group(prefix, \|group\| { … })` | A `Router` filled by the closure (or a `fn(&mut Router)`) and mounted under `prefix` |
| `app.param(name, mw)` | Middleware run before the handler of every route with the parameter `name` (Express's `app.param`) |
| `app.bind_param(name, \|req, value\| async { … })` | Route model binding: the loader's `Ok(T)` is in `req.get::<T>()`, its `Err` is the response |

| Pattern | Matches |
|---|---|
| `/users` | Exactly `/users` (and `/users/`) |
| `/users/:id` | One segment, read with `req.param("id")`; names use letters, digits and `_` |
| `/flights/:from-:to` | Several parameters in one segment, separated by text: `LAX-SFO` gives `from=LAX`, `to=SFO` |
| `/files/:name.:ext` | `archive.tar.gz` gives `name=archive`, `ext=tar.gz`: the first parameter takes as little as it can |
| `/api/v:version/status` | Text and a parameter in one segment: `/api/v2/status` |
| `r"/users/:id(\d+)"` | A parameter constrained by a regular expression, which must match the whole value |
| `/archive/:year?/:month?` | Optional parameters, at the end only: `/archive`, `/archive/2024`, `/archive/2024/05` (`req.param("month")` is `None` when absent) |
| `/files/*path` | The rest of the path, read with `req.param("path")`; also matches `/files` |
| `/*` | Every path; the parameter is named `*` |

Priority: static > constrained or composite segment > parameter > wildcard, so `r"/users/:id(\d+)"` and `/users/:name` can coexist. Parameters are percent-decoded (`%C3%A9` → `é`), matching is case-sensitive (unless `app.case_sensitive(false)`), a wrong method answers `405` with `Allow`, and an invalid or duplicate route, or an invalid regular expression, panics at startup. See [Routing](routing.md#path-parameters) and [Named routes](routing.md#named-routes-and-url-generation).

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
| `req.version()` | The HTTP version: `HTTP/1.1`, `HTTP/2.0`, `HTTP/3.0`… |
| `req.set_uri(uri)` | Rewrites the URI (URL rewriting in a global middleware) |
| `req.set_method(method)` | Changes the method before routing (method override, in a global middleware) |
| `req.params_as::<T>()?` | Every route parameter, deserialized into a struct (axum's `Path<T>`); `400` on failure |
| `req.valid_json::<T>().await?`, `req.valid_form::<T>().await?`, `req.valid_query::<T>()?` | Read, then check the `Validate` rules: `422` with the messages of each field |
| `req.route()` | The pattern of the matched route, mount prefix included (`Some("/users/:id")`); `None` if no route matches |
| `req.path_for("user", &[&42])?` | The path of a named route, values percent-encoded: `"/users/42"`; `500` for an unknown name or a missing value |
| `req.url_for("user", &[&42])?` | The same, with the protocol and host of the request: `"https://example.com/users/42"` |
| `req.header(name)` | A header as `Option<&str>`, case-insensitive (`req.get('host')`) |
| `req.header_all(name)` | Every value of a repeated header |
| `req.headers()`, `req.headers_mut()` | All headers (`HeaderMap`) |
| `req.content_type()` | The `Content-Type` |
| `req.is("json")` | Checks the body type: full type, subtype or `text/*` |
| `req.accepts(&["html", "json"])` | The best type for the `Accept` header (`req.accepts()`); `None`: answer `406` |
| `req.accepts_language(&["en", "fr"])` | The best language for `Accept-Language` (`req.acceptsLanguages()`) |
| `req.hostname()` | The requested host, without the port (`X-Forwarded-Host` behind a trusted proxy) |
| `req.host()` | The requested host, with its port |
| `req.protocol()` | `"https"` or `"http"` (`X-Forwarded-Proto` behind a trusted proxy) |
| `req.secure()` | `true` over TLS, HTTP/3, or `X-Forwarded-Proto: https` from a trusted proxy |
| `req.cookie(name)` | A cookie value (`req.cookies.name`) |
| `req.signed_cookie(name)` | A signed cookie's value, `Option<String>`; `None` if missing or tampered with (`req.signedCookies.name`, `session` feature) |
| `req.sign_cookie(cookie)` | Signs a cookie before `.cookie(...)`; panics without `app.cookie_secret` (`session` feature) |
| `req.session()` | The visitor's session, `&Session` (`req.session`); `500` without the `session` middleware |
| `req.render(name, context)` | Renders a template: `Result<Html<String>, Error>` (`res.render()`, `templates` feature) |
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
| `req.multipart()?` | A `multipart/form-data` body, field by field (`multipart` feature); `415` if not multipart |
| `req.body_limit()`, `req.set_body_limit(bytes)` | The body limit of this request; change it in a middleware |
| `req.take_body()` | The body as a stream (`Body`), not limited by `body_limit` |
| `req.set_body(body)` | Replaces the body |
| `req.state::<T>()` | Global state (`&'static T`); panics, hence `500`, if missing |
| `req.try_state::<T>()` | Same, as an `Option` |
| `req.set(value)` | Attaches data to the request (`res.locals`); the type must be `Clone` |
| `req.get::<T>()` | Reads data attached by a middleware |
| `req.extensions()`, `req.extensions_mut()` | The raw `http::Extensions` |
| `req.ip()` | Client IP; follows `X-Forwarded-For` behind a trusted proxy (`app.trust_proxy`) |
| `req.remote_addr()` | Address and port of the connection itself (`None` on a Unix socket) |

The body is read on demand and cached: the reading methods can be called more than once. See [Requests](requests.md).

### Multipart forms (`multipart` feature)

| Usage | Description |
|---|---|
| `let mut form = req.multipart()?;` | Reads the form as it arrives; the whole body is limited by `req.body_limit()` (`413`) |
| `while let Some(field) = form.next_field().await? { … }` | The fields, in order; read (or drop) one before asking for the next |
| `field.name()` | The input's name (`<input name="avatar">`) |
| `field.file_name()` | The client's file name, for a file: never use it as a path as is |
| `field.content_type()`, `field.headers()` | The field's `Content-Type`, all its headers |
| `field.chunk().await?` | The next chunk of the content; `None` at the end |
| `field.bytes().await?`, `field.text().await?` | The whole content in memory, as bytes or UTF-8 text |
| `field.save(path).await?` | Streams the content to a file and returns its size (`u64`); the partial file is removed if the upload fails |

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
| `cookie.name()`, `cookie.value()`, `cookie.with_value(v)` | Read a cookie; the same cookie with another value |
| `Body::empty()`, `Body::from(x)` | Empty body, or from `&'static str`, `String`, `Vec<u8>`, `Bytes`… |
| `Body::from_stream(stream)` | Streamed (`chunked`) body from a `Stream` of `Result<impl Into<Bytes>, E>` |
| `Body::wrap(body)` | Wraps any `http_body::Body` (proxies) |
| `body.size()`, `body.to_bytes().await` | Size if known; reads everything into memory |

### Server-Sent Events (`vitesse::sse`)

| Usage | Description |
|---|---|
| `let (tx, stream) = sse::channel(16);` | Return `stream` from the handler, send events from a `tokio::spawn`ed task |
| `tx.send(event).await` | Sends an event; `Err(Closed)` once the browser has gone away |
| `tx.try_send(event)` | Without waiting: `Err(TrySendError::Full(event))` if the buffer is full, `Err(TrySendError::Closed(event))` if the browser left |
| `tx.is_closed()`, `tx.closed().await` | Has the browser gone away? Wait until it does; `Sender` is `Clone` |
| `Sse::new(stream)` | A response from any `Stream` of `Result<Event, E>` |
| `.keep_alive(KeepAlive::new().interval(d).text("ping"))`, `.no_keep_alive()` | Keep-alive comment on a quiet connection (default: every 15 s) |
| `Event::new().data("…")` | An event; multi-line data becomes several `data:` lines |
| `.json_data(&value)?` | Data serialised as JSON |
| `.event("tick")`, `.id("42")`, `.retry(duration)`, `.comment("…")` | Event type, id (`Last-Event-ID`), reconnection delay, comment |
| `event.to_string()` | The event in the wire format |

The response has `content-type: text/event-stream` and `cache-control: no-cache`, and ends when every `Sender` is dropped. See [Responses](responses.md#server-sent-events).

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
| `.with_details(json!({ … }))` | Structured data sent next to the message: `{"error":"…","details":{…}}`; read with `err.details()` |
| `err.status()`, `err.message()`, `err.source()` | Read the error |
| `vitesse::Result<T>` | Alias for `Result<T, vitesse::Error>` |
| `?` on any standard error | `500 {"error":"Internal Server Error"}`; the cause is only logged |
| Panic in a handler or a middleware | `500`; the server keeps running |

See [Errors](errors.md).

## Validation

| Usage | Description |
|---|---|
| `impl Validate for T { fn validate(&self, v: &mut Validator) { … } }` | The rules of a type (`vitesse::validate`) |
| `v.field("name", &self.name).not_empty().max_length(50)` | Rules of a field; it stops at its first failing rule |
| `v.optional(name, &opt)`, `v.required(name, &opt)` | An `Option`: rules skipped when `None`, or "is required" |
| `.min_length(n)`, `.max_length(n)`, `.not_empty()` | Characters of a string, items of a collection |
| `.min(x)`, `.max(x)`, `.between(a, b)`, `.one_of(&[…])` | Bounds and choices |
| `.email()`, `.url()`, `.alphanumeric()` | Formats of a string |
| `.check(\|v\| bool, "message")`, `.message("…")` | A rule of your own; replaces the message of the rule just before |
| `v.nested("address", &self.address)`, `v.each("items", &self.items)` | Nested values: `address.city`, `items[0].name` |
| `v.error(name, message)` | A problem reported directly (rules across fields) |

See [Validation](validation.md) and [OpenAPI](openapi.md).

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
| `middleware::body_limit(bytes)` | `express.json({ limit })` | Body limit for a route (`.with()`) or a router |
| `middleware::request_id()` | `express-request-id` | Id in `X-Request-Id` (kept from the request, or a new UUID), in `req.get::<RequestId>()` and on the response |
| `.header("x-correlation-id")`, `.trust_incoming(false)` | | Another header; always generate a new id |
| `middleware::basic_auth(("admin", "secret"))` | `express-basic-auth` | `401` + `WWW-Authenticate` without valid credentials; also a closure `\|user, password\| -> bool` or a list of pairs |
| `.realm("Admin")` | | The realm shown by the browser (default `Restricted`) |
| `req.get::<middleware::BasicUser>()` | | The authenticated user name |
| `middleware::bearer_auth(token)` | | `Authorization: Bearer …`; a `&'static str`, a `String`, a list, or a closure `\|token\| -> bool` |
| `middleware::constant_time_eq(a, b)` | | Compares two secrets in constant time |
| `middleware::concurrency_limit(n)` | | At most `n` requests in flight; the others wait |
| `.max_wait(duration)` | | `503` + `Retry-After` beyond this wait (`Duration::ZERO`: right away) |
| `middleware::rate_limit(max, period)` | `express-rate-limit` | `max` requests per `period` per IP, then `429` + `Retry-After`; `X-RateLimit-Limit` and `X-RateLimit-Remaining` headers |
| `.key(\|req: &Request\| -> Option<String>)` | `keyGenerator` | Another client key (API key, user id); `None`: not limited |
| `middleware::session(store)` | `express-session` | Server-side sessions, read with `req.session()` (`session` feature); see [below](#signed-cookies-and-sessions-session-feature) |
| `middleware::compress()` | `compression()` | Brotli or gzip responses (`compress` feature) |
| `middleware::decompress()` | | Decompresses `gzip`, `br` and `deflate` request bodies (`compress` feature) |
| `middleware::layer(tower_layer)` | | Any tower / tower-http `Layer` (`tower` feature) |
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
| `router.fallback(handler)` | Answers the paths under the router's prefix that no route matches, instead of the app's fallback |
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

## Signed cookies and sessions (`session` feature)

| Usage | Description |
|---|---|
| `app.cookie_secret(secret)` | The secret (`&str`, `String` or bytes), 32 bytes at least or it panics; read it from an environment variable |
| `.cookie(req.sign_cookie(Cookie::new("theme", "dark")))` | Sends `theme=dark.<signature>`: readable but not modifiable, not encrypted (`res.cookie(…, { signed: true })`) |
| `req.signed_cookie("theme")` | `Some("dark")`, or `None` if missing or changed (`req.signedCookies.theme`) |
| `app.middleware(middleware::session(MemoryStore::new()))` | Server-side sessions (`express-session`), with the id in the `vitesse.sid` cookie |
| `.cookie_name("sid")`, `.max_age(duration)`, `.rolling(true)` | Cookie name; lifetime after the last change (default 1 day); extend it on every request |
| `.secure(bool)`, `.same_site(SameSite::Strict)`, `.domain(d)`, `.path(p)` | Cookie attributes; by default `HttpOnly`, `SameSite=Lax`, `Path=/`, and `Secure` over HTTPS |
| `req.session().get::<T>(key)` | A value, deserialised: `Option<T>` |
| `session.insert(key, value)?` | Stores any `Serialize` value |
| `session.remove(key)`, `session.contains(key)` | Removes a value and returns it; checks a key |
| `session.clear()`, `session.is_empty()` | Removes every value; checks whether there is none |
| `session.regenerate()` | A new id, the same data: call it at login |
| `session.destroy()` | Deletes the session, in the store and in the browser: logout |
| `session.id()` | The id, `None` until the session is first saved |
| `vitesse::session::MemoryStore` | Sessions in the memory of the process: lost on restart, not shared between instances |
| `impl SessionStore for MyStore` | `load(id)`, `save(id, &record, ttl)` and `delete(id)`, each returning a `BoxFuture`; a `Record` is a `serde_json::Map` |

A session is saved, and its cookie sent, only when a handler changes it (or on every request with `rolling(true)`); the cookie is signed when the app has a `cookie_secret`. See [Signed cookies and sessions](sessions.md).

## HTML templates (`templates` feature)

| Usage | Description |
|---|---|
| `app.views("templates")` | The templates' folder (`app.set('views')`); each template is read on first use, then cached |
| `req.render("users.html", json!({ … }))` | Renders a template (`res.render()`): `Result<Html<String>, Error>`; any `Serialize` value as context |
| `app.templates(env)` | Your own `vitesse::templates::minijinja::Environment` instead of `app.views` |
| `env.add_filter(name, f)`, `env.add_function(name, f)`, `env.add_global(name, value)` | Filters, functions and variables of your own |
| `env.add_template(name, include_str!("../templates/x.html"))` | A template embedded in the binary |
| `{{ value }}`, `{{ value\|upper }}` | A value, escaped in `.html`, `.htm` and `.xml` templates; a filter |
| `{% for x in list %}…{% else %}…{% endfor %}`, `{% if %}…{% elif %}…{% endif %}` | Loops and conditions |
| `{% extends "layout.html" %}`, `{% block content %}…{% endblock %}` | Layouts |
| `{% include "partials/nav.html" %}`, `{% macro %}`, `{% import %}` | Partials and macros |
| `{{ html\|safe }}` | Trusted HTML, not escaped |

A missing template or a rendering error gives a `500` (the cause is logged); restart the server to see a template change. See [HTML templates](templates.md).

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

`WebSocket` implements `Stream` and `Sink`; global and router middleware run on the handshake. WebSockets work over HTTP/1.1 and, with the `http2` feature, over HTTP/2 (RFC 8441), but not over HTTP/3. See [WebSocket](websocket.md).

## Server

| Usage | Description |
|---|---|
| `3000`, `"3000"` | Listens on `0.0.0.0:3000` |
| `"127.0.0.1:8080"`, `"[::]:3000"`, `"localhost:3000"` | Listens on that address |
| `String`, `SocketAddr`, `([127, 0, 0, 1], 8080)` | Other accepted forms (`ListenAddr`) |
| `server.local_addr()` | The address actually used (port `0`) |
| `server.http3_addr()` | The UDP address of HTTP/3, if configured |
| `server.run().await` | Serves until `Ctrl+C` / `SIGTERM`, then stops cleanly |
| `server.with_graceful_shutdown(signal).await` | Serves until `signal` completes, instead of `Ctrl+C` / `SIGTERM`; in-progress requests get `shutdown_timeout` (10 s) to finish |
| `unix_server.path()` | The socket file (`UnixServer`), e.g. to change its permissions |
| `unix_server.run().await`, `unix_server.with_graceful_shutdown(signal).await` | Same as `Server`; the socket file is removed when it stops |

See [Server configuration](server.md).

## HTTPS and HTTP/2

| Usage | Description |
|---|---|
| `app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?)` | HTTPS, TLS 1.2 and 1.3 (`vitesse::tls::Tls`, `tls` feature) |
| `Tls::from_pem(chain, key)` | Certificate chain and private key from memory (PEM) |
| `Tls::from_rustls(config)` | Your own `rustls::ServerConfig` (mutual TLS, several domains…); ALPN `h2` and `http/1.1` added if it lists none |
| `tls.config()` | The rustls configuration (`&Arc<rustls::ServerConfig>`) |
| `http2` feature | HTTP/2 over TLS (browsers, ALPN), and h2c with prior knowledge on a plain port |
| `req.version() == Version::HTTP_2` | A request received over HTTP/2 (`vitesse::http::Version`) |

Same routes, middleware and handlers in every protocol. With the `ws` feature, browsers also open WebSockets over HTTP/2 (RFC 8441), on the same connection as the page; `app.ws` handlers don't change. See [HTTPS and HTTP/2](https.md).

## Trusted proxies

| Usage | Description |
|---|---|
| `app.trust_proxy(false)` | Nobody (default): `req.ip()` is the address of the connection |
| `app.trust_proxy("loopback")` | Proxies at these addresses or networks: IPs, CIDR (`10.0.0.0/8`), `loopback`, `linklocal`, `uniquelocal`, separated by commas |
| `app.trust_proxy(1)` | The `n` proxies closest to the app, whatever their address |
| `app.trust_proxy(true)` | Every proxy: the client is the left-most address of `X-Forwarded-For` |
| `vitesse::TrustProxy` | `None`, `All`, `Hops(n)`, `Networks(Vec<Network>)`; `"…".parse::<TrustProxy>()` to handle errors |
| `trust.trusts(ip, hop)`, `network.contains(ip)` | Checks an address (`vitesse::Network`: `"10.0.0.0/8".parse()`) |

Effects: `req.ip()` (`X-Forwarded-For`), `req.protocol()` and `req.secure()` (`X-Forwarded-Proto`), `req.host()` and `req.hostname()` (`X-Forwarded-Host`). See [Going to production](production.md#getting-the-clients-ip-address).

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

Same routes, middleware and handlers in both protocols; request bodies are streamed to the handler, with the same body limits. With `app.tls(...)` too, HTTPS responses carry `alt-svc` and browsers switch to HTTP/3 directly. See [HTTP/3 and QUIC](http3.md).

## tower (`tower` feature)

| Usage | Description |
|---|---|
| `app.middleware(middleware::layer(CorsLayer::permissive()))` | A tower `Layer` as a middleware: also with `router.middleware` and `handler.with` |
| `vitesse::tower::layer(l)` | Same as `middleware::layer` |
| `app.into_service()` | The app as a `Service<http::Request<B>>` (`AppService`, `Clone` + `Copy`): hyper, axum's `Router::nest_service`, `ServiceExt::oneshot` |
| `vitesse::tower::{Layer, Service}` | The tower traits |
| `vitesse::tower::NextService` | The rest of the Vitesse chain, the innermost service of a layer |

The layer can change the method, URI, headers, extensions and body; its errors become `500`. WebSocket upgrades are not available through `into_service` (`501`). See [Middleware](middleware.md#tower-layers).

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
| `.header("cookie", "vitesse.sid=…")` | Sends back a cookie read from `res.header("set-cookie")`: there is no cookie jar (sessions, signed cookies) |
| `app.into_service().oneshot(request).await` | Tests through tower (`tower` feature and tower's `ServiceExt`) |

See [Testing](testing.md).

## Re-exports

| Item | Description |
|---|---|
| `vitesse::Bytes` | The `bytes::Bytes` type |
| `vitesse::http`, `HeaderMap`, `Method`, `StatusCode`, `header` | The `http` crate and its common types |
| `vitesse::serde_json`, `json!` | The `serde_json` crate and its macro |
| `vitesse::tokio` | The `tokio` crate, with the features Vitesse uses |
| `vitesse::http3::rustls`, `vitesse::tls::rustls` | The `rustls` crate used by HTTP/3 and HTTPS (`http3` and `tls` features) |
| `vitesse::templates::minijinja` | The `minijinja` crate (`templates` feature) |
| `vitesse::session::{Session, Sessions, SessionStore, MemoryStore, Record}` | The session types (`session` feature) |
| `vitesse::DEFAULT_BODY_LIMIT` | `1024 * 1024` bytes |
| `vitesse::VERSION` | The version of the crate (`"0.1.0"`) |
| `Handler`, `Middleware`, `IntoResponse`, `IntoStatus`, `HandlerExt`, `ListenAddr` | The public traits |
| `BoxFuture<T>`, `BoxError`, `Chained`, `Next`, `Server`, `UnixServer`, `SameSite`, `Cookie`, `Body`, `TrustProxy`, `Network` | The other public types |
