# Introduction

Vitesse is a minimalist web framework for Rust that borrows the API of Express.js. You write `app.get("/users/:id", ...)`, read `req.param("id")`, answer with `res::status(201).json(...)` and chain middleware with `next`. The difference is that it all runs as native code, with the performance that comes with it.

## What is Vitesse?

Vitesse (French for "speed") is a library for building HTTP servers, websites and JSON APIs. It has its own HTTP/1.1 engine on top of [tokio](https://tokio.rs), a tree router that only runs a regular expression when a route asks for one, a middleware chain and a set of ready-made middlewares, plus WebSocket and, as options, HTTPS, HTTP/2, HTTP/3 over QUIC, sessions and HTML templates.

Here is a complete server:

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    app.run(3000)
}
```

If you have written Express code before, you can probably already read it:

```js
const express = require('express');
const app = express();

app.get('/', (req, res) => res.send('Hello World!'));

app.listen(3000);
```

## Who is it for?

- **Express and Node.js developers** who want Rust's speed, low memory use and reliability without learning a whole new way of thinking about web servers.
- **Rust developers** who want a small, explicit framework. There are no extractors and no procedural macros: a handler takes a `Request` and returns a response.
- **Anyone paying for servers**: less CPU per request means fewer machines, or smaller ones.

You don't need to be a Rust expert. Most handlers are a few lines long, and this documentation explains the Rust-specific parts as they come up.

## Philosophy

- **A small core, like Express.** Vitesse covers routing, middleware, request and response helpers, static files, WebSocket and a test client. Sessions and templates are opt-in features, and you pick your own crates for databases and the rest.
- **A familiar API.** Most Express concepts map directly onto Vitesse. See [Coming from Express](from-express.md) for the full mapping.
- **No magic.** A handler asks the request for what it needs (`req.param("id")`, `req.json().await`). Errors are ordinary values, and `?` turns them into HTTP responses.
- **Fast by default.** The numbers below need no tuning. `app.run(3000)` already spreads the work over every CPU core.
- **Robust by default.** A panicking handler becomes a `500`, an oversized body a `413`, and a malformed request is rejected. `Ctrl+C` and `SIGTERM` trigger a graceful shutdown that lets in-flight requests finish.

## Key features

- **Express-style routing**: `get`, `post`, `put`, `patch`, `delete`, `all`, parameters (`/users/:id`, `/flights/:from-:to`), constraints (`/users/:id(\d+)`), optional parameters (`/archive/:year?`) and wildcards (`/files/*path`). `HEAD`, `OPTIONS` and `405 Method Not Allowed` are handled for you, and named routes build their own URLs (`req.path_for("user", &[&42])`).
- **Typed input**: `req.param_as::<u64>("id")?`, `req.query_as::<T>()?`, `req.json::<T>().await?` and `req.form::<T>().await?` answer `400` or `413` automatically when the input is bad.
- **Flexible responses**: return a `&str`, a `String`, `Json(...)`, `json!({...})`, `Html(...)`, a `(status, body)` tuple, an `Option` or a `Result`. You can also build the response yourself with `res::status(201).header(...).json(...)`.
- **Middleware** at three levels (global, per router, per route) with `next.run(req).await`. Built in: `logger`, `cors`, `helmet`, `timeout`, request ids, `Basic` and `Bearer` authentication, rate and concurrency limits, per-route body limits, and compression in both directions (`compress` feature).
- **Routers** you can mount under a prefix, each with its own middleware and fallback, **shared state** for the whole app and **per-request data** set by middleware.
- **One error type** (`vitesse::Error`) that becomes `{"error": "..."}`. You can change the format of every error response with `app.on_error`.
- **Static files** with MIME types, `ETag`/`Last-Modified`, `Range` requests, and protection against `../` and hidden files.
- **Cookies, redirects, downloads and streaming** bodies in both directions, **content negotiation** (`req.accepts(&["html", "json"])`) and **file uploads** (`multipart` feature) read as a stream.
- **Signed cookies and sessions** (`session` feature): `req.sign_cookie(...)`, and `req.session()` with an in-memory store or your own.
- **HTML templates** (`templates` feature): `app.views("templates")` and `req.render("page.html", json!({...}))`, with minijinja's Jinja2 syntax.
- **Server-Sent Events** with `vitesse::sse`: push live updates to the browser's `EventSource`.
- **WebSocket** with `app.ws("/chat", |req, socket| async move { … })`, in the style of `express-ws`: route parameters, middleware and state work as in any route, over HTTP/1.1 and HTTP/2.
- **HTTPS and HTTP/2**, with the opt-in `tls` and `http2` features: no reverse proxy needed. **HTTP/3 over QUIC** with the `http3` feature: the same app also answers over UDP.
- **Production settings**: keep-alive and timeouts, a connection limit, trusted proxies (`app.trust_proxy`, for the client's real address), Unix sockets.
- **tower interop** (`tower` feature): use tower and tower-http middleware, or serve the app as a tower `Service`.
- **A `TestClient`** that tests your app in memory, without opening a port.
- **A managed runtime**: you don't need `#[tokio::main]`. Vitesse runs one thread per core on Linux and shuts down gracefully.

## How fast is it?

Requests per second, with the server's CPU time per request in brackets (lower is better):

| Scenario | Express 5 | Drogon 1.9 (C++) | axum 0.8 | actix-web 4 | **Vitesse** |
|---|---:|---:|---:|---:|---:|
| `GET /` (text) | 5,921 (180 µs) | 203,244 (9.7 µs) | 168,165 (11.7 µs) | 274,195 (7.1 µs) | **291,415 (6.1 µs)** |
| `GET /json` | 5,765 (184 µs) | 120,489 (16.5 µs) | 165,988 (11.9 µs) | 248,987 (7.9 µs) | **313,438 (5.9 µs)** |
| `GET /json` sent by a browser (12 headers) | 5,627 (188 µs) | 92,041 (21.7 µs) | 131,710 (15.0 µs) | 179,505 (10.9 µs) | **290,478 (6.8 µs)** |
| `GET /users/:id` (parameter + JSON) | 5,627 (187 µs) | 98,486 (20.1 µs) | 151,303 (13.1 µs) | 209,173 (9.4 µs) | **304,323 (6.2 µs)** |
| `POST /echo` (reads and returns JSON) | 4,538 (235 µs) | 69,876 (28.4 µs) | 105,570 (18.8 µs) | 170,129 (11.7 µs) | **253,941 (7.6 µs)** |
| `GET /` pipelined ×16 | 8,352 (128 µs) | 724,059 (2.7 µs) | 213,678 (9.3 µs) | 1,272,510 (1.6 µs) | **2,781,541 (0.64 µs)** |

In short:

- **Against actix-web**, often called the fastest Rust framework: up to **+62%** throughput with a real browser request, +45 to +49% with parameters or a JSON body, **2.2 times more** with pipelining, and 15 to 59% less CPU per request.
- **Against axum**: 1.7 to 2.4 times more requests per second, half the CPU per request, and 13 times more with pipelining.
- **Against Drogon (C++)**: 1.4 to 3.8 times faster.
- **Against Express**: about 50 times faster.

All of these ran on a 4 vCPU VM, with the server pinned to 2 cores, wrk on the other 2, 128 keep-alive connections and 10 s per scenario. The Express server used for these numbers has since been removed from the repository to keep it 100% Rust (it is still in the git history), and the benchmark now compares Drogon, axum, actix-web and Vitesse. The [Performance](performance.md) page covers the methodology, how to reproduce the numbers (with the Rust runner in `bench/runner`) and why Vitesse is fast. The raw results are also in the README.

## Current limitations

A few things are left out, each for a reason:

- Extractors: by design, a handler takes one `Request` and asks it for what it needs.
- WebSocket over HTTP/3: browsers don't rely on it, and open their [WebSockets](websocket.md) over HTTP/2 or HTTP/1.1 instead.
- HTTP/2 server push and `Upgrade: h2c`, which browsers and the HTTP/2 specification have dropped, and 0-RTT over HTTP/3, which exposes requests to replays.
- Runtimes other than tokio.
- A stable 1.x API: Vitesse is at version 0.1.

The [FAQ](faq.md#current-limitations) gives the reasons in detail.

## How this documentation is organised

- **Getting started**: [Installation](installation.md), then [Your first app](first-app.md), a step-by-step tutorial.
- **Essentials**: [Routing](routing.md), [Reading requests](requests.md), [Sending responses](responses.md), [HTML templates](templates.md), [Middleware](middleware.md), [Routers](routers.md), [Shared state](state.md), [Signed cookies and sessions](sessions.md), [Error handling](errors.md), [Static files](static-files.md) and [WebSocket](websocket.md).
- **Going further**: [Testing](testing.md), [Server configuration](server.md) (addresses, threads, timeouts, graceful shutdown), [HTTPS and HTTP/2](https.md), [HTTP/3 and QUIC](http3.md), [Performance](performance.md), [Coming from Express](from-express.md) and [Coming from axum or actix-web](from-axum-actix.md).
- **Deployment**: [Deploy to Heroku from your phone](heroku-mobile.md), [Docker](docker.md) and [Going to production](production.md).
- **Reference**: the [Cheat sheet](cheatsheet.md) and the [FAQ](faq.md).

> [!TIP]
> If you already know Express well, do [Your first app](first-app.md) and then keep [Coming from Express](from-express.md) open while you write your own routes.

The full API reference is generated from the source code. Run `cargo doc --open` in a project that depends on Vitesse, or browse the code on [GitHub](https://github.com/maxlestage/Vitesse).
