# Performance

Vitesse was built to add as little work as possible around what the kernel already does for each request. This page shows the benchmark results, how they were measured and how to reproduce them, explains where the speed comes from, and gives practical tips to keep your own application fast.

## Benchmark

Requests per second and, in brackets, the CPU time used by the server for each request (lower is better):

| Scenario | Express 5 | Drogon 1.9 (C++) | axum 0.8 | actix-web 4 | **Vitesse** |
|---|---:|---:|---:|---:|---:|
| `GET /` (text) | 5,921 (180 µs) | 203,244 (9.7 µs) | 168,165 (11.7 µs) | 274,195 (7.1 µs) | **291,415 (6.1 µs)** |
| `GET /json` | 5,765 (184 µs) | 120,489 (16.5 µs) | 165,988 (11.9 µs) | 248,987 (7.9 µs) | **313,438 (5.9 µs)** |
| `GET /json` sent by a browser (12 headers) | 5,627 (188 µs) | 92,041 (21.7 µs) | 131,710 (15.0 µs) | 179,505 (10.9 µs) | **290,478 (6.8 µs)** |
| `GET /users/:id` (parameter + JSON) | 5,627 (187 µs) | 98,486 (20.1 µs) | 151,303 (13.1 µs) | 209,173 (9.4 µs) | **304,323 (6.2 µs)** |
| `POST /echo` (reads and returns JSON) | 4,538 (235 µs) | 69,876 (28.4 µs) | 105,570 (18.8 µs) | 170,129 (11.7 µs) | **253,941 (7.6 µs)** |
| `GET /` pipelined ×16 | 8,352 (128 µs) | 724,059 (2.7 µs) | 213,678 (9.3 µs) | 1,272,510 (1.6 µs) | **2,781,541 (0.64 µs)** |

- **Against actix-web**, the Rust framework with the reputation of being the fastest: up to **+62%** throughput with a real browser request, +45 to +49% with parameters or a JSON body, **2.2 times more** with pipelining, and 15 to 59% less CPU per request.
- **Against axum**: 1.7 to 2.4 times more requests per second, half the CPU per request, and 13 times more with pipelining.
- **Against Drogon (C++)**: 1.4 to 3.8 times faster.
- **Against Express**: about 50 times faster.

> [!NOTE]
> The Express numbers (Express 5.3.0 on Node 22.22) were measured in the same session as the others. The Express server has since been removed from the repository to keep the project 100% Rust, with no JavaScript: it is still in the git history (`git show 484eed3:bench/express/server.js`), and the benchmark runner now compares Drogon, axum, actix-web and Vitesse.

### Why is the gap with actix smaller on `GET /`?

