# Server configuration

This page covers everything around the server itself: the three ways to start it, listen addresses, threads, body size, the limits built into the HTTP engine and graceful shutdown. The defaults are designed for production, so most apps only ever need `app.run(port)`.

## Three ways to start the server

| | `app.run(addr)` | `app.listen(addr).await` | `app.bind(addr).await?` + `Server` |
|---|---|---|---|
| Needs `#[tokio::main]` | No | Yes | Yes |
| Threads | One per core, see `workers` and `thread_per_core` | Those of your runtime | Those of your runtime |
| Stops cleanly on `Ctrl+C` / `SIGTERM` | Yes | No | Yes, with `with_graceful_shutdown` |
| Knows the port before serving | No | No | Yes, with `local_addr()` |
| Typical use | Most apps, production | An existing tokio app | Tests, custom shutdown, port `0` |

### `app.run`: the default

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "Hello World!" });

    app.run(3000) // blocks until Ctrl+C / SIGTERM
}
```

`run` creates its own tokio runtime, starts one thread per core and blocks until the server is stopped. It returns an `io::Result<()>`: if the port is already taken, you get the error right away. It is also the fastest mode (see [Performance](performance.md)).

### `app.listen`: inside your own runtime

When you already have a tokio runtime, for example to run async initialisation before starting, use `listen`:

```rust
use vitesse::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Async initialisation before the server starts (database, config...).
    let greeting = tokio::fs::read_to_string("greeting.txt")
        .await
        .unwrap_or_else(|_| "Hello World!".to_string());

    let mut app = App::new();
    app.state(greeting);
    app.get("/", |req: Request| async move { req.state::<String>().clone() });

    app.listen(3000).await // runs forever, on the current runtime
}
```

This needs tokio in your `Cargo.toml` (`tokio = { version = "1", features = ["full"] }`).

> [!WARNING]
> `listen` serves forever: it does not catch `Ctrl+C` or `SIGTERM`, so the process is simply killed, together with the requests in progress. `app.workers` and `app.thread_per_core` have no effect either: the threads are those of your runtime. For a clean shutdown in your own runtime, use `bind` and `with_graceful_shutdown`.

### `app.bind` and `Server`: full control

`bind` opens the port without serving yet, and returns a `Server`:

```rust
use vitesse::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "ok" });

    let server = app.bind("127.0.0.1:0").await?; // port 0: the OS picks a free port
    println!("Listening on http://{}", server.local_addr());

    server.with_graceful_shutdown(shutdown_signal()).await
}

/// Resolves on Ctrl+C or, on Unix, on SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        signal(SignalKind::terminate())
            .expect("cannot listen for SIGTERM")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
```

| `Server` method | Role |
|---|---|
| `server.local_addr()` | The address actually used (handy with port `0`) |
| `server.run().await` | Serves forever |
| `server.with_graceful_shutdown(signal).await` | Serves until the `signal` future completes, then shuts down cleanly |

This is the mode used for [integration tests](testing.md) on a real port.

## Listen addresses

All three methods accept anything that implements the `ListenAddr` trait:

| Value | Listens on |
|---|---|
| `3000` (any integer) | `0.0.0.0:3000`: every IPv4 interface |
| `"3000"` | Same thing |
| `"127.0.0.1:8080"` | This machine only |
| `"[::]:3000"` | Every IPv6 interface |
| `"localhost:3000"` | The name is resolved; the first address that works is used |
| `String`, `&String` | Same as the equivalent `&str` |
| `SocketAddr` | That exact address |
| `([127, 0, 0, 1], 8080)`, `(Ipv4Addr::LOCALHOST, 8080)` | An `(IP, port)` pair |

A bare port listens on every interface, like `app.listen(3000)` in Express: that is what you want in a container or on a server. Use `127.0.0.1` to only accept connections from the machine itself. A port outside `0..=65535` returns an error at startup.

> [!TIP]
> Prefer an explicit IP to `localhost`: depending on the system, `localhost` may resolve to the IPv6 address `::1` first, and the server would then not answer on `127.0.0.1`.

### Reading the port from the environment

Hosting platforms (Heroku and others) give you the port in the `PORT` variable. Since a `String` holding a bare port is a valid address, this is all you need:

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "Hello!" });

    // PORT=8080 → 0.0.0.0:8080; no PORT → 0.0.0.0:3000
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    app.run(port)
}
```

See [Heroku](heroku-mobile.md) and [Docker](docker.md) for complete deployments.

## Threads: `workers` and `thread_per_core`

By default, `app.run` starts one thread per CPU available to the process. How they share the work depends on the mode:

- **Thread per core** (the default on Linux): each thread has its own event loop and its own listening socket (`SO_REUSEPORT`). The kernel spreads new connections across threads, and a connection never changes thread, with no synchronisation at all between cores. This is the fastest mode.
- **Multi-threaded runtime** (`thread_per_core(false)`, and always on systems other than Linux, such as macOS and Windows): a single socket and tokio's work-stealing runtime with `workers` threads. Idle threads take over pending work from busy ones.

