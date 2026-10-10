**English** · [Français](README.fr.md) · [Español](README.es.md)

# Vitesse ⚡

**The comfort of Express.js, the speed of Rust.**

[![CI](https://github.com/maxlestage/Vitesse/actions/workflows/ci.yml/badge.svg)](https://github.com/maxlestage/Vitesse/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/vitesse.svg)](https://crates.io/crates/vitesse)
[![docs.rs](https://img.shields.io/docsrs/vitesse)](https://docs.rs/vitesse)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Vitesse is a minimalist web framework that brings the API of Express
(`app.get`, `req.params`, `res.status(201).json(...)`, `app.use`, `Router`,
`express.static`…) to native Rust, with its own HTTP/1.1 engine built on
[tokio](https://tokio.rs), WebSocket built in and HTTP/3 over QUIC as an
option.

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    app.run(3000)
}
```

- **Familiar**: routes and parameters, middleware with `next`, routers,
  static files, cookies, JSON and forms, and an in-memory test client.
- **Real time and modern protocols**: WebSocket with `app.ws`, and HTTP/3
  over QUIC with a single line (`http3` feature).
- **Fast**: faster than actix-web, axum and Drogon in the benchmark below.
- **Sturdy**: panics become `500` responses, size limits and timeouts are
  built in, and the server shuts down gracefully.

## Benchmark

Requests per second, and in parentheses the CPU time used by the server for
each request (lower is better):

| Scenario | Express 5 | Drogon 1.9 (C++) | axum 0.8 | actix-web 4 | **Vitesse** |
|---|---:|---:|---:|---:|---:|
| `GET /` (text) | 5,921 (180 µs) | 203,244 (9.7 µs) | 168,165 (11.7 µs) | 274,195 (7.1 µs) | **291,415 (6.1 µs)** |
| `GET /json` | 5,765 (184 µs) | 120,489 (16.5 µs) | 165,988 (11.9 µs) | 248,987 (7.9 µs) | **313,438 (5.9 µs)** |
| `GET /json` sent by a browser (12 headers) | 5,627 (188 µs) | 92,041 (21.7 µs) | 131,710 (15.0 µs) | 179,505 (10.9 µs) | **290,478 (6.8 µs)** |
| `GET /users/:id` (parameter + JSON) | 5,627 (187 µs) | 98,486 (20.1 µs) | 151,303 (13.1 µs) | 209,173 (9.4 µs) | **304,323 (6.2 µs)** |
| `POST /echo` (reads and returns JSON) | 4,538 (235 µs) | 69,876 (28.4 µs) | 105,570 (18.8 µs) | 170,129 (11.7 µs) | **253,941 (7.6 µs)** |
| `GET /` pipelined ×16 | 8,352 (128 µs) | 724,059 (2.7 µs) | 213,678 (9.3 µs) | 1,272,510 (1.6 µs) | **2,781,541 (0.64 µs)** |

- **Against actix-web**, the Rust framework reputed to be the fastest: up to
  **+62%** throughput with a real browser request, +45 to +49% with
  parameters or a JSON body, **2.2 times more** with pipelining, and 15 to
  59% less CPU per request.
- **Against axum**: 1.7 to 2.4 times more requests per second, half the CPU
  per request, and 13 times more with pipelining.
- **Against Drogon (C++)**: 1.4 to 3.8 times faster.
- **Against Express**: about 50 times faster.

**Why is the gap with actix smaller on `GET /`?** On the simplest request,
every fast server hits the same floor: about 4.7 µs of kernel work per
request (reading, writing and, on the loopback interface, processing the
client-side receive, which is billed to the server's send). Vitesse adds
only ~1.4 µs on top of that, actix ~2.4 µs and axum ~7 µs. As soon as the
request looks like a real one (browser headers, parameters, JSON body,
pipelining), the framework's code makes the difference and the gap widens.
In production, over a real network, the server-side share of kernel work is
smaller, which makes Vitesse's advantage even more visible.

<sub>4 vCPU VM: server pinned to 2 cores, [wrk](https://github.com/wg/wrk)
on the other 2, 128 keep-alive connections, 10 s per scenario, same machine
and same session for all. Node 22.22 / Express 5.3.0, Drogon 1.9.13 (GCC 13,
`-O3`), axum 0.8, actix-web 4.15, Rust 1.97, system allocator everywhere.
Without pipelining, the fastest servers saturate wrk: the CPU time per
request, measured on the server side, is then the most reliable judge.
Results vary by a few percent from one run to the next. To reproduce:
`cargo run --release --manifest-path bench/runner/Cargo.toml` (server
code in `bench/`). The Express server, measured in the same session, has
since been removed from the repository to keep the project 100% Rust, with
no JavaScript: it is still in the git history
(`git show 484eed3:bench/express/server.js`), and the runner now compares
Drogon, axum, actix-web and Vitesse.</sub>

## Installation

```sh
cargo add vitesse
cargo add serde --features derive   # for your JSON structs
```

Or, to follow the development version on GitHub:

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse" }
serde = { version = "1", features = ["derive"] }
```

Vitesse requires Rust 1.85 or later. No `#[tokio::main]` needed:
`app.run(port)` creates the runtime, uses every core and shuts down
gracefully on `Ctrl+C` / `SIGTERM`. If you already run a tokio runtime, use
`app.listen(port).await` instead.

### Cargo features

| Feature | Default | Adds |
|---|---|---|
| `ws` | Enabled | WebSocket: `app.ws(...)` and `vitesse::ws` ([guide](docs/en/websocket.md)) |
| `http3` | Disabled | HTTP/3 over QUIC: `app.http3(...)` and `vitesse::http3` ([guide](docs/en/http3.md)) |

```toml
[dependencies]
vitesse = { version = "0.1", features = ["http3"] }
```

## Quick tour

```rust
use serde::Deserialize;
use vitesse::prelude::*;

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

// Route middleware: only lets requests with the right token through.
async fn auth(req: Request, next: Next) -> Response {
    match req.header("authorization") {
        Some("Bearer secret") => next.run(req).await,
        _ => Error::unauthorized("missing or invalid token").into_response(),
    }
}

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    // Global middleware (app.use): runs for every request, in order.
    app.middleware(middleware::logger());
    app.middleware(|req: Request, next: Next| async move {
        let start = std::time::Instant::now();
        let res = next.run(req).await;
        res.header("x-response-time", format!("{:.3}ms", start.elapsed().as_secs_f64() * 1000.0))
    });

    app.get("/", |_| async { "Hello World!" });

    // Route parameters: a 400 is sent automatically if `id` is not a number.
    app.get("/users/:id", |req: Request| async move {
        let id: u32 = req.param_as("id")?;
        Ok::<_, Error>(Json(json!({ "id": id, "name": "Ada" })))
    });

    // JSON body in, 201 Created out (400 if the JSON is invalid).
    app.post("/users", |req: Request| async move {
        let user: NewUser = req.json().await?;
        Ok::<_, Error>((201, Json(json!({ "name": user.name }))))
    });

    // A router mounted under a prefix, protected by a middleware.
    let mut admin = Router::new();
    admin.middleware(auth);
    admin.get("/stats", |_| async { json!({ "users": 1 }) });
    app.mount("/admin", admin);

    app.run(3000)
}
```

```sh
curl localhost:3000/users/42      # {"id":42,"name":"Ada"}
curl -X POST localhost:3000/users -H 'content-type: application/json' -d '{"name":"Ada"}'
curl localhost:3000/admin/stats -H 'authorization: Bearer secret'
```

### WebSocket

```rust
// A WebSocket route, in the style of express-ws: the request, then the socket.
app.ws("/echo/:name", |req, mut socket| async move {
    let name = req.param("name").unwrap_or("stranger").to_owned();
    while let Some(Ok(message)) = socket.recv().await {
        if let ws::Message::Text(text) = message {
            if socket.send(format!("{name} said: {text}")).await.is_err() {
                break;
            }
        }
    }
});
```

Try it with [websocat](https://github.com/vi/websocat):
`websocat ws://localhost:3000/echo/ada`. Subprotocols, size limits,
`split` and a complete chat room: [WebSocket](docs/en/websocket.md).

### HTTP/3

```rust
use vitesse::http3::Http3;

// With the `http3` feature: the same app, also served over QUIC (UDP).
app.http3(Http3::from_pem_files("fullchain.pem", "privkey.pem")?);
app.run(443) // HTTP/1.1 on TCP 443, HTTP/3 on UDP 443
```

Certificates, `Alt-Svc` and deployment behind Caddy or Nginx:
[HTTP/3 and QUIC](docs/en/http3.md).

## Documentation

- **Website**: https://maxlestage.github.io/Vitesse/#/docs
- **In this repository**: [docs/en/README.md](docs/en/README.md), from
  [your first app](docs/en/first-app.md) to
  [going to production](docs/en/production.md), including a guide for
  [developers coming from Express](docs/en/from-express.md), and the
  [WebSocket](docs/en/websocket.md) and [HTTP/3](docs/en/http3.md) guides.
- **API reference**: [docs.rs/vitesse](https://docs.rs/vitesse)

The documentation is also available in [French](docs/fr/README.md) and
[Spanish](docs/es/README.md).

## Deploy to Heroku

[![Deploy to Heroku](https://www.herokucdn.com/deploy/button.svg)](https://www.heroku.com/deploy?template=https://github.com/maxlestage/Vitesse)

One tap deploys the demo app (`examples/demo.rs`) to your Heroku account:
Heroku builds the Docker image itself, so you don't even need a computer
(Heroku has no free plan, so a paid dyno is required). The step-by-step
guide also covers automatic deployments with GitHub Actions:
[Deploy to Heroku from your phone](docs/en/heroku-mobile.md).

## From Express to Vitesse

| Express | Vitesse |
|---|---|
| `const app = express()` | `let mut app = App::new();` |
| `app.get('/u/:id', (req, res) => …)` | `app.get("/u/:id", \|req: Request\| async move { … })` |
| `app.use(fn)` | `app.middleware(fn)` |
| `app.use('/api', router)` | `app.mount("/api", router)` |
| `express.Router()` | `Router::new()` |
| `app.use(express.static('public'))` | `app.middleware(ServeDir::new("public"))` |
| `app.ws('/chat', (ws, req) => …)` (express-ws) | `app.ws("/chat", \|req, socket\| async move { … })` |
| `express.json()` + `req.body` | `req.json::<T>().await?` |
| `next()` | `next.run(req).await` |
| `req.params.id` | `req.param("id")` or `req.param_as::<u64>("id")?` |
| `req.query.q` | `req.query("q")` or `req.query_as::<T>()?` |
| `res.send('text')` | return `"text"` |
| `res.status(201).json(obj)` | `res::status(201).json(obj)` or `(201, Json(obj))` |
| `res.redirect('/login')` | `Redirect::to("/login")` |
| `app.listen(3000)` | `app.run(3000)` |

The complete table is in [Coming from Express](docs/en/from-express.md).

## Why it's fast

Almost all the time of a simple request is spent in the kernel, reading and
writing the socket: a fast server is one that adds as little as possible
around it. Vitesse does exactly **one `read` and one `write` per request**,
and a single one of each for a whole batch of pipelined requests.

- **Its own HTTP/1.1 engine**: request heads are parsed by
  [httparse](https://github.com/seanmonstar/httparse) (SIMD) without
  copying, headers and URI are only built if a handler asks for them, and
  responses are written straight into a reused buffer.
- **Almost no allocations**: requests, buffers and response header maps are
  recycled per thread.
- **One thread per core** (Linux): each core has its own event loop and its
  own `SO_REUSEPORT` socket, and a request never changes thread.
- **A regex-free router**: a tree of segments, with no allocation for static
  routes.
- **Zero shared atomic counters per request**: the app is frozen at startup
  (`&'static`), so handlers, middleware and state are read without `Arc`.

Details in [Performance](docs/en/performance.md).

## Current limitations

Like Express, Vitesse deliberately does little. Not included (yet): HTTP/2
and TLS over TCP (put it behind a reverse proxy such as Nginx or Caddy, as
is often done with Express: see [Going to production](docs/en/production.md);
HTTP/3, whose TLS is built in, is supported), WebSocket over HTTP/3,
compression, template engines, and partial parameters within a segment
(`/flights/:from-:to`).

## Running the project

```sh
cargo run --release --example hello      # Hello World
cargo run --release --example rest_api   # complete CRUD API
cargo run --release --example demo       # the demo app deployed on Heroku (reads $PORT)
cargo run --release --example chat       # WebSocket chat room (websocat ws://localhost:3000/chat/ada)
cargo run --release --example http3 --features http3   # HTTP/1.1 + HTTP/3 on port 4433, self-signed certificate
cargo test                               # unit, integration and doc tests
cargo run --release --manifest-path bench/runner/Cargo.toml   # benchmark (Linux, wrk, and Drogon if installed)
docker build -t vitesse-demo . && docker run --rm -p 8080:8080 vitesse-demo
```

### The showcase website

The [`site/`](site) folder contains the project's website, written in Rust
with [Yew](https://yew.rs) and compiled to WebAssembly with
[Trunk](https://trunkrs.dev). The `Site` workflow publishes it to GitHub
Pages whenever it changes on `master`.

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
cd site && trunk serve --open            # http://127.0.0.1:8080, hot reload
```

## Contributing

Bug reports, ideas, documentation fixes and pull requests are welcome. For a
large change, please open an issue first so we can discuss it.

Before opening a pull request, run the same checks as the CI (which runs on
Linux, macOS and Windows):

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The benchmark runner ([`bench/runner`](bench/runner)) and the website
([`site/`](site)) are separate crates, outside the main build: if you change
them, also run `cargo test` in their folder.

The documentation lives in [`docs/`](docs) in English, French and Spanish:
when you change a page, please update the other languages too, or mention
it in your pull request. Notable changes go in
[`CHANGELOG.md`](CHANGELOG.md).

### Publishing a release (maintainers)

1. Bump `version` in `Cargo.toml` and update `CHANGELOG.md`.
2. On GitHub, open **Releases** → **Draft a new release**, create a tag
   `vX.Y.Z` matching the version, and publish the release.
3. The `Release` workflow runs the tests and publishes the crate to
   crates.io. It needs the repository secret `CARGO_REGISTRY_TOKEN` (a
   crates.io API token with the `publish-new` and `publish-update` scopes).
   It can also be started by hand from the **Actions** tab, as a dry run by
   default.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