On the simplest possible request, every fast server hits the same floor: about 4.7 µs of kernel work per request (reading, writing and, over the loopback interface, processing the client-side reception, which is charged to the server's send). Vitesse adds only ~1.4 µs on top of that, actix ~2.4 µs and axum ~7 µs. As soon as the request looks like a real one (browser headers, parameters, JSON body, pipelining), the framework's own code makes the difference, and the gap widens. In production, over a real network, the kernel's share on the server side is smaller, which makes Vitesse's advantage even more visible.

## Methodology

- **Machine**: a 4-vCPU VM. The server is pinned to 2 cores and [wrk](https://github.com/wg/wrk) to the other 2, so the load generator never competes with the server.
- **Load**: 128 keep-alive connections, 10 seconds per scenario, after a 2-second warm-up, on the same machine and in the same session for every server.
- **Versions**: Node 22.22 / Express 5.3.0, Drogon 1.9.13 (GCC 13, `-O3`), axum 0.8, actix-web 4.15, Rust 1.97, and the system allocator everywhere.
- **Scenarios**: the "browser" scenario sends the 12 headers of a real Chrome request (about 650 bytes); `POST /echo` sends a small JSON document that the server parses and sends back; the pipelined scenario sends 16 requests at once on each connection.
- **CPU time per request**: the CPU time (user + system) consumed by the server process during the scenario, divided by the number of requests served. Without pipelining, the fastest servers saturate wrk itself: the CPU time measured on the server side is then the most reliable judge.

Results vary by a few percent from one run to the next.

## Reproducing the benchmark

The benchmark runner is a small Rust program, [`bench/runner`](https://github.com/maxlestage/Vitesse/blob/master/bench/runner/src/main.rs), and the code of every server is in [`bench/`](https://github.com/maxlestage/Vitesse/tree/master/bench) (the Vitesse server is [`examples/bench.rs`](https://github.com/maxlestage/Vitesse/blob/master/examples/bench.rs)). You need Linux (the runner uses `taskset` and reads `/proc`), Rust and [wrk](https://github.com/wg/wrk). Drogon is optional: if it is installed, the runner builds its server from `bench/drogon` with CMake (set `DROGON_PREFIX` to its install path if needed); otherwise it is skipped.

```sh
cargo run --release --manifest-path bench/runner/Cargo.toml              # 10 s per scenario, 128 connections
cargo run --release --manifest-path bench/runner/Cargo.toml -- 30s 256   # custom duration and number of connections
SERVER_CPUS=0-3 CLIENT_CPUS=4-7 cargo run --release --manifest-path bench/runner/Cargo.toml   # on an 8-core machine
```

The runner builds every server in release mode, starts them one after the other (Drogon, axum, actix-web, Vitesse) with the server and wrk pinned to separate cores, plays the six scenarios (the wrk scripts are in `bench/lua/`), and prints the results as a Markdown table, with the server's CPU time per request read from `/proc`. `SERVER_CPUS` (default `0,1`) and `CLIENT_CPUS` (default `2,3`) take lists in the `taskset` format, such as `0,1`, `0-3` or `0-1,4`; the number of server CPUs also sets the number of server threads and of wrk threads. To use another wrk binary, set `WRK=/path/to/wrk`.

To quickly try Vitesse alone:

```sh
cargo run --release --example bench    # listens on port 3000
wrk -t2 -c128 -d10s http://127.0.0.1:3000/json
```

The bench server also reads `PORT`, `WORKERS`, and `VITESSE_MODE=mt` to use tokio's multi-threaded runtime instead of one thread per core.

## Why it is fast

Almost all the time spent on a simple request goes into the kernel (reading from and writing to the socket): a fast server is one that adds as little as possible around it. Vitesse does exactly **one `read` and one `write` per request**, and only one of each for a whole batch of pipelined requests.

- **Its own HTTP/1.1 engine** ([`src/http1.rs`](https://github.com/maxlestage/Vitesse/blob/master/src/http1.rs)):
  - the request head is parsed by [httparse](https://github.com/seanmonstar/httparse) (SIMD), and headers are only recorded by their position. The `HeaderMap` and the `Uri` are only built if a handler asks for them: a browser request and its twelve headers cost almost as little as a bare request;
  - responses are serialised straight into a reused write buffer: common status lines and content types are precomputed, the `Date` header is cached per thread, and a `HeaderMap` is only created if you add other headers;
  - a handler that answers without waiting follows a fully synchronous path, with no intermediate `Future` and no copy of large structures;
  - a single timer per connection (not per request) handles idle connections.
- **Almost no allocations**: requests (and their buffers) are recycled per thread, so are response header maps, and the router writes parameters into reused buffers. All that remains is the handler's `Future` (and the buffer of a JSON body).
- **One thread per core** (Linux): each core has its own event loop and its own `SO_REUSEPORT` socket; the kernel spreads the connections and a request never changes thread.
- **A router without regular expressions**: a tree of path segments, walked without any allocation for static routes; parameters point into the path.
- **No shared atomic counter per request**: the application is frozen at startup (`&'static`), so handlers, middleware and state are read without `Arc`.
- **Few copies**: the request travels through middleware and handlers by moving a single pointer, a response weighs only 72 bytes, and the body is only read if the handler asks for it.

Speed does not come at the expense of robustness: see the [limits built into the engine](server.md).

## Keeping your app fast

### Compile in release mode

Debug builds are many times slower: never measure or deploy them. Run `cargo build --release` (or `cargo run --release`), and enable the same optimisations as Vitesse in your `Cargo.toml`:

```toml
[profile.release]
lto = "fat"         # optimise across crates, including Vitesse
codegen-units = 1   # slower build, faster binary
```

> [!WARNING]
> Do not add `panic = "abort"`: Vitesse catches panics to turn them into `500` responses, and with `abort` a single panicking handler would bring the whole server down.

### Never block the event loop

Handlers run on a small number of threads. A blocking call (heavy computation, password hashing, `std::fs`, a synchronous database driver, `std::thread::sleep`) freezes every other connection of its thread while it runs. Use async APIs (`tokio::fs`, async drivers) and move CPU-heavy work to tokio's blocking thread pool:

```rust
app.post("/hash", |req: Request| async move {
    let password = req.text().await?;
    // Runs on tokio's blocking thread pool: the event loop stays free.
    let hash = tokio::task::spawn_blocking(move || expensive_hash(&password)).await?;
    Ok::<_, Error>(hash)
});
```

Likewise, never hold a `std::sync::Mutex` lock across an `.await`. If many handlers really must block, see [`thread_per_core(false)`](server.md).

### Create expensive resources once

Database pools, HTTP clients and compiled templates should be created once at startup and shared through the [state](state.md), not rebuilt for every request:

```rust
struct Services {
    http: reqwest::Client, // keeps a pool of connections, reused by every request
}

app.state(Services { http: reqwest::Client::new() });
app.get("/weather", |req: Request| async move {
    let services = req.state::<Services>();
    let body = services
        .http
        .get("https://example.com/weather")
        .send()
        .await?
        .text()
        .await?;
    Ok::<_, Error>(body)
});
```

`req.state::<T>()` returns a plain reference, with no lock and no reference counting.

### Read only what you need

The body is only read if you call `req.json()`, `req.text()`, `req.bytes()` or `req.form()`: a route that does not need it pays nothing. For large uploads, `req.take_body()` hands you the stream so you can process it piece by piece instead of loading it all in memory. Keep `body_limit` as small as your use case allows.

### Prefer static data and typed JSON

```rust
#[derive(serde::Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

// Serialised directly into the response buffer: no intermediate tree.
app.get("/health", |_| async { Json(Health { status: "ok", version: "1.0" }) });

// Builds a `serde_json::Value` (a few allocations) before serialising it.
app.get("/health-dyn", |_| async { json!({ "status": "ok", "version": "1.0" }) });

// Static data: never copied.
app.get("/robots.txt", |_| async { "User-agent: *\nDisallow:\n" });
app.get("/pixel", |_| async { Bytes::from_static(b"GIF89a") });
```

`json!` is perfect for prototypes and rarely-called routes; on hot paths, a `#[derive(Serialize)]` struct wrapped in `Json` avoids building an intermediate value. A `&'static str` or a `Bytes::from_static` is sent without any copy (`Bytes` is re-exported as `vitesse::Bytes`).

### Keep global middleware light

Global middleware runs for every request, including 404s. Do expensive checks in route or router middleware, only where they are needed (see [Middleware](middleware.md)). For example, `middleware::logger()` writes a line to standard output for every request: very handy, but measure its cost under heavy load.

### Measure

Measure the release build, from a load generator that does not compete with the server for the same cores:

```sh
cargo run --release
wrk -t2 -c128 -d10s http://127.0.0.1:3000/
oha -z 10s -c 128 http://127.0.0.1:3000/
```

[wrk](https://github.com/wg/wrk) and [oha](https://github.com/hatoo/oha) both report throughput and latency. Look at latency percentiles as well as requests per second, and measure on production-like data: an application is usually limited by its database long before it is limited by Vitesse.
