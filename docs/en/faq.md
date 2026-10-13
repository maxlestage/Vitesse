# FAQ & limitations

Answers to the questions people ask most often about Vitesse, followed by an honest list of what it does not do (yet). If your question is not here, open an issue on GitHub.

## General

### Is Vitesse ready for production?

Vitesse is young (version 0.1), but its engine was built for production from the start: the test suite covers the HTTP engine in detail (pipelining, `chunked` bodies, `Expect: 100-continue`, size limits, graceful shutdown) and runs on Linux, macOS and Windows for every change. Panics become `500` responses instead of crashing the server, malformed requests are rejected, and `SIGTERM` lets in-flight requests finish.

Until version 1.0, the API may still change between minor versions. Depend on `vitesse = { git = "https://github.com/maxlestage/Vitesse" }` (Cargo then only installs compatible `0.1.x` updates), read the changelog before upgrading, serve HTTPS ([directly](https.md) or through a reverse proxy, see [Production](production.md)), and load-test with your own workload.

### Is the project really 100% Rust?

Yes: the framework, the examples, the benchmark runner (`bench/runner`) and the website (served by Vitesse itself, with pages and WebAssembly islands written with [Active](https://active-c1883a966650.herokuapp.com/)) are all Rust, and the repository contains no JavaScript or TypeScript. Two caveats, to be precise:

- browsers cannot start WebAssembly without a few lines of JavaScript, and they only run service workers (what makes the website work offline) as scripts: the website build (wasm-bindgen) generates a small loader file, and the website's server generates its service worker; neither is written by hand nor stored in the repository;
- the documentation shows Express (JavaScript) snippets, but only as before/after comparisons for people coming from Express.

The benchmark also measures servers written in other languages, since comparing them is the whole point: the Drogon server is in C++ (`bench/drogon`), and wrk, which generates the load, runs scenario scripts written in Lua (`bench/lua`).

### Why not actix-web or axum?

Both are excellent and mature frameworks, and Vitesse now covers most of what they offer: HTTPS and HTTP/2 (plus HTTP/3), WebSocket (over HTTP/2 too), Server-Sent Events, route constraints and named routes with URL generation, multipart uploads, per-route body limits, signed cookies and sessions, HTML templates, authentication, rate and concurrency limits, request ids, compression, trusted proxies, configurable timeouts, Unix sockets, and the [tower](https://crates.io/crates/tower) middleware ecosystem through the `tower` feature. [Coming from axum or actix-web](from-axum-actix.md) gives the equivalents, feature by feature.

Vitesse is a good fit if:

- you come from Express and want the same mental model: one `Request`, middleware with `next`, routers, and handlers that simply return their response;
- you want top performance: in the [benchmark](performance.md), Vitesse handles more requests per second than both, with less CPU per request.

What remains different:

- **no extractors, by design**: a handler asks its `Request` for what it needs (`req.param_as`, `req.json`…) instead of declaring typed arguments;
- **no stable 1.x API yet**: Vitesse is at version 0.1 and may still change before 1.0, while axum and actix-web have years of production use behind them;
- **a smaller ecosystem**: fewer crates, examples and answers online are written for Vitesse, although tower layers and services bridge much of the gap.

### How is it different from Express?

The API is deliberately close, but handlers return their response instead of mutating `res`, data is typed, errors go up with `?`, and the program is compiled, which makes it about 50 times faster in the benchmark. The [Coming from Express](from-express.md) guide covers everything.

### Which async runtime does it use?

[tokio](https://tokio.rs), and only tokio. `app.run` creates its own runtime, while `app.listen` and `app.bind` run inside yours. Any crate built on tokio works in your handlers: database drivers, HTTP clients, Redis… Vitesse re-exports the crate as `vitesse::tokio` (with the features it uses itself); add tokio to your `Cargo.toml` to use `#[tokio::main]` or `#[tokio::test]`. Other runtimes (async-std, smol) are not supported.

### Which Rust version do I need?

Rust 1.85 or later (the 2024 edition), as declared by `rust-version` in Vitesse's `Cargo.toml`. Update with `rustup update`.

### Does it work on Windows and macOS?

Yes: the test suite runs on Linux, macOS and Windows. The thread-per-core mode (one event loop and one `SO_REUSEPORT` socket per core) is a Linux-only optimisation; on other systems, `app.run` uses tokio's multi-threaded runtime, which is a bit slower but offers exactly the same API. `Ctrl+C` stops the server cleanly everywhere, `SIGTERM` on Unix. Unix domain sockets (`app.run_unix`) are not available on Windows. The benchmark runner (`bench/runner`) only runs on Linux.

## Features

### Does Vitesse support HTTPS and HTTP/2?

Yes, with two opt-in Cargo features. `tls` serves HTTPS (TLS 1.2 and 1.3, with rustls: no OpenSSL to install): `app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?)`. `http2` adds HTTP/2, which browsers negotiate over TLS, and which clients that start in HTTP/2 (h2c) also get without TLS. The same routes and middleware answer both protocols. See [HTTPS and HTTP/2](https.md).

A reverse proxy (Nginx, Caddy) in front is still a good option, for automatic certificates or several apps on one machine, and platforms such as Heroku handle HTTPS for you: see [Production](production.md).

### Does Vitesse support WebSocket and HTTP/3?

Yes, both:

- **WebSocket** is built in (the `ws` feature, enabled by default): `app.ws("/chat", |req, socket| async move { … })`, in the style of `express-ws`. See [WebSocket](websocket.md).
- **HTTP/3** over QUIC is available with the opt-in `http3` feature: `app.http3(Http3::from_pem_files(...)?)` serves the same application over UDP, next to HTTP/1.1. See [HTTP/3 and QUIC](http3.md).

WebSocket works over HTTP/1.1 (`ws://`, or `wss://` with the `tls` feature) and, with the `http2` feature, [over HTTP/2](websocket.md#websocket-over-http2), which browsers use on their own on an HTTPS site. It doesn't work over HTTP/3: browsers then open their WebSockets over HTTP/2 or HTTP/1.1. For one-way server-to-client push, Server-Sent Events are often simpler: see the next question.

### Does Vitesse support Server-Sent Events?

Yes, with `vitesse::sse` (always available, and in the prelude as `sse`): return an `Sse` response, the browser reads it with `EventSource`. The simplest way is a channel, fed by a task:

```rust
use std::time::Duration;

use vitesse::prelude::*;
use vitesse::sse::Event;

let mut app = App::new();
app.get("/clock", |_| async {
    let (tx, stream) = sse::channel(16);
    tokio::spawn(async move {
        for i in 0.. {
            // `send` fails once the browser has gone away.
            if tx.send(Event::new().event("tick").data(i.to_string())).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    stream
});
```

Vitesse sends a keep-alive comment every 15 seconds, so proxies don't close a quiet connection. It works over HTTP/1.1, HTTP/2 and HTTP/3. See [Server-Sent Events](responses.md#server-sent-events).

### Compression?

Yes, with the `compress` feature. `middleware::compress()` compresses responses in Brotli, or gzip for browsers that don't accept Brotli, when it is worth it (text-like bodies of 1 KiB or more). `middleware::decompress()` does the reverse for request bodies sent with `Content-Encoding: gzip`, `br` or `deflate`, within the body limit (which also stops "zip bombs"):

```rust
app.middleware(middleware::compress());
app.middleware(middleware::decompress());
```

Behind a reverse proxy, you can also let the proxy compress responses instead. See [Middleware](middleware.md#compress).

### Template engines?

Yes, with the `templates` feature, which brings in [minijinja](https://crates.io/crates/minijinja) (the Jinja2 syntax: `{{ name }}`, `{% for %}`, `{% extends %}`…). It plays the role of Express's `app.set('views', …)` and `res.render(…)`:

```rust
app.views("templates"); // templates/users.html, templates/layout.html…
app.get("/users", |req: Request| async move {
    req.render("users.html", json!({ "users": ["Ada", "Grace"] }))
});
```

Values are HTML-escaped in `.html` templates, and a missing template or a rendering error becomes a `500` whose cause is logged. See [HTML templates](templates.md).

Other template crates ([askama](https://crates.io/crates/askama), [tera](https://crates.io/crates/tera)…) work too: render with them, and return the result with `Html(rendered)`.

### How do I use a database?

With any async driver (for example [sqlx](https://crates.io/crates/sqlx)). Create the connection pool once at startup, register it with `app.state(pool)` and read it in handlers with `req.state::<Pool>()`. Creating a pool is usually async: start the server with `#[tokio::main]` and `app.listen` (which shuts down gracefully, just like `app.run`), as shown in [Server configuration](server.md).

### File uploads (multipart)?

Yes, with the `multipart` feature. `req.multipart()?` reads a `multipart/form-data` body field by field, as it arrives, and `field.save(path)` streams a file to disk without holding it in memory. Raise the body limit on the upload route only, with `middleware::body_limit`:

```rust
async fn upload(req: Request) -> vitesse::Result<String> {
    let mut form = req.multipart()?; // 415 if the body isn't multipart
    let (mut files, mut bytes) = (0, 0);
    while let Some(field) = form.next_field().await? {
        if field.file_name().is_some() {
            files += 1;
            // Never use the client's file name as a path as is.
            bytes += field.save(format!("uploads/{files}.bin")).await?;
        }
    }
    Ok(format!("{files} files, {bytes} bytes"))
}

app.post("/upload", upload.with(middleware::body_limit(100 * 1024 * 1024)));
```

See [File uploads](requests.md#file-uploads-multipart).

### Sessions and authentication?

For authentication, two middlewares are built in: `middleware::basic_auth(("admin", "secret"))` (the browser asks for a user name and a password) and `middleware::bearer_auth(token)` (`Authorization: Bearer …`, for APIs). Both take a closure too, to check the credentials against your own users, and compare them in constant time. They send credentials with every request: only use them over HTTPS. See [`basic_auth` and `bearer_auth`](middleware.md#basic_auth-and-bearer_auth).

Sessions and signed cookies are built in, with the `session` feature, like Express's `express-session` and `cookie-parser`:

```rust
use vitesse::session::MemoryStore;

app.cookie_secret(std::env::var("COOKIE_SECRET").expect("COOKIE_SECRET is not set"));
app.middleware(middleware::session(MemoryStore::new()));
app.get("/visits", |req: Request| async move {
    let session = req.session();
    let visits = session.get::<u32>("visits").unwrap_or(0) + 1;
    session.insert("visits", visits)?;
    Ok::<_, Error>(format!("visit #{visits}"))
});
```

The browser only gets a random id, in an `HttpOnly` cookie (signed, here, by the app's secret); the data stays on the server. `MemoryStore` forgets everything on restart and isn't shared between instances: in production, plug in a store of your own (Redis, PostgreSQL…) by implementing the `SessionStore` trait. Without sessions, `req.sign_cookie(...)` and `req.signed_cookie(name)` give cookies the client can read but not change. See [Signed cookies and sessions](sessions.md).

[tower-sessions](https://crates.io/crates/tower-sessions), the usual choice with axum, works too, through the `tower` feature and its ready-made stores: see [Sessions with tower-sessions](from-axum-actix.md#sessions-with-tower-sessions).

### Why does CORS refuse my cookies?

`middleware::cors()` allows every origin with `Access-Control-Allow-Origin: *`. For a request with credentials (cookies, `Authorization`), browsers reject a response that says `*`, and that is still what you get with `allow_credentials(true)` if you list no origin. This is deliberate, as in Express: echoing back any origin would let any website read the responses of a logged-in user. List your trusted origins explicitly:

```rust
app.middleware(
    middleware::cors()
        .allow_origin("https://app.example.com")
        .allow_credentials(true),
);
```

### How do I log requests and errors?

`middleware::logger()` prints one line per request, like `morgan('dev')`: `GET /users/42 200 0.084 ms`. The cause of server errors (a `5xx` error with a source, for example a database error converted with `?`) is printed on standard error, prefixed with `[vitesse]`.

For structured logs, write your own middleware with the logging crate of your choice. Two helpers make the logs easier to use: `middleware::request_id()` gives each request an id (`req.get::<middleware::RequestId>()`), sent back in the `X-Request-Id` response header, and `req.route()` gives the matched route pattern (`/users/:id`), to group the logs by route rather than by path. With the `tower` feature, tower-http's `TraceLayer` works too, through `middleware::layer`. See [Logging](production.md#logging) for a complete example.

### Why does `req.ip()` return the proxy's address?

Because the proxy is the one connected to Vitesse. The client's address is in the `X-Forwarded-For` header, which a client could also send: tell Vitesse which proxies to trust, like Express's `trust proxy` setting, and `req.ip()` reads it for you:

```rust
app.trust_proxy("loopback"); // a proxy on the same machine; or "10.0.0.0/8", a number of hops, true
```

`req.protocol()` and `req.hostname()` then follow `X-Forwarded-Proto` and `X-Forwarded-Host` too. See [Getting the client's IP address](production.md#getting-the-clients-ip-address).

### Can I use tower or tower-http middleware?

Yes, with the `tower` feature. `middleware::layer(...)` runs any tower `Layer` as a Vitesse middleware, globally, on a router or on a single route: tower-http's `CorsLayer`, `TraceLayer` or `SetResponseHeaderLayer`, tower's `ConcurrencyLimitLayer`, tower-governor, tower-sessions…

```rust
use tower_http::cors::CorsLayer;

app.middleware(middleware::layer(CorsLayer::permissive()));
```

The other way round, `app.into_service()` turns your application into a tower `Service`: serve it with hyper, nest it in an axum router (`Router::nest_service`), or test it with `tower::ServiceExt::oneshot`. See [Tower layers](middleware.md#tower-layers).

### Can routes have regular expressions or optional parameters?

Yes, with Express 4's syntax: `r"/users/:id(\d+)"` only matches digits, `/archive/:year?/:month?` also matches `/archive`, and `/flights/:from-:to` takes two parameters from one segment. A route with a constraint wins over a plain parameter at the same position, so `/users/:id(\d+)` and `/users/:name` can coexist. `app.case_sensitive(false)` makes routes ignore case, as Express does by default. See [Path parameters](routing.md#path-parameters).

### Can I generate URLs from route names?

Yes, like actix-web's `url_for`: name the route with `.name(...)`, then build its path with `req.path_for` or its absolute URL with `req.url_for`. Values are percent-encoded, and constraints are checked:

```rust
app.get("/users/:id", show_user).name("user");
app.post("/users", |req: Request| async move {
    let link = req.path_for("user", &[&42])?; // "/users/42"
    Ok::<_, Error>(Redirect::see_other(link))
});
```

See [Named routes and URL generation](routing.md#named-routes-and-url-generation).

### Can I add routes while the server is running?

No. The application is frozen when the server starts (this is part of what makes it fast: no locks, no reference counting). Decide your routes at startup, and use [state](state.md) for data that changes.

## Deployment and community

### How do I deploy a Vitesse app?

Build a release binary (`cargo build --release`), listen on the port given by the `PORT` environment variable, and run it. Step-by-step guides: [Heroku from your phone](heroku-mobile.md), [Docker](docker.md), and [Production](production.md) for reverse proxies and HTTPS.

### How do I report a bug or contribute?

Open an issue on GitHub, with your Vitesse version, your system and, ideally, a minimal example that reproduces the problem. Pull requests are welcome too; for a large change, open an issue first to discuss it. Before submitting, run the same checks as the CI:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The documentation lives in `docs/` in English, French and Spanish: when you change a page, update the other languages too, or mention it in your pull request.

### What is the license?

Vitesse is proprietary software: © Maxime Nathan Lestage, all rights reserved. Its source is published on crates.io and GitHub so that you can read, audit and build it, and you may use it freely as a dependency of your own programs, commercial ones included. Modifying it or redistributing its source needs the author's written permission. The full terms are in the repository's [LICENSE](https://vitesse-f16135654c3d.herokuapp.com/assets/LICENSE.txt).

## Current limitations

Not included, each for a reason:

- **Extractors**, by design: a handler takes one `Request` and asks it for what it needs;
- **WebSocket over HTTP/3** (RFC 9220): the h3 crate doesn't implement it, and browsers don't rely on it. They open their WebSockets over HTTP/2 or HTTP/1.1, which [Vitesse serves](websocket.md#websocket-over-http2);
- **HTTP/2 server push**: Chrome removed it in 2022 and Firefox in 2024, and axum and actix-web don't offer it either;
- the **`Upgrade: h2c`** mechanism: RFC 9113 removed it from HTTP/2. HTTP/2 works over TLS, and in clear text with prior knowledge (see [HTTPS and HTTP/2](https.md#http2-in-clear-text-h2c));
- **0-RTT (early data) over HTTP/3**: a request sent that early can be replayed by an attacker, so Vitesse waits for the end of the handshake (see [HTTP/3](http3.md#what-changes-and-what-isnt-supported-yet));
- **Runtimes other than tokio** (async-std, smol);
- **A stable 1.x API**: Vitesse is at version 0.1, and its API may still change before 1.0.

A few other things to know:

- the engine's size limits are fixed (60 KiB for the request head, 64 headers), but its delays are settings: `keep_alive` (60 s), `header_timeout` (30 s) and `shutdown_timeout` (10 s), as well as `max_connections` (see [Server configuration](server.md#timeouts-and-connections));
- `body_limit` applies to the whole application, and `middleware::body_limit` changes it per route or per router, over every protocol;
- routes are case-sensitive by default (`/Users` ≠ `/users`): call `app.case_sensitive(false)` to ignore case, as Express does;
- routes cannot be added once the server has started;
- templates loaded with `app.views(...)` are read once, then kept in memory: restart the server to see a change;
- sessions in a `MemoryStore` live in the memory of each process: they are lost on restart and not shared between instances;
- `X-Forwarded-For`, `X-Forwarded-Proto` and `X-Forwarded-Host` are only read once you declare your proxies with `app.trust_proxy(...)`;
- `middleware::rate_limit` counts in the memory of each process: with several instances, each one counts on its own.
