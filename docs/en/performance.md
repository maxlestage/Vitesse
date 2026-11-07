# Performance

Vitesse was built to add as little work as possible around what the kernel already does for each request. This page shows the benchmark results, how they were measured and how to reproduce them, explains where the speed comes from, and gives practical tips to keep your own application fast.

## Benchmark

Requests per second and, in brackets, the CPU time used by the server for each request (lower is better), measured with Vitesse's new engine per thread:

| Scenario | axum 0.8 | actix-web 4 | **Vitesse** |
|---|---:|---:|---:|
| `GET /` (text) | 233,459 (8.4 µs) | **359,229 (5.4 µs)** | 347,836 (**5.1 µs**) |
| `GET /json` | 215,706 (9.1 µs) | 346,652 (5.6 µs) | **350,972 (5.1 µs)** |
| `GET /json` sent by a browser (12 headers) | 179,989 (11.0 µs) | 256,421 (7.7 µs) | **349,424 (5.3 µs)** |
| `GET /users/:id` (parameter + JSON) | 204,665 (9.6 µs) | 304,271 (6.4 µs) | **359,307 (5.2 µs)** |
| `POST /echo` (reads and returns JSON) | 164,843 (12.0 µs) | 227,272 (8.7 µs) | **365,321 (5.3 µs)** |
| `GET /` pipelined ×16 | 366,673 (5.4 µs) | 1,612,135 (1.2 µs) | **3,288,486 (0.51 µs)** |

- **Against actix-web**, the Rust framework reputed to be the fastest: +1.2% on `GET /json` and −3% on `GET /`, where both are held back by the kernel and the load generator (Vitesse still uses 6% less CPU there), **+36%** with a real browser request, **+18%** with a parameter, **+61%** on the JSON echo, **twice as many** with pipelining, and 6 to 58% less CPU per request. With 2,048 connections, pipelined: 3.0 million requests per second against 1.27 million (×2.4).
- **Against axum**: 1.5 to 2.2 times more requests per second, 1.7 to 2.3 times less CPU per request, and 9 times more with pipelining.
- **Against Drogon (C++) and Express**: measured in an earlier session, on another machine, before the engine per thread. Vitesse was then 1.4 to 3.8 times faster than Drogon and about 50 times faster than Express.

> [!NOTE]
> Drogon and Express could not be measured again on this machine: the comparison above comes from an earlier session, with Drogon 1.9.13 (GCC 13, `-O3`) and Express 5.3.0 on Node 22.22. The Express server has since been removed from the repository to keep the project 100% Rust, with no JavaScript: it is still in the git history (`git show 484eed3:bench/express/server.js`), and the benchmark runner now compares Drogon, axum, actix-web and Vitesse.

### Why is the gap with actix smaller on `GET /`?

