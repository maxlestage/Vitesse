# FAQ & limitations

Answers to the questions people ask most often about Vitesse, followed by an honest list of what it does not do (yet). If your question is not here, open an issue on [GitHub](https://github.com/maxlestage/Vitesse/issues).

## General

### Is Vitesse ready for production?

Vitesse is young (version 0.1), but its engine was built for production from the start: the test suite covers the HTTP engine in detail (pipelining, `chunked` bodies, `Expect: 100-continue`, size limits, graceful shutdown) and runs on Linux, macOS and Windows for every change. Panics become `500` responses instead of crashing the server, malformed requests are rejected, and `SIGTERM` lets in-flight requests finish.

Until version 1.0, the API may still change between minor versions. Depend on `vitesse = "0.1"` (Cargo then only installs compatible `0.1.x` updates), read the [changelog](https://github.com/maxlestage/Vitesse/blob/master/CHANGELOG.md) before upgrading, put a reverse proxy in front for HTTPS (see [Production](production.md)), and load-test with your own workload.

### Is the project really 100% Rust?

Yes: the framework, the examples, the benchmark runner (`bench/runner`) and the website (written with [Yew](https://yew.rs) and compiled to WebAssembly) are all Rust, and the repository contains no JavaScript or TypeScript. Two caveats, to be precise:

- browsers cannot start WebAssembly without a few lines of JavaScript, so the website build (Trunk and wasm-bindgen) generates a small loader file automatically; it is neither written by hand nor stored in the repository;
- the documentation shows Express (JavaScript) snippets, but only as before/after comparisons for people coming from Express.

The benchmark also measures servers written in other languages, since comparing them is the whole point: the Drogon server is in C++ (`bench/drogon`), and wrk, which generates the load, runs scenario scripts written in Lua (`bench/lua`).

### Why not actix-web or axum?

Both are excellent and mature frameworks. Vitesse is a good fit if:

- you come from Express and want the same mental model: one `Request`, middleware with `next`, routers, and handlers that simply return their response, with no extractors or service layers to learn;
- you want top performance: in the [benchmark](performance.md), Vitesse handles more requests per second than both, with less CPU per request.

Prefer actix-web or axum if you need what Vitesse does not do (yet): HTTP/2, TLS or WebSocket built in, the [tower](https://github.com/tower-rs/tower) middleware ecosystem, or a stable 1.x API.

### How is it different from Express?

The API is deliberately close, but handlers return their response instead of mutating `res`, data is typed, errors go up with `?`, and the program is compiled, which makes it about 50 times faster in the benchmark. The [Coming from Express](from-express.md) guide covers everything.

### Which async runtime does it use?

[tokio](https://tokio.rs), and only tokio. `app.run` creates its own runtime, while `app.listen` and `app.bind` run inside yours. Any crate built on tokio works in your handlers: database drivers, HTTP clients, Redis… Vitesse re-exports the crate as `vitesse::tokio` (with the features it uses itself); add tokio to your `Cargo.toml` to use `#[tokio::main]` or `#[tokio::test]`. Other runtimes (async-std, smol) are not supported.

### Which Rust version do I need?

Rust 1.85 or later (the 2024 edition), as declared by `rust-version` in Vitesse's `Cargo.toml`. Update with `rustup update`.

### Does it work on Windows and macOS?

Yes: the test suite runs on Linux, macOS and Windows. The thread-per-core mode (one event loop and one `SO_REUSEPORT` socket per core) is a Linux-only optimisation; on other systems, `app.run` uses tokio's multi-threaded runtime, which is a bit slower but offers exactly the same API. `Ctrl+C` stops the server cleanly everywhere, `SIGTERM` on Unix. The benchmark runner (`bench/runner`) only runs on Linux.

## Features

### Does Vitesse support HTTPS and HTTP/2?

Not directly: Vitesse speaks HTTP/1.1 over plain TCP. Put it behind a reverse proxy (Nginx, Caddy, or the load balancer of your platform) that handles TLS and HTTP/2 and forwards requests in HTTP/1.1, as is common with Express. Platforms such as Heroku already do this for you. See [Production](production.md).

### WebSocket?

Not supported. For server-to-client push, a streamed response often does the job: `Body::from_stream` sends each chunk as soon as it is produced, which is all you need for [Server-Sent Events](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events) with a `text/event-stream` content type (see [Responses](responses.md)). For real bidirectional WebSocket, run a dedicated service next to Vitesse.

### Compression?

Not built in. Let the reverse proxy compress responses (gzip, brotli), which is usually the most efficient option anyway.

### Template engines?

Not built in, just like Express without a view engine. Use any template crate ([askama](https://crates.io/crates/askama), [minijinja](https://crates.io/crates/minijinja), [tera](https://crates.io/crates/tera)…) and return the result with `Html(rendered)`. Load or compile your templates once at startup and share them through the [state](state.md).

### How do I use a database?

With any async driver (for example [sqlx](https://crates.io/crates/sqlx)). Create the connection pool once at startup, register it with `app.state(pool)` and read it in handlers with `req.state::<Pool>()`. Creating a pool is usually async: start the server with `#[tokio::main]` and `app.listen` (which shuts down gracefully, just like `app.run`), as shown in [Server configuration](server.md).

### File uploads (multipart)?

There is no built-in `multipart/form-data` parser. Read the raw body with `req.bytes()` (within the `body_limit`, to be raised for large files) or as a stream with `req.take_body()`, and parse it with a dedicated crate such as [multer](https://crates.io/crates/multer).

### Sessions and authentication?

There are no built-in sessions, but all the building blocks are there: read cookies with `req.cookie(name)`, set them with `Cookie` (`http_only`, `secure`, `same_site`…), check a token in a middleware and attach the user to the request with `req.set(user)`. See [Middleware](middleware.md).

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

`middleware::logger()` prints one line per request, like `morgan('dev')`: `GET /users/42 200 0.084 ms`. The cause of server errors (a `5xx` error with a source, for example a database error converted with `?`) is printed on standard error, prefixed with `[vitesse]`. For structured logs, write your own middleware with the logging crate of your choice.

### Why does `req.ip()` return the proxy's address?

Because the proxy is the one connected to Vitesse. The client's address is in the `X-Forwarded-For` header (`req.header("x-forwarded-for")`), which you should only trust when it is set by your own proxy. See [Production](production.md).

### Can I add routes while the server is running?

No. The application is frozen when the server starts (this is part of what makes it fast: no locks, no reference counting). Decide your routes at startup, and use [state](state.md) for data that changes.

## Deployment and community

### How do I deploy a Vitesse app?

Build a release binary (`cargo build --release`), listen on the port given by the `PORT` environment variable, and run it. Step-by-step guides: [Heroku from your phone](heroku-mobile.md), [Docker](docker.md), and [Production](production.md) for reverse proxies and HTTPS.

### How do I report a bug or contribute?

Open an issue on [GitHub](https://github.com/maxlestage/Vitesse/issues), with your Vitesse version, your system and, ideally, a minimal example that reproduces the problem. Pull requests are welcome too; for a large change, open an issue first to discuss it. Before submitting, run the same checks as the CI:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The documentation lives in [`docs/`](https://github.com/maxlestage/Vitesse/tree/master/docs) in English, French and Spanish: when you change a page, update the other languages too, or mention it in your pull request.

### What is the license?

Vitesse is dual-licensed under [MIT](https://github.com/maxlestage/Vitesse/blob/master/LICENSE-MIT) or [Apache 2.0](https://github.com/maxlestage/Vitesse/blob/master/LICENSE-APACHE), at your option (`MIT OR Apache-2.0`), like most of the Rust ecosystem.

## Current limitations

Like Express, Vitesse deliberately does little. Not included (yet):

- **HTTP/2 and TLS**: put Vitesse behind a reverse proxy such as Nginx or Caddy (see [Production](production.md));
- **WebSocket**;
- **Compression**: delegate it to the reverse proxy;
- **Template engines**: use a template crate and return `Html(...)`;
- **Partial parameters within a segment** (`/flights/:from-:to`), as well as optional parameters (`/:id?`) and regular expressions in routes;
- **Multipart forms**: use a dedicated crate on the raw body.

A few other things to know:

- the engine's limits are fixed: 60 KiB for the request head, 64 headers, about 60 s before closing an idle connection, 10 s of grace period at shutdown (see [Server configuration](server.md));
- `body_limit` applies to the whole application, not per route;
- routes are case-sensitive (`/Users` ≠ `/users`);
- routes cannot be added once the server has started;
- `X-Forwarded-For` is not interpreted automatically by `req.ip()`;
- tokio is the only supported runtime.
