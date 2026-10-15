# Middleware

A middleware is a function that runs around your handlers: it receives the request and `next`, the rest of the chain, and can act before the handler, after it, or answer on its own. This page shows how to write them, in which order they run, documents the middleware that ships with Vitesse, and shows how to use the tower layers of the axum ecosystem.

## Your first middleware

In Express, a middleware receives `(req, res, next)` and calls `next()`. In Vitesse, it receives `(req, next)`, passes the request on with `next.run(req).await`, and **returns** the response:

```js
app.use((req, res, next) => {
  console.log(`${req.method} ${req.path}`);
  next();
});
```

```rust
use vitesse::prelude::*;

let mut app = App::new();
app.middleware(|req: Request, next: Next| async move {
    println!("{} {}", req.method(), req.path());
    next.run(req).await
});
```

`next.run(req)` takes ownership of the request and gives back the `Response` produced by the rest of the chain. Because it consumes `next`, it can only be called once: the compiler rules out "called `next()` twice" bugs.

## Before and after the handler

Code before `next.run` runs on the way in, code after it runs on the way out, with the response in hand:

```rust
use std::time::Instant;

app.middleware(|req: Request, next: Next| async move {
    let start = Instant::now();
    let res = next.run(req).await;
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    res.header("x-response-time", format!("{ms:.3}ms"))
});
```

To change the response, use the builder methods (`.header(...)`, `.status(...)`) or the in-place ones on a `mut` response (`set_header`, `set_status`, `headers_mut()`), described in [Sending responses](responses.md).

## Short-circuiting

To stop the chain, return a response without calling `next.run`:

```rust
app.middleware(|req: Request, next: Next| async move {
    if req.header("x-api-key") != Some("secret") {
        return Error::unauthorized("missing API key").into_response();
    }
    next.run(req).await
});
```

A middleware can return anything that implements `IntoResponse`, including `vitesse::Result<Response>`, which lets you use `?`:

```rust
async fn require_json(req: Request, next: Next) -> vitesse::Result<Response> {
    if req.method() == Method::POST && !req.is("json") {
        return Err(Error::new(415, "send JSON"));
    }
    Ok(next.run(req).await)
}

app.middleware(require_json);
```

## Changing the request

Take the request as `mut req` to modify it before passing it on:

