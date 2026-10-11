# Server configuration

This page covers everything around the server itself: the three ways to start it, listen addresses and Unix sockets, threads, body size, timeouts and connection limits, the limits built into the HTTP engine, graceful shutdown and the protocols it speaks. The defaults are designed for production, so most apps only ever need `app.run(port)`.

## Three ways to start the server

| | `app.run(addr)` | `app.listen(addr).await` | `app.bind(addr).await?` + `Server` |
|---|---|---|---|
| Needs `#[tokio::main]` | No | Yes | Yes |
| Threads | One per core, see `workers` and `thread_per_core` | Those of your runtime | Those of your runtime |
| Stops cleanly on `Ctrl+C` / `SIGTERM` | Yes | Yes | Yes with `run()`, or on your own signal with `with_graceful_shutdown` |
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

    app.listen(3000).await // until Ctrl+C / SIGTERM, on the current runtime
}
```

This needs tokio in your `Cargo.toml` (`tokio = { version = "1", features = ["full"] }`).

> [!NOTE]
> Like `run`, `listen` stops cleanly on `Ctrl+C` or `SIGTERM` and lets requests in progress finish (see [Graceful shutdown](#graceful-shutdown)). On the other hand, `app.workers` and `app.thread_per_core` have no effect: the threads are those of your runtime.

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

    server.run().await // until Ctrl+C / SIGTERM, like app.run
}
```

| `Server` method | Role |
|---|---|
| `server.local_addr()` | The address actually used (handy with port `0`) |
| `server.http3_addr()` | The UDP address of [HTTP/3](http3.md), if `app.http3` is configured (`http3` feature) |
| `server.run().await` | Serves until `Ctrl+C` / `SIGTERM`, then shuts down cleanly |
| `server.with_graceful_shutdown(signal).await` | Serves until the `signal` future completes, then shuts down cleanly |

`with_graceful_shutdown` replaces `Ctrl+C` / `SIGTERM` with a future of your choice: a channel in [tests](testing.md), or the usual signals plus some code of your own, as in [Going to production](production.md#graceful-shutdown).

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

## Unix domain sockets

Behind a reverse proxy on the same machine, Vitesse can listen on a Unix domain socket, a file such as `/run/my-app/app.sock`, instead of a TCP port. No port to choose or to firewall: the file's permissions decide who may connect. Three methods mirror the TCP ones:

| Method | Like | Notes |
|---|---|---|
| `app.run_unix(path)` | `app.run` | Creates its own tokio runtime: the multi-threaded one, with `workers` threads (no thread-per-core mode) |
| `app.listen_unix(path).await` | `app.listen` | On your runtime |
| `app.bind_unix(path).await?` | `app.bind` | Returns a `UnixServer`: `path()`, `run().await`, `with_graceful_shutdown(signal).await` |

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.trust_proxy(1); // one proxy in front (Nginx): it reports the client's address
    app.get("/", |req: Request| async move { format!("Hello {:?}!", req.ip()) });

    app.run_unix("/run/my-app/app.sock") // until Ctrl+C / SIGTERM
}
```

- **The socket file**: a file left over by a previous run is replaced; if a server still answers on it, you get an `AddrInUse` error instead, and a regular file at that path is never deleted. The file is removed when the server stops. The folder must exist and be writable by the app.
- **Permissions**: the file is created with the process's umask. To let the proxy's user connect, change them with `bind_unix`, before serving: `std::fs::set_permissions(server.path(), Permissions::from_mode(0o660))` (see [Going to production](production.md#over-a-unix-socket) for a complete setup).
- **No client address**: a Unix socket has no IP, so `req.ip()` and `req.remote_addr()` are `None`. With `app.trust_proxy(...)` (usually a number of hops: `1` for a single proxy), the peer counts as a trusted proxy: `req.ip()` reads `X-Forwarded-For`, and `req.protocol()` and `req.hostname()` read `X-Forwarded-Proto` and `X-Forwarded-Host`.
- **HTTP/3** is not opened with a Unix socket (QUIC needs a UDP port): use `app.run` for it.
- **Unix only**: these methods don't exist on Windows. They need no Cargo feature.

On the proxy side, Nginx connects with `proxy_pass http://unix:/run/my-app/app.sock;` and Caddy with `reverse_proxy unix//run/my-app/app.sock`.

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

The body is only read if the handler asks for it, so routes that never read it are unaffected. `req.take_body()`, which hands you the raw stream (proxies), is not limited: counting the bytes is then up to you. [Multipart forms](requests.md) (`req.multipart()`) are limited too, even though they are read as a stream.