On the simplest request, most of the ~5 µs a request costs is kernel work (reading, writing and, on the loopback interface, the client-side receive billed to the server's send), and the load generator itself runs out of breath: neither server uses all of its two cores. The framework's code is a small share, and both servers end up at the same floor. As soon as the request looks like a real one (browser headers, parameters, JSON body, pipelining), that code makes the difference and the gap widens. In production, over a real network, the server-side share of kernel work is smaller, which makes Vitesse's advantage even more visible.

## Methodology

- **Machine**: a 4-vCPU VM, faster than the one used for earlier measurements (so compare the columns with each other, not with older numbers). The server is pinned to 2 cores and wrk to the other 2, so the load generator never competes with the server.
- **Load**: 128 keep-alive connections, 5 seconds per run and the median of 3 runs for each scenario, with the servers one after the other, all in a single session on November 7, 2026.
- **Versions**: axum 0.8, actix-web 4.18, Rust 1.97. Vitesse uses its default allocator (mimalloc), axum and actix-web the system allocator.
- **Scenarios**: the "browser" scenario sends the 12 headers of a real Chrome request (about 650 bytes); `POST /echo` sends a small JSON document that the server parses and sends back; the pipelined scenario sends 16 requests at once on each connection.
- **CPU time per request**: the CPU time (user + system) consumed by the server process during the scenario, divided by the number of requests served. Without pipelining, the fastest servers saturate wrk itself: the CPU time measured on the server side is then the most reliable judge.

Results vary by a few percent from one run to the next.

## Reproducing the benchmark

The benchmark runner is a small Rust program, `bench/runner`, and the code of every server is in `bench/` (the Vitesse server is `examples/bench.rs`). You need Linux (the runner uses `taskset` and reads `/proc`), Rust and wrk. Drogon is optional: if it is installed, the runner builds its server from `bench/drogon` with CMake (set `DROGON_PREFIX` to its install path if needed); otherwise it is skipped.

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

- **Its own HTTP/1.1 engine** (`src/http1.rs`):
  - the request head is parsed by httparse (SIMD), and headers are only recorded by their position. The `HeaderMap` and the `Uri` are only built if a handler asks for them: a browser request and its twelve headers cost almost as little as a bare request;
  - responses are serialised straight into a reused write buffer: common status lines and content types are precomputed, the `Date` header is cached per thread, and a `HeaderMap` is only created if you add other headers;
  - a handler that answers without waiting follows a fully synchronous path, with no intermediate `Future` and no copy of large structures;
  - a single timer per connection (not per request) handles idle connections.
- **Almost no allocations**: requests (and their buffers) are recycled per thread, so are response header maps, and the router writes parameters into reused buffers. All that remains is the handler's `Future` (and the buffer of a JSON body).
- **A faster allocator by default**: the `mimalloc` feature, enabled by default, makes the whole program allocate with mimalloc. Measured against the system allocator: +10.5% requests per second and −9.9% CPU per request on `GET /json`, +6.9% and −10.9% on `POST /echo`; routes that barely allocate do not change. If your program sets its own `#[global_allocator]`, turn it off with `default-features = false, features = ["ws"]`.
- **One thread per core, and an engine per thread** (Linux): each core has its own event loop and its own `SO_REUSEPORT` socket; the kernel spreads the connections and a request never changes thread. The plain HTTP/1.1 connections of a core are watched by an epoll of their own: a request received whole, whose handler answers without waiting, is read, served and written without a task, a waker or a timer per connection. Anything else (a body still arriving, a handler that waits for a database, a streamed response, WebSocket, HTTP/2) hands the connection over to a task, which picks up where it was. After a busy turn, it keeps looking for events for 15 µs before letting the thread sleep: waking a sleeping thread costs more than these looks. Measured against the previous engine: up to +12% requests per second, and up to 13% less CPU per request.
- **A router without regular expressions on the common path**: a tree of path segments, walked without any allocation for static routes; parameters point into the path. Regular expressions only run for the segments that need them ([constraints](routing.md#constraints-with-regular-expressions), text around parameters), compiled once at startup.
- **No shared atomic counter per request**: the application is frozen at startup (`&'static`), so handlers, middleware and state are read without `Arc`.
- **Few copies**: the request travels through middleware and handlers by moving a single pointer, a response weighs only 72 bytes, and the body is only read if the handler asks for it.

### And in a real application

A real application adds middlewares and serves files: security headers (`helmet`), CORS, a request log, compression for browsers, a stylesheet. Measured with the requests of a browser (`examples/bench_app.rs`, server on 2 cores, wrk on 2 others, logs through a pipe), against the previous version of Vitesse:

| Request | Before | Now |
|---|---:|---:|
| `GET /api/users` (a JSON list of 50 users, compressed) | 4,684 req/s (393 µs) | **24,818 req/s (79 µs)**, ×5.3 |
| `GET /static/app.css` (20 KiB, compressed) | 1,667 req/s (1,127 µs) | **62,697 req/s (31 µs)**, ×37.6 |

Three changes make the difference: a response is compressed at a fast level on its own thread ([`compress`](middleware.md#compress)), a static file is kept in memory and compressed once at the best level ([static files](static-files.md#kept-in-memory)), and the log is written in batches ([`logger`](middleware.md#logger)).

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

Global middleware runs for every request, including 404s. Do expensive checks in route or router middleware, only where they are needed (see [Middleware](middleware.md)). `middleware::logger()` writes its lines in batches, every 50 ms, so it costs little; a logging middleware of your own that writes every line at once would cost a system call per request.

### Measure

Measure the release build, from a load generator that does not compete with the server for the same cores:

```sh
cargo run --release
wrk -t2 -c128 -d10s http://127.0.0.1:3000/
oha -z 10s -c 128 http://127.0.0.1:3000/
```

wrk and [oha](https://crates.io/crates/oha) both report throughput and latency. Look at latency percentiles as well as requests per second, and measure on production-like data: an application is usually limited by its database long before it is limited by Vitesse.
