# Middleware

A middleware is a function that runs around your handlers: it receives the request and `next`, the rest of the chain, and can act before the handler, after it, or answer on its own. This page shows how to write them, in which order they run, and documents the middleware that ships with Vitesse.

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
- `req.headers_mut()` gives access to the headers, and `req.set_body(...)` replaces the body.
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

## Built-in middleware

The `vitesse::middleware` module (available as `middleware::` with the prelude) provides the classics:

| Middleware | Express equivalent | Role |
|---|---|---|
| `middleware::logger()` | `morgan('dev')` | logs each request |
| `middleware::cors()` | `cors()` | CORS headers and preflight requests |
| `middleware::helmet()` | `helmet()` | security headers |
| `middleware::timeout(duration)` | `connect-timeout` | `503` when a request takes too long |
| `middleware::serve_static(dir)` | `express.static(dir)` | static files, see [Static files](static-files.md) |

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