`app.body_limit` sets the default for the whole application. To accept larger bodies on a few routes only (uploads), or smaller ones, change the limit per route or per router with the `body_limit` middleware:

```rust
async fn upload(req: Request) -> vitesse::Result<String> {
    let bytes = req.bytes().await?;
    Ok(format!("{} bytes", bytes.len()))
}

let mut app = App::new(); // 1 MiB everywhere...
app.post("/upload", upload.with(middleware::body_limit(50 * 1024 * 1024))); // ...50 MiB here
```

`req.body_limit()` returns the limit that applies to a request, and `req.set_body_limit(bytes)` changes it from your own middleware. Over [HTTP/3](http3.md), request bodies are read before routing, within `app.body_limit`: raise that one if HTTP/3 clients upload large files. See [Body size limit and 413](requests.md#body-size-limit-and-413) for the details.

## Timeouts and connections

Four settings control the connections themselves, like actix-web's `HttpServer`:

```rust
use std::time::Duration;

let mut app = App::new();
app.keep_alive(Duration::from_secs(75))        // idle keep-alive connections (default 60 s)
    .header_timeout(Duration::from_secs(10))   // time to send the request head (default 30 s)
    .shutdown_timeout(Duration::from_secs(25)) // grace period at shutdown (default 10 s)
    .max_connections(10_000);                  // open connections per accept loop (default: unlimited)
```

| Setting | Default | Effect |
|---|---|---|
| `keep_alive(duration)` | 60 s | A keep-alive connection without a request is closed after this delay (HTTP/2 included). `Duration::ZERO` disables keep-alive: every response carries `connection: close` |
| `header_timeout(duration)` | 30 s | A client that hasn't sent a complete request head by then gets `408 Request Timeout`, and the connection is closed: protection against clients that open connections and send nothing (slowloris) |
| `shutdown_timeout(duration)` | 10 s | The time given to requests in progress at [graceful shutdown](#graceful-shutdown), HTTP/3 included |
| `max_connections(n)` | Unlimited | Open connections per accept loop; extra connections wait in the system's queue until one closes |

The delays are checked by one timer per connection: they are respected to within half of the shorter of `keep_alive` and `header_timeout`.

When to change them:

- **`keep_alive`**: behind a proxy or a load balancer, keep it longer than the proxy's own idle timeout, so the proxy always closes first and never reuses a connection that Vitesse is closing (an AWS load balancer waits 60 s by default: use 65 s or more).
- **`header_timeout`**: lower it (10 s) when the app faces the internet directly, without a proxy.
- **`shutdown_timeout`**: raise it if some requests (reports, uploads) take longer than 10 s, and keep it shorter than the time your platform waits before killing the process.
- **`max_connections`**: to cap the memory and file descriptors used by connections. With `app.run` in thread-per-core mode, there is one accept loop per thread, so the total is `n` × the number of threads; in the other modes, `n` is the total. It counts connections, not requests: an HTTP/2 connection can carry many requests at once. To cap the requests being handled, use `middleware::concurrency_limit` (see [Middleware](middleware.md)).

## Limits built into the engine

The HTTP/1.1 engine protects the server against malformed or abusive requests. The size limits are fixed; the delays can be changed (see [Timeouts and connections](#timeouts-and-connections)):

| Situation | Behaviour |
|---|---|
| Request head (request line and headers) over 60 KiB | `431 Request Header Fields Too Large`, connection closed |
| More than 64 headers | `431`, connection closed |
| Malformed request | `400 Bad Request`, connection closed |
| Both `Content-Length` and `Transfer-Encoding`, or two different `Content-Length` values | `400` (protection against request smuggling) |
| `Transfer-Encoding` whose last coding is not `chunked` | `501 Not Implemented` |
| Body over the limit (`body_limit`), when read | `413 Payload Too Large` |
| Keep-alive connection without a request for about 60 s (`keep_alive`) | Connection closed |
| Request head still incomplete after about 30 s (`header_timeout`) | `408 Request Timeout`, connection closed |
| TLS handshake not completed within 10 s ([HTTPS](https.md)) | Connection closed |
| `Expect: 100-continue` | `100 Continue` sent automatically before the body is read |
| `chunked` request body | Decoded transparently |
| Body up to 64 KiB | Read before the handler is called; larger bodies are streamed to the handler as they arrive |
| Large body the handler does not read | Response sent with `connection: close`, then the end of the upload is drained (2 s and 8 MiB at most) so the client receives the response |
| Pipelined requests | Answered in order; the responses of a batch leave in a single system call |
| HTTP/1.0 | Connection closed after the response, unless the client asks for keep-alive |
| Panic in a handler or a middleware | `500`, and the server keeps running |

On the response side, the engine computes `content-length`, adds the `date` header, sends bodies of unknown size (streams) in `chunked` encoding, sends no body for `204`, `304` and `HEAD` requests, and closes the connection after the response if your handler sets a `connection: close` header. HTTP/2 has its own limits: see [HTTPS and HTTP/2](https.md#limits-and-what-isnt-supported).

> [!IMPORTANT]
> There is no time limit on the handler itself: a handler that waits forever keeps its request open forever. Add the `timeout` middleware, which answers `503 Service Unavailable` after the given duration.

```rust
app.middleware(middleware::timeout(Duration::from_secs(30))); // std::time::Duration
```

To protect the server from load peaks and abusive clients, see also `middleware::concurrency_limit` and `middleware::rate_limit` in [Middleware](middleware.md).

## Graceful shutdown

With `app.run`, `app.listen` and `Server::run` (and their Unix socket versions), `Ctrl+C` (SIGINT) and, on Unix, `SIGTERM` trigger a graceful shutdown:

1. the server stops accepting new connections;
2. requests in progress get up to **10 seconds** (`shutdown_timeout`) to finish, and their responses carry `connection: close`; HTTP/2 connections receive a `GOAWAY`;
3. the remaining connections (idle keep-alive connections, requests still running when the grace period ends) are closed, and `run` (or `listen`) returns `Ok(())`.

With [HTTP/3](http3.md), the UDP side stops at the same time: open connections receive a `GOAWAY`, and their requests in progress get the same grace period. [WebSocket](websocket.md) connections are not waited for: they are closed at the latest when the grace period ends, so clients should reconnect.

This is exactly what Docker, Kubernetes or Heroku expect: they send `SIGTERM` and wait a while before forcing the process to stop. With `bind`, `with_graceful_shutdown(signal)` follows the same steps once your `signal` future completes, so you decide what triggers the shutdown; `Ctrl+C` and `SIGTERM` are then no longer watched, so include them in your future if you still need them.

The grace period is 10 seconds by default. Change it with `shutdown_timeout`, keeping it a little shorter than the time your platform waits before killing the process (30 seconds on Kubernetes and Heroku, 10 seconds on Docker):

```rust
app.shutdown_timeout(Duration::from_secs(25)); // std::time::Duration
```

## Protocols and HTTPS

| Protocol | Cargo feature | How |
|---|---|---|
| HTTP/1.1 (and HTTP/1.0) | Built in | Always, over TCP or a [Unix socket](#unix-domain-sockets) |
| [WebSocket](websocket.md) | `ws` (enabled by default) | `app.ws(...)`, over HTTP/1.1: `ws://`, or `wss://` with TLS |
| [HTTPS](https.md) (TLS 1.2 and 1.3) | `tls` | `app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?)`: every TCP connection speaks TLS |
| [HTTP/2](https.md) over TLS | `tls` and `http2` | Negotiated during the TLS handshake (ALPN): what browsers use |
| [HTTP/2 in clear text](https.md#http2-in-clear-text-h2c) (h2c) | `http2` | For the clients that start in HTTP/2 ("prior knowledge"), on the same port as HTTP/1.1 |
| [HTTP/3](http3.md) over QUIC | `http3` | `app.http3(...)`, on a UDP port next to the TCP one; `server.http3_addr()` returns its address |

The same routes, middleware and handlers answer every protocol, and `req.version()` tells them apart. With `tls` and `http3` together, a single binary serves HTTPS, HTTP/2 and HTTP/3 to browsers, with no proxy in front: see [HTTPS and HTTP/2](https.md).

### Behind a reverse proxy

You can also leave TLS to a reverse proxy (Nginx, Caddy, or your platform's load balancer) that forwards requests to Vitesse in HTTP/1.1, as is often done with Express: [HTTPS and HTTP/2](https.md#tls-directly-or-a-reverse-proxy) compares both options.

Behind a proxy, `req.ip()` returns the proxy's address for the requests it forwards, and `req.protocol()` says `"http"`: the client's address and protocol are in the `X-Forwarded-For` and `X-Forwarded-Proto` headers. Declare the proxy with `app.trust_proxy(...)` and Vitesse reads them for you:

```rust
app.trust_proxy("loopback"); // a proxy on the same machine
```

All of this is covered in [Going to production](production.md#getting-the-clients-ip-address).