- `req.set(value)` attaches data that handlers read with `req.get::<T>()` (the equivalent of `res.locals` or `req.user`; the type must implement `Clone`). See [Shared state](state.md).
- `req.headers_mut()` gives access to the headers, `req.set_body(...)` replaces the body, and `req.set_body_limit(...)` changes how large a body the handler may read (see [Body size limit](requests.md#body-size-limit-and-413)).
- `req.set_method(...)` changes the method, for a [method override](requests.md#method-override).
- `req.set_uri(...)` rewrites the URL. Global middleware runs *before* routing, so the new path is the one that gets routed:

```rust
app.middleware(|mut req: Request, next: Next| async move {
    if req.path().starts_with("/old/") {
        let new = req.uri().to_string().replacen("/old/", "/new/", 1);
        if let Ok(uri) = new.parse() {
            req.set_uri(uri);
        }
    }
    next.run(req).await
});
```

A middleware can also read the body (`req.bytes().await`, `req.json().await`…), for example to check a webhook signature: the body is cached, so the handler can read it again.

## Execution order

Global middleware runs in the order it was added, like layers of an onion:

```rust
app.middleware(a);
app.middleware(b);
app.get("/", handler);
// a (before) → b (before) → handler → b (after) → a (after)
```

The full chain for a request is: **global middleware → [router](routers.md) middleware → route middleware (`.with`) → handler**. Router middleware also runs, after global middleware, for the requests under the router's prefix that no route answers: the `404`, the `405` and the automatic `OPTIONS` (see [Routers](routers.md#requests-that-no-route-answers)).

> [!IMPORTANT]
> Unlike Express's `app.use()`, the position of `app.middleware(...)` relative to your routes doesn't matter: global middleware always wraps **every** request, including routes declared before it, `404`s and `405`s. The [`app.on_error`](errors.md) handler is always the outermost layer.

## Lifecycle hooks

Hooks are plain functions that run at a given point of every request, like Fastify's hooks. They don't return a future, which makes them lighter than a middleware for what they do: look at or change the request or the response.

| Hook | Runs | Can |
|---|---|---|
| `app.on_request(\|req: &mut Request\| …)` | First, before the middleware and the routing (Fastify's `onRequest`) | Change the request; stop it with `Err(error)` |
| `app.pre_handler(\|req: &mut Request\| …)` | After routing, just before the route's middleware and handler (`preHandler`): `req.route()` and the parameters are known | Same |
| `app.on_response(\|res: &mut Response\| …)` | On every response, before it is sent (`onSend`) | Add headers, change the status… |

```rust
app.on_request(|req: &mut Request| {
    if req.header("x-api-key") != Some("secret") {
        return Err(Error::unauthorized("missing API key"));
    }
    req.set(std::time::Instant::now());
    Ok(())
});

app.pre_handler(|req: &mut Request| {
    println!("{} {}", req.method(), req.route().unwrap_or("?"));
    Ok(())
});

app.on_response(|res: &mut Response| {
    res.set_header("x-powered-by", "Vitesse");
});
```

- An error returned by `on_request` or `pre_handler` becomes the response, through `app.on_error` and the `on_response` hooks.
- `on_response` sees every response: routes, `404`s, errors (before `app.on_error` rewrites them: `res.error()` is still there).
- `pre_handler` runs for every route, those of mounted routers included, but not for the `404`, `405` and automatic `OPTIONS` responses, which have no route.
- Hooks of a kind run in the order they were added; a panic in a hook becomes a `500`.
- Without hooks, nothing is added to the request's path. To run code *around* the handler (timing, a `tokio::time::timeout`…), or asynchronously, use a middleware.

### Async hooks

`app.on_request_async` and `app.pre_handler_async` are the versions of `on_request` and `pre_handler` for hooks that wait on something (a cache, a database, another service). They receive the request and give it back, or stop it with an error:

```rust
app.pre_handler_async(|mut req: Request| async move {
    if let Some(slug) = req.param("tenant").map(str::to_owned) {
        let tenant = load_tenant(&slug).await.ok_or_else(|| Error::not_found("no such tenant"))?;
        req.set(tenant);
    }
    Ok(req)
});
```

Async hooks run after the plain hooks of the same kind, in the order they were added. Routers and groups have hooks of their own, which only cover them: see [Routers](routers.md).

## Middleware for some routes only

There are three ways to limit a middleware to some routes:

- **One route**: `.with(middleware)` (from the `HandlerExt` trait, included in the prelude). Calls can be chained, and run from left to right. Wrap a closure in parentheses before calling `.with`:

```rust
#[derive(Clone)]
struct CurrentUser {
    name: String,
}

async fn auth(mut req: Request, next: Next) -> Response {
    match req.header("authorization") {
        Some("Bearer secret") => {
            req.set(CurrentUser { name: "ada".into() });
            next.run(req).await
        }
        _ => Error::unauthorized("please log in").into_response(),
    }
}

async fn dashboard(req: Request) -> String {
    let user = req.get::<CurrentUser>().unwrap();
    format!("Hello {}", user.name)
}

app.get("/dashboard", dashboard.with(auth));
app.get("/health", (|_| async { "ok" }).with(auth));
```

- **A group of routes**: put them in a `Router` and call `router.middleware(...)`. Like `router.use()` in Express, it covers the router's whole prefix, `404`s included (see [Routers](routers.md)).
- **A path prefix**: test the path in a global middleware:

```rust
app.middleware(|req: Request, next: Next| async move {
    if req.path().starts_with("/admin") && req.header("x-admin") != Some("1") {
        return Error::forbidden("admins only").into_response();
    }
    next.run(req).await
});
```

## Middleware groups: `Stack` and `middleware::group`

A `Stack` bundles several middlewares into one, to use them together on the application, a router, a group or a single route:

```rust
use std::time::Duration;
use vitesse::middleware::Stack;

let api = Stack::new()
    .with(middleware::cors())
    .with(middleware::rate_limit(100, Duration::from_secs(60)));

app.group("/api", |r| {
    r.middleware(api.clone());
    r.get("/users", list_users);
});
app.post("/webhook", webhook.with(api));
```

Like Laravel's `web` and `api` groups, a stack can be named with `app.middleware_group`, then used by its name with `middleware::group`, even in a module that has no access to the application:

```rust
app.middleware_group("web", Stack::new().with(middleware::helmet()).with(middleware::request_id()));
app.middleware_group("api", Stack::new().with(middleware::cors()).with(middleware::bearer_auth("secret")));
app.group("/admin", admin::routes);

// src/admin.rs
pub fn routes(r: &mut Router) {
    r.middleware(middleware::group("web"));
    r.get("/dashboard", dashboard);
}
```

The middlewares of a stack run in order, exactly as if they had been added one by one. The name is looked up on the first request, then remembered; an unknown name answers `500` and is logged.

## Built-in middleware

The `vitesse::middleware` module (available as `middleware::` with the prelude) provides the classics:

| Middleware | Express equivalent | Role |
|---|---|---|
| `middleware::logger()` | `morgan('dev')` | logs each request |
| `middleware::cors()` | `cors()` | CORS headers and preflight requests |
| `middleware::helmet()` | `helmet()` | security headers |
| `middleware::timeout(duration)` | `connect-timeout` | `503` when a request takes too long |
| `middleware::body_limit(bytes)` | `express.json({ limit })` | body size limit for a route or a router |
| `middleware::request_id()` | `express-request-id` | an id for each request, in `X-Request-Id` |
| `middleware::basic_auth(credentials)` | `express-basic-auth` | HTTP Basic authentication (user name and password) |
| `middleware::bearer_auth(token)` | `passport-http-bearer` | `Authorization: Bearer` tokens |
| `middleware::session(store)` | `express-session` | server-side sessions, read with `req.session()` (feature `session`) |
| `middleware::concurrency_limit(max)` | | caps the number of requests handled at once |
| `middleware::rate_limit(max, period)` | `express-rate-limit` | requests per client and period, `429` beyond |
| `middleware::etag()` | Express's `etag` setting | `ETag` and `304 Not Modified` for the responses in memory |
| `middleware::cache(ttl)` | `apicache` | a server-side cache of whole responses, with a time to live |
| `middleware::serve_static(dir)` | `express.static(dir)` | static files, see [Static files](static-files.md) |
| `middleware::compress()` | `compression()` | Brotli or gzip responses (feature `compress`) |
| `middleware::decompress()` | | compressed request bodies (feature `compress`) |
| `middleware::layer(layer)` | | any tower layer (feature `tower`), see [Tower layers](#tower-layers) |

### `logger`

Prints one line per request on standard output: method, URL, status and duration. The status is colored when the output is a terminal.

```rust
app.middleware(middleware::logger());
```

```text
GET /users/42 200 0.084 ms
```

Add it first so that it measures the whole chain.

### `cors`

With no options, `middleware::cors()` behaves like Express's `cors()`: every origin is allowed (`Access-Control-Allow-Origin: *`) for the methods `GET, HEAD, PUT, PATCH, POST, DELETE`. Each option can be chained:

```rust
use std::time::Duration;

app.middleware(
    middleware::cors()
        .allow_origin("https://app.example.com")
        .allow_origin("https://admin.example.com")
        .allow_methods([Method::GET, Method::POST])
        .allow_headers("content-type, authorization")
        .expose_headers("x-total-count")
        .allow_credentials(true)
        .max_age(Duration::from_secs(600)),
);
```

| Option | Effect |
|---|---|
| `allow_origin(origin)` | only allows the listed origins (can be called several times; `"*"` keeps every origin) |
| `allow_methods(methods)` | methods sent in `Access-Control-Allow-Methods` |
| `allow_headers(list)` | `Access-Control-Allow-Headers` (by default, echoes the headers requested by the browser) |
| `expose_headers(list)` | response headers that JavaScript is allowed to read |
| `allow_credentials(true)` | adds `Access-Control-Allow-Credentials: true` (cookies and authentication), to combine with `allow_origin` |
| `max_age(duration)` | how long the browser can cache the preflight response |

When the request's origin is in the list, Vitesse echoes it in `Access-Control-Allow-Origin` and adds `Vary: Origin`. Without any `allow_origin`, the answer is always `*`, even with `allow_credentials(true)`, exactly like Express.

> [!IMPORTANT]
> Browsers refuse credentialed requests (cookies, `Authorization`) when the answer is `*`. To use cookies or authentication across origins, list your trusted origins explicitly with `.allow_origin(...)`, once per origin. Vitesse deliberately never echoes an arbitrary origin: that would let any website read the responses of a logged-in user.

Requests without an `Origin` header pass through untouched. Preflight requests (`OPTIONS` with `Access-Control-Request-Method`) are answered directly with a `204`, without reaching your routes. Add `cors()` to the app or to a [router](routers.md), which also covers the preflights of its routes; on a single route (`.with`), it never sees the preflight, which the automatic `OPTIONS` answers without CORS headers. When an origin isn't allowed, Vitesse doesn't add any CORS header and the browser blocks the response: CORS protects users' browsers, it doesn't replace authentication.

> [!TIP]
> Add `cors()` before your authentication middleware: that way, error responses (a `401`, for example) also carry the CORS headers, and your front-end JavaScript can read them.

### `helmet`

Adds the usual security headers, without overwriting the ones your handler already set:

```text
x-content-type-options: nosniff
x-frame-options: SAMEORIGIN
x-dns-prefetch-control: off
x-download-options: noopen
x-permitted-cross-domain-policies: none
x-xss-protection: 0
referrer-policy: no-referrer
cross-origin-opener-policy: same-origin
strict-transport-security: max-age=31536000; includeSubDomains
```

### `timeout`

Cancels requests that take longer than the given duration and answers `503 {"error":"request timed out"}`:

```rust
app.middleware(middleware::timeout(Duration::from_secs(10)));
```

The handler's future is dropped at its next `.await`. Code that blocks the thread without ever awaiting (a long computation, `std::thread::sleep`) can't be interrupted.

### `body_limit`

Changes the maximum size of the body that handlers can read, for the routes it covers: one route with `.with`, or a whole router. The rest of the app keeps `app.body_limit` (1 MiB by default):

```rust
app.post("/upload", upload.with(middleware::body_limit(100 * 1024 * 1024))); // 100 MiB
```

Beyond the limit, reading the body fails with `413 Payload Too Large`. Everything about it is in [Body size limit and 413](requests.md#body-size-limit-and-413).

### `request_id`

Gives each request an id, to follow it through your logs and from one service to the next:

```rust
use vitesse::middleware::RequestId;

app.middleware(middleware::request_id());
app.get("/", |req: Request| async move {
    let id = req.get::<RequestId>().unwrap();
    println!("[{id}] home page");
    "hello"
});
```

- An `X-Request-Id` received with the request (set by your proxy, or by the service that calls you) is kept if it looks valid: 1 to 200 visible ASCII characters. Otherwise, Vitesse generates a UUID v4 (`9b2f3c1e-4a7d-4e1b-…`).
- The id is put in the request's header (forward it when you call other services), in `req.get::<RequestId>()` (`RequestId(pub String)`, which prints with `{}`) and in the response's header, `404`s and errors included.
- `.header("x-correlation-id")` uses another header. `.trust_incoming(false)` always generates a new id: useful when clients reach the app directly and you don't want their ids in your logs.

Add it before the middlewares that log, so that they can read the id. It identifies requests, it isn't a secret: don't use it as a token.

### `basic_auth` and `bearer_auth`

HTTP authentication, for a back office or an internal API. `basic_auth` asks for a user name and a password:

```rust
use vitesse::middleware::BasicUser;

let password = std::env::var("ADMIN_PASSWORD").expect("ADMIN_PASSWORD is not set");

let mut admin = Router::new();
admin.middleware(middleware::basic_auth(("admin".to_string(), password)).realm("Back office"));
admin.get("/", |req: Request| async move {
    let user = req.get::<BasicUser>().unwrap();
    format!("Hello {}", user.0)
});
app.mount("/admin", admin);
```

Without valid credentials, the answer is a `401` with `WWW-Authenticate: Basic realm="Back office", charset="UTF-8"`, and the browser shows its login dialog. Once logged in, the user name is in `req.get::<BasicUser>()`. The credentials can be:

| Credentials | Example |
|---|---|
| a pair | `("admin", "secret")`, with `&'static str` or `String` |
| several pairs | `[("admin", "s3cret"), ("ada", "lovelace")]`, or a `Vec` of pairs |
| a closure | `\|user: &str, password: &str\| user == "ada" && check(password)` |

`bearer_auth` checks an `Authorization: Bearer <token>` header, for APIs:

```rust
let token = std::env::var("API_TOKEN").expect("API_TOKEN is not set");

let mut api = Router::new();
api.middleware(middleware::bearer_auth(token));
api.get("/stats", |_| async { Json(json!({ "users": 42 })) });
app.mount("/api", api);
```

It accepts a token (`&'static str` or `String`), an array or a `Vec` of tokens, or a closure `|token: &str| -> bool`. Without a token, the answer is a `401` with `WWW-Authenticate: Bearer`; with a wrong one, `WWW-Authenticate: Bearer error="invalid_token"`.

- Passwords and tokens are compared in constant time, so that the response time doesn't give them away. `middleware::constant_time_eq(a, b)` does the same in your own code.
- The closures are synchronous: to look users up in a database, write your own middleware (see [Short-circuiting](#short-circuiting)).
- On a router, the authentication also covers the `404`s under its prefix: a visitor without credentials doesn't learn which pages exist.

> [!IMPORTANT]
> Basic authentication sends the password with every request, merely encoded in base64, and a token can be reused by anyone who sees it: only use them over HTTPS ([HTTPS and HTTP/2](https.md), or a proxy that terminates TLS).

### `session`

Server-side sessions, like `express-session`. It needs the `session` Cargo feature:

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["session"] }
```

```rust
use vitesse::session::MemoryStore;

app.middleware(middleware::session(MemoryStore::new()));
app.get("/visits", |req: Request| async move {
    let session = req.session();
    let visits = session.get::<u32>("visits").unwrap_or(0) + 1;
    session.insert("visits", visits)?;
    Ok::<_, Error>(format!("visit #{visits}"))
});
```

- On the way in, it loads the visitor's session from the store, using the id in the `vitesse.sid` cookie. On the way out, it saves the session and sends the cookie, only if something changed: visitors who never write to their session get no cookie.
- Handlers, and the middlewares that run after it, reach the session with `req.session()`, which answers `500` where the middleware isn't installed. Add it with `app.middleware`, before an authentication middleware that reads the session.
- The cookie is `HttpOnly`, `SameSite=Lax`, `Secure` over HTTPS, and signed when the app has a secret (`app.cookie_secret(...)`). `.cookie_name(...)`, `.max_age(...)` (1 day by default), `.rolling(true)`, `.secure(...)`, `.same_site(...)`, `.domain(...)` and `.path(...)` change it.
- `MemoryStore` keeps sessions in the memory of the process; implement the `SessionStore` trait to keep them in Redis or a database.

Logging in and out, the options, stores and CSRF are covered in [Signed cookies and sessions](sessions.md).

### `concurrency_limit`

Caps the number of requests handled at the same time by what it covers, to protect an expensive route or a fragile database:

```rust
use std::time::Duration;

// At most 4 reports at once; the others wait up to 2 s, then get a 503.
app.get(
    "/report",
    report.with(middleware::concurrency_limit(4).max_wait(Duration::from_secs(2))),
);
```

- At most `max` requests are inside at the same time; the next ones wait for their turn.
- `.max_wait(duration)` caps that wait: beyond it, the answer is `503 Service Unavailable` with `Retry-After: 1`. With `Duration::ZERO`, requests are turned away as soon as the limit is reached, which keeps the server responsive during a spike.
- Each middleware counts on its own: with `app.middleware(...)`, the limit covers the whole app; with `.with(...)`, that route only.

This limits requests in progress, wherever they come from. `app.max_connections` (see [Server configuration](server.md#timeouts-and-connections)) limits open connections instead, idle ones included.

### `rate_limit`

Limits each client to `max` requests per `period`, like `express-rate-limit`:

```rust
use std::time::Duration;

app.middleware(middleware::rate_limit(100, Duration::from_secs(60))); // 100 requests per minute
app.post("/login", login.with(middleware::rate_limit(5, Duration::from_secs(60)))); // slows down password guessing
```

- A client can send a burst of `max` requests, then one every `period / max` (the GCRA algorithm): with `rate_limit(60, Duration::from_secs(60))`, 60 at once, then one per second.
- Beyond that, the answer is `429 Too Many Requests` with `Retry-After` (in seconds). Every response that goes through the middleware carries `X-RateLimit-Limit` and `X-RateLimit-Remaining`.
- By default, a client is an IP address, `req.ip()`. Behind a reverse proxy, set [`app.trust_proxy`](requests.md#behind-a-proxy-trust_proxy), or every visitor shares the proxy's counter. Requests without any address (on a Unix socket without `trust_proxy`) share one counter too, rather than escape the limit.
- `.key(...)` identifies clients differently, by API key or user id for instance. When it returns `None`, the request isn't limited:

```rust
let mut api = Router::new();
api.middleware(
    middleware::rate_limit(10, Duration::from_secs(1))
        .key(|req: &Request| req.header("x-api-key").map(str::to_owned)),
);
```

The counters live in the memory of the process and are cleaned up automatically. With several instances of the application (several servers or containers), each one counts on its own.

### `etag`

Adds an `ETag` to the `200` responses of `GET` and `HEAD` requests whose body is in memory (text, JSON, HTML…), and answers `304 Not Modified`, without a body, when the browser already has that version (`If-None-Match`). It is Express's default `etag` setting, here opt-in:

```rust
app.middleware(middleware::etag());
```

The handler still runs: the bandwidth is saved, not the work. Streamed bodies are left alone, and files served with `res::file` or `ServeDir` already have their own `ETag`.

### `cache`

Keeps whole responses in memory for a time to live, like `apicache` or Laravel's `responsecache`: the first `GET` of a URL runs the handler, the next ones, until the entry expires, are answered from memory without running it.

```rust
use std::time::Duration;

let cache = middleware::cache(Duration::from_secs(60)).max_entries(1_000);
app.get("/posts", list_posts.with(cache.clone()));
app.post("/posts", move |req: Request| {
    let cache = cache.clone();
    async move {
        let post = create_post(req).await?;
        cache.invalidate("/posts"); // the list changed
        Ok::<_, Error>((201, Json(post)))
    }
});
```

- Cached responses carry an `ETag` (and get `304` when the browser has the same version), `Age`, `X-Cache: HIT` or `MISS`, and `Cache-Control: public, max-age=…` unless the handler set one (`.cache_control(false)` to tell browsers nothing).
- Only `200` responses with a body in memory are kept, at most 1 MiB by default (`.max_body(bytes)`), and never those that set a cookie or say `Cache-Control: no-store`, `private` or `no-cache`. Requests with an `Authorization` header are not cached.
- The key is the path and the query string. `.key(|req| …)` adds what the response depends on (the language, the user), or returns `None` not to cache a request.
- Clones share their entries: `cache.invalidate(key)`, `cache.invalidate_prefix("/posts")` and `cache.clear()` remove them. Beyond `.max_entries(n)` (10,000 by default), the expired entries go first, then the oldest.

The entries live in the memory of the process: with several instances of the application, each one has its own cache.

### `compress`

Compresses responses with Brotli, or with gzip when the browser does not accept Brotli, like Express's `compression()`. It needs the `compress` Cargo feature:

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["compress"] }
```

```rust
app.middleware(middleware::compress());
```

Only bodies worth it are compressed: HTML, CSS, JavaScript, JSON, XML, SVG, WebAssembly and the other text-like types, of 1 KiB or more, with a known size (not streamed, so [Server-Sent Events](responses.md#server-sent-events) pass through too) and not encoded already. Images, videos and archives pass through. The response gets `Content-Encoding` and `Vary: Accept-Encoding`, and the compression runs on a blocking thread, so the workers keep answering other requests. Add it before the other middlewares, so that every response goes through it.

### `decompress`

The other way round: some clients (mobile apps, other services, log shippers) compress the bodies they send. `decompress()` decodes them before your handlers read them with `req.json()`, `req.text()` and the others. It comes with the same `compress` feature:

```rust
app.middleware(middleware::decompress());
```

- Bodies sent with `Content-Encoding: gzip` (or `x-gzip`), `br` or `deflate` are decompressed, then `Content-Encoding` is removed: the handler sees the original body. Requests without `Content-Encoding` pass through untouched.
- Another encoding gets a `415 Unsupported Media Type`, and corrupted data a `400`.
- The compressed body and the decompressed one are both limited by the [body limit](requests.md#body-size-limit-and-413) (`413` beyond), so a small "zip bomb" can't fill the memory.
- The body is read in full, then decompressed on a blocking thread, so the workers keep answering other requests.

## Writing a reusable middleware

The simplest form is an `async fn`, like `auth` above. To make a configurable middleware, write a function that returns `impl Middleware`. The `Middleware` trait isn't in the prelude: import it from `vitesse`.

```rust
use vitesse::Middleware;

fn api_key(expected: &'static str) -> impl Middleware {
    move |req: Request, next: Next| async move {
        if req.header("x-api-key") != Some(expected) {
            return Error::unauthorized("invalid API key").into_response();
        }
        next.run(req).await
    }
}

app.middleware(api_key("my-secret-key"));
```

If the configuration isn't `Copy` (a `String`, a `Vec`…), clone it into each request's future: `move |req: Request, next: Next| { let value = value.clone(); async move { /* ... */ } }`.

For more control, implement the trait on a struct. The middleware lives as long as the application (`&'static self`), so its future can borrow its fields without `Arc` or cloning:

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use vitesse::{BoxFuture, Middleware};

struct RequestCounter {
    total: AtomicU64,
}

impl Middleware for RequestCounter {
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response> {
        Box::pin(async move {
            let n = self.total.fetch_add(1, Ordering::Relaxed) + 1;
            next.run(req).await.header("x-request-number", n)
        })
    }
}

app.middleware(RequestCounter { total: AtomicU64::new(0) });
```

`next.run(req)` already returns a `BoxFuture<Response>`: when you don't need to touch the response, return it directly, without `Box::pin`.

> [!NOTE]
> A panic in a middleware written as a closure or an `async fn` becomes a `500` that goes back through the outer middleware and `app.on_error`, just like a panic in a handler. A hand-written `impl Middleware` doesn't get this treatment: its panics are caught further up the chain (see [Panics](errors.md#panics)).

## Tower layers

axum, hyper and tonic share a middleware ecosystem: [tower](https://docs.rs/tower) and [tower-http](https://docs.rs/tower-http). With the `tower` feature, `middleware::layer(...)` runs any tower `Layer` as a Vitesse middleware:

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tower"] }
tower-http = { version = "0.6", features = ["cors", "set-header", "trace"] }
```

```rust
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use vitesse::http::{HeaderName, HeaderValue};
use vitesse::prelude::*;

let mut app = App::new();
app.middleware(middleware::layer(TraceLayer::new_for_http()));
app.middleware(middleware::layer(SetResponseHeaderLayer::overriding(
    HeaderName::from_static("x-powered-by"),
    HeaderValue::from_static("Vitesse"),
)));

let mut api = Router::new();
api.middleware(middleware::layer(CorsLayer::permissive()));
api.get("/users", |_| async { Json(vec!["ada", "grace"]) });
app.mount("/api", api);
```

A layer goes wherever a middleware goes: `app.middleware(...)`, `router.middleware(...)` and `handler.with(...)`, in the same [order](#execution-order) as the others. `TraceLayer` reports through the [tracing](https://docs.rs/tracing) crate: its lines only appear once a subscriber is installed, `tracing_subscriber::fmt::init()` for instance.

Any layer whose service accepts an `http::Request<vitesse::Body>` and returns an `http::Response` works: most of tower-http (`CorsLayer`, `SetResponseHeaderLayer`, `ValidateRequestHeaderLayer`, `RequestBodyLimitLayer`, `TraceLayer`…), tower's own layers (`ConcurrencyLimitLayer`, `MapRequestLayer`…) and crates such as tower-governor.

How the two worlds meet:

- **The request.** The layer sees an `http::Request` built from the Vitesse request. Whatever it changes (method, URI, headers, extensions, body) is carried over to the request that the rest of the chain receives, routing included if the layer is global.
- **The response.** It comes back as a Vitesse `Response`, which the middlewares around the layer can still change.
- **Errors.** An error returned by the tower service (tower's `Timeout` reports one, for example) becomes a `500`, logged without being shown to the client. Errors produced by Vitesse (a `404`, a handler's `Err`) cross the layer and still reach [`app.on_error`](errors.md).
- **One service for all requests.** The service is built once, at startup, then cloned for each request, as in axum. State that its clones share is shared by all requests: a `ConcurrencyLimitLayer` really limits the whole route.

> [!TIP]
> Each layer converts the request to `http::Request` and back, which costs a little. For what Vitesse already ships (CORS, timeouts, compression, limits, authentication), the built-in middlewares are the faster choice; layers are there for everything else.

The other way round, `app.into_service()` turns a whole Vitesse application into a tower `Service`: serve it with hyper, nest it in an axum router (`Router::nest_service`), or test it with `tower::ServiceExt` (see [Testing as a tower service](testing.md#testing-as-a-tower-service)).