```rust
let mut app = App::new();
app.workers(4)              // 4 threads instead of one per core
    .thread_per_core(false) // tokio's multi-threaded, work-stealing runtime
    .body_limit(10 * 1024 * 1024); // accept bodies up to 10 MiB
```

When to change these settings:

- **`workers(n)`** when the server shares the machine with other services (a database, a worker process), when you want to leave cores free, or when the number of CPUs detected in a container does not match the quota it really gets. `n` is at least `1`.
- **`thread_per_core(false)`** when handlers do long blocking computations (in thread-per-core mode, they hold up every connection of their thread), or when connections are few and very uneven: for example, a reverse proxy that keeps a handful of keep-alive connections open, which could all land on the same thread.
- Otherwise, keep the defaults.

These two settings only apply to `app.run`. To set them without recompiling, read them from the environment:

```rust
let mut app = App::new();
if let Some(n) = std::env::var("WORKERS").ok().and_then(|w| w.parse().ok()) {
    app.workers(n);
}
```

> [!TIP]
> Before switching modes because of slow handlers, move the blocking work to `tokio::task::spawn_blocking`: see [Performance](performance.md).

## Request body size: `body_limit`

```rust
app.body_limit(10 * 1024 * 1024); // 10 MiB
```

The default is `vitesse::DEFAULT_BODY_LIMIT`, 1 MiB. The limit applies whenever a handler reads the body with `req.bytes()`, `req.text()`, `req.json()` or `req.form()`: beyond it, the request fails with `413 Payload Too Large`. If the announced `Content-Length` is already too big, the 413 is returned without reading anything.

The body is only read if the handler asks for it, so routes that never read it are unaffected. `req.take_body()`, which hands you the raw stream (uploads, proxies), is not limited: counting the bytes is then up to you. The setting is global to the application. See [Requests](requests.md) for reading bodies.

## Limits built into the engine

The HTTP/1.1 engine protects the server against malformed or abusive requests. These values are fixed:

| Situation | Behaviour |
|---|---|
| Request head (request line and headers) over 60 KiB | `431 Request Header Fields Too Large`, connection closed |
| More than 64 headers | `431`, connection closed |
| Malformed request | `400 Bad Request`, connection closed |
| Both `Content-Length` and `Transfer-Encoding`, or two different `Content-Length` values | `400` (protection against request smuggling) |
| `Transfer-Encoding` whose last coding is not `chunked` | `501 Not Implemented` |
| Body over `body_limit`, when read | `413 Payload Too Large` |
| Keep-alive connection without a request for about 60 s | Connection closed |
| Request head still incomplete after about 30 s | `408 Request Timeout`, connection closed |
| `Expect: 100-continue` | `100 Continue` sent automatically before the body is read |
| `chunked` request body | Decoded transparently |
| Body up to 64 KiB | Read before the handler is called; larger bodies are streamed to the handler as they arrive |
| Large body the handler does not read | Response sent with `connection: close`, then the end of the upload is drained (2 s and 8 MiB at most) so the client receives the response |
| Pipelined requests | Answered in order; the responses of a batch leave in a single system call |
| HTTP/1.0 | Connection closed after the response, unless the client asks for keep-alive |
| Panic in a handler | `500`, and the server keeps running |

On the response side, the engine computes `content-length`, adds the `date` header, sends bodies of unknown size (streams) in `chunked` encoding, sends no body for `204`, `304` and `HEAD` requests, and closes the connection after the response if your handler sets a `connection: close` header.

> [!IMPORTANT]
> There is no time limit on the handler itself: a handler that waits forever keeps its request open forever. Add the `timeout` middleware, which answers `503 Service Unavailable` after the given duration.

```rust
app.middleware(middleware::timeout(Duration::from_secs(30))); // std::time::Duration
```

## Graceful shutdown

With `app.run`, `Ctrl+C` (SIGINT) and, on Unix, `SIGTERM` trigger a graceful shutdown:

1. the server stops accepting new connections;
2. requests in progress get up to **10 seconds** to finish, and their responses carry `connection: close`;
3. the remaining connections (idle keep-alive connections, requests still running after 10 s) are closed, and `run` returns `Ok(())`.

This is exactly what Docker, Kubernetes or Heroku expect: they send `SIGTERM` and wait a while before forcing the process to stop. With `bind`, `with_graceful_shutdown(signal)` follows the same steps once your `signal` future completes, so you decide what triggers the shutdown. `app.listen()` and `Server::run()` never stop by themselves. The 10-second grace period is not configurable.

## HTTP/1.1 only

Vitesse speaks HTTP/1.1 (and HTTP/1.0) over plain TCP. For HTTPS and HTTP/2, put it behind a reverse proxy (Nginx, Caddy, or your platform's load balancer) that terminates TLS and forwards requests to Vitesse in HTTP/1.1, as is often done with Express. The proxy can also handle compression.

Behind a proxy, `req.ip()` returns the proxy's address: the client's address is in the `X-Forwarded-For` header. All of this is covered in [Production](production.md).
