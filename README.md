[![Vitesse: the comfort of Express, the speed of Rust](https://raw.githubusercontent.com/maxlestage/Vitesse/master/site/assets/og-image.png)](https://vitesse-f16135654c3d.herokuapp.com/)

**English** · [Français](README.fr.md) · [Español](README.es.md)

# Vitesse ⚡

**The comfort of Express.js, the speed of Rust.**

[![crates.io](https://img.shields.io/crates/v/vitesse.svg)](https://crates.io/crates/vitesse)
[![docs.rs](https://img.shields.io/docsrs/vitesse)](https://docs.rs/vitesse)
[![License: proprietary](https://img.shields.io/badge/license-proprietary-2a2a2e.svg)](#license)

Vitesse is a minimalist web framework that brings the API of Express
(`app.get`, `req.params`, `res.status(201).json(...)`, `app.use`, `Router`,
`express.static`…) to native Rust, with its own HTTP/1.1 engine built on
[tokio](https://tokio.rs), WebSocket built in, and HTTPS, HTTP/2 and HTTP/3
over QUIC as options.

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    app.run(3000)
}
```

- **Familiar**: routes and parameters, middleware with `next`, routers,
  static files, cookies, JSON, forms and file uploads, and an in-memory test
  client.
- **Real time and modern protocols**: WebSocket with `app.ws`, Server-Sent
  Events, HTTPS and HTTP/2 (`tls` and `http2` features), and HTTP/3 over
  QUIC with a single line (`http3` feature).
- **Batteries included**: CORS, compression, request ids, Basic and Bearer
  authentication, rate and concurrency limits, trusted proxies, signed
  cookies and sessions (`session` feature), HTML templates (`templates`
  feature), and any [tower](https://crates.io/crates/tower) layer (`tower`
  feature).
- **Expressive routes**: `/flights/:from-:to`, `/users/:id(\d+)`,
  `/archive/:year?`, named routes and URL generation, route groups,
  `app.param` and route model binding.
- **APIs**: request validation with per-field `422` errors, OpenAPI 3.1
  generated from the routes with Swagger UI, lifecycle hooks (`on_request`,
  `pre_handler`, `on_response`) and plugins, RFC 9457 Problem Details,
  OpenAPI schemas derived from your types, Scalar and Redoc.
- **Batteries**: PubSub topics for WebSocket rooms and Server-Sent Events, a
  background job queue with retries and rate limits, a scheduler, a response
  cache, named middleware groups, typed routes, and test assertions with a
  cookie jar.
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

<sub>4 vCPU VM: server pinned to 2 cores, wrk
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
cargo add vitesse --git https://github.com/maxlestage/Vitesse
cargo add serde --features derive   # for your JSON structs
```

Vitesse requires Rust 1.85 or later. No `#[tokio::main]` needed:
`app.run(port)` creates the runtime, uses every core and shuts down
gracefully on `Ctrl+C` / `SIGTERM`. If you already run a tokio runtime, use
`app.listen(port).await` instead.

### Cargo features

| Feature | Default | Adds |
|---|---|---|
| `ws` | Enabled | WebSocket: `app.ws(...)` and `vitesse::ws` ([guide](docs/en/websocket.md)) |
| `tls` | Disabled | HTTPS: `app.tls(...)` and `vitesse::tls`, with rustls ([guide](docs/en/https.md)) |
| `http2` | Disabled | HTTP/2: through ALPN with `tls`, and h2c in clear text ([guide](docs/en/https.md)) |
| `http3` | Disabled | HTTP/3 over QUIC: `app.http3(...)` and `vitesse::http3` ([guide](docs/en/http3.md)) |
| `multipart` | Disabled | File uploads: `req.multipart()` and `vitesse::multipart` ([guide](docs/en/requests.md)) |
| `compress` | Disabled | `middleware::compress()` and `middleware::decompress()`: Brotli, gzip ([guide](docs/en/middleware.md#compress)) |
| `session` | Disabled | Signed cookies and sessions: `app.cookie_secret(...)`, `middleware::session(...)` ([guide](docs/en/sessions.md)) |
| `templates` | Disabled | HTML templates with minijinja: `app.views(...)`, `req.render(...)` ([guide](docs/en/templates.md)) |
| `schemars` | Disabled | OpenAPI schemas derived from your types ([guide](docs/en/openapi.md)) |
| `tower` | Disabled | tower layers as middleware, the app as a tower `Service` ([guide](docs/en/middleware.md)) |

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tls", "http2"] }
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

Try it with [websocat](https://crates.io/crates/websocat):
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

### HTTPS and HTTP/2

```rust
use vitesse::tls::Tls;

// With the `tls` and `http2` features: TLS with rustls, HTTP/2 for browsers.
app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?);
app.run(443)
```

Without TLS, the `http2` feature also answers clients that speak HTTP/2 in
clear text (h2c). See [HTTPS and HTTP/2](docs/en/https.md).

### Server-Sent Events

```rust
use vitesse::sse::Event;

app.get("/clock", |_| async {
    let (tx, events) = sse::channel(16);
    tokio::spawn(async move {
        for tick in 0.. {
            // Fails once the browser has gone away.
            if tx.send(Event::new().data(tick.to_string())).await.is_err() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });
    events
});
```

## Documentation

- **Website**: https://vitesse-f16135654c3d.herokuapp.com/en/docs/
- **In this repository**: [docs/en/README.md](docs/en/README.md), from
  [your first app](docs/en/first-app.md) to
  [going to production](docs/en/production.md), including a guide for
  [developers coming from Express](docs/en/from-express.md), and the
  [WebSocket](docs/en/websocket.md) and [HTTP/3](docs/en/http3.md) guides.
- **API reference**: [docs.rs/vitesse](https://docs.rs/vitesse)

The documentation is also available in [French](docs/fr/README.md) and
[Spanish](docs/es/README.md).

## Deploy to Heroku

Open `https://heroku.com/deploy?template=https://github.com/maxlestage/Vitesse` on your
phone to deploy the Vitesse website (`site/`, served by Vitesse itself, with its
documentation) to your Heroku account: Heroku reads `app.json` and compiles it itself
with the Rust buildpack (`emk/rust`; a Docker build of the demo app,
`examples/demo.rs`, is also available), so you don't even need a computer (Heroku has no free plan, so a paid dyno is required). The step-by-step
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
Coming from Rust's own frameworks? [Coming from axum or
actix-web](docs/en/from-axum-actix.md) maps their features, extractor by
extractor and layer by layer.

## Why it's fast

Almost all the time of a simple request is spent in the kernel, reading and
writing the socket: a fast server is one that adds as little as possible
around it. Vitesse does exactly **one `read` and one `write` per request**,
and a single one of each for a whole batch of pipelined requests.

- **Its own HTTP/1.1 engine**: request heads are parsed by
  httparse (SIMD) without
  copying, headers and URI are only built if a handler asks for them, and
  responses are written straight into a reused buffer.
- **Almost no allocations**: requests, buffers and response header maps are
  recycled per thread.
- **One thread per core** (Linux): each core has its own event loop and its
  own `SO_REUSEPORT` socket, and a request never changes thread.
- **A tree router**: a tree of segments, with no allocation for static
  routes; regular expressions only run for the parameters that declare a
  constraint.
- **Zero shared atomic counters per request**: the app is frozen at startup
  (`&'static`), so handlers, middleware and state are read without `Arc`.

Details in [Performance](docs/en/performance.md).

## Current limitations

Not included, on purpose: WebSocket over HTTP/3 (no browser uses it),
HTTP/2 server push (removed from browsers), the `Upgrade: h2c` mechanism
(removed from the HTTP/2 standard; h2c "prior knowledge" works) and 0-RTT
over HTTP/3 (replayable requests). tokio is the only runtime, and the API
may change until 1.0. The full list is in the [FAQ](docs/en/faq.md).

## Running the project

```sh
cargo run --release --example hello      # Hello World
cargo run --release --example rest_api   # complete CRUD API
cargo run --release --example demo       # the demo app, Heroku's Docker build (reads $PORT)
cargo run --release --example chat       # WebSocket chat room (websocat ws://localhost:3000/chat/ada)
cargo run --release --example http3 --features http3   # HTTP/1.1 + HTTP/3 on port 4433, self-signed certificate
cargo run --release --example https --features tls,http2   # HTTPS + HTTP/2 on port 3443, self-signed certificate
cargo run --release --example live --features multipart    # Server-Sent Events and file uploads
cargo test                               # unit, integration and doc tests
cargo run --release --manifest-path bench/runner/Cargo.toml   # benchmark (Linux, wrk, and Drogon if installed)
docker build -t vitesse-demo . && docker run --rm -p 8080:8080 vitesse-demo
```

### The showcase website

The [`site/`](site) folder contains the project's website, in English,
French and Spanish, with the documentation of [`docs/`](docs). It is served
by Vitesse itself: the server is a Vitesse app whose pages are written with
[Active](https://active-c1883a966650.herokuapp.com/) (server-rendered, with
complete SEO tags), and the interactive parts are active islands compiled to
WebAssembly. It is a Progressive Web App: it can be installed on a phone's
home screen and keeps working offline. It is served by Heroku only, at
<https://vitesse-f16135654c3d.herokuapp.com/>. The `Site` workflow checks it, builds its browser code and
checks that it can still be exported to static files.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129   # the version in site/Cargo.lock
cd site
# 1. The browser bundle (islands and animations) → site/pkg
cargo build --lib --target wasm32-unknown-unknown --profile wasm-release
wasm-bindgen --target web --no-typescript --out-dir pkg target/wasm32-unknown-unknown/wasm-release/vitesse_site.wasm
# 2. The server, on http://localhost:3000 (PORT to change it)
cargo run --release
# Or the whole site as static files, for any static host, behind a path prefix
BASE_PATH=/Vitesse SITE_URL=https://example.com/Vitesse cargo run --release -- export dist
```

The JavaScript that browsers need to start WebAssembly and the service
worker are generated (by wasm-bindgen and by the server): the repository
still contains no JavaScript file.

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
them, also run `cargo test` in their folder (for the website, also
`cargo clippy --lib --target wasm32-unknown-unknown -- -D warnings`).

The documentation lives in [`docs/`](docs) in English, French and Spanish:
when you change a page, please update the other languages too, or mention
it in your pull request. Notable changes go in
[`CHANGELOG.md`](CHANGELOG.md).

### Publishing a release (maintainers)

1. Bump `version` in `Cargo.toml` and write its section in `CHANGELOG.md`.
2. Push to `master`. Once CI is green, the `Release` workflow tags the new
   version (`vX.Y.Z`), creates its GitHub release with that section of
   `CHANGELOG.md` as notes, runs the tests and publishes the crate to
   crates.io. There is no form to fill in.
3. Publishing needs the repository secret `CARGO_REGISTRY_TOKEN` (a
   crates.io API token with the `publish-new` and `publish-update` scopes).
   Without it, the tag and the GitHub release are still created, and the
   publication waits: add the secret, then run the workflow by hand from
   the **Actions** tab with **Only check the package** unticked. A release drafted by hand
   (**Releases** → **Draft a new release**) publishes too, and a manual run
   is a dry run by default.

## License

Vitesse is proprietary software: © 2026 Maxime Nathan Lestage, all rights
reserved. It is not open source. The source code is published on crates.io
and GitHub so that it can be read, audited and built, and you may use it
freely as a dependency of your own programs, commercial ones included.
Modifying it or redistributing its source needs the author's written
permission. Full terms: [LICENSE](LICENSE).

Contributions (code, documentation, ideas) are welcome and become part of
the project under the same terms: by submitting one, you assign its rights
to the author, as the license states.

## Credits

Designed and developed by Maxime Nathan Lestage,
as part of one family of Rust projects: [Active](https://active-c1883a966650.herokuapp.com/), [Vitesse](https://vitesse-f16135654c3d.herokuapp.com/), [Securite](https://securite-2db4c1994be7.herokuapp.com/), [Memoire](https://memoire-7f4525f1fdaf.herokuapp.com/) and [Paquets](https://paquets-90dfc06b6fea.herokuapp.com/).
