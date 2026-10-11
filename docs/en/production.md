# Going to production

This page gathers what matters when a Vitesse app leaves your laptop: an optimised build, configuration, HTTPS (served by Vitesse or by a reverse proxy), the client's real address, WebSocket and HTTP/3, a service manager, graceful shutdown, limits, logs, security and monitoring. For containers, see also [Docker](docker.md); for Heroku, [Deploy to Heroku from your phone](heroku-mobile.md).

## Build in release mode

Always deploy a release build: a debug build is many times slower.

```sh
cargo build --release
```

For the best performance, add the profile used by Vitesse's benchmarks to **your** `Cargo.toml`:

```toml
[profile.release]
lto = "fat"
codegen-units = 1
```

Cargo only reads profiles from the root package (or workspace), so the settings in Vitesse's own `Cargo.toml` don't apply to your app. Compilation gets slower, the binary gets faster. Optionally, `strip = true` makes the binary smaller and `debug = "line-tables-only"` keeps readable backtraces and profiles.

> [!WARNING]
> Don't set `panic = "abort"`. Vitesse catches panics in handlers and turns them into `500` responses, so one faulty request can't take the server down. With `abort`, any panic kills the whole process.

The binary only depends on the system's C library (glibc): build it on the same Linux distribution as the server (or an older one), or build it in Docker. `RUSTFLAGS="-C target-cpu=native"` can help, but only when the binary runs on the machine that compiled it (or an identical CPU).

## Configuration from the environment

Read your settings from environment variables, so the same binary runs everywhere:

```rust
use std::time::Duration;

use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.middleware(middleware::logger()); // one line per request on stdout
    app.middleware(middleware::helmet()); // security headers
    app.middleware(middleware::timeout(Duration::from_secs(15))); // 503 if too slow
    app.body_limit(256 * 1024); // 256 KiB instead of 1 MiB
    app.trust_proxy("loopback"); // a reverse proxy on this machine (see below)

    app.get("/health", |_| async { "ok" });
    app.get("/", |_| async { "Hello from production!" });

    // Configuration comes from the environment.
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
    if let Some(n) = std::env::var("WORKERS").ok().and_then(|n| n.parse().ok()) {
        app.workers(n);
    }

    app.run(format!("{host}:{port}"))
}
```

- **`PORT`** is the convention used by most platforms (Heroku, Render, Cloud Run…).
- **`HOST`**: `0.0.0.0` in a container or on a platform; `127.0.0.1` behind a reverse proxy on the same machine, so the app can't be reached directly from outside. Use `[::]` for IPv6.
- A bare port number (`app.run(3000)`, or a string made only of digits) listens on every IPv4 interface.

`HOST` and `WORKERS` are just names chosen for this example: Vitesse itself doesn't read any environment variable.

## HTTPS: directly or through a proxy

Two setups work well:

- **Vitesse serves HTTPS itself**, with the `tls` and `http2` features (and `http3` if you like): one binary on ports 80 and 443, no other component. You handle certificate renewal (certbot, then a restart). See [HTTPS and HTTP/2](https.md).
- **A reverse proxy in front** (Caddy, Nginx): it handles HTTPS, certificates and HTTP/2, can host several apps on the same machine and balance the load across instances, and talks plain HTTP/1.1 to Vitesse. The next sections describe this setup.

On a platform (Heroku, Cloud Run, Render…), HTTPS is already handled at the platform's edge: neither is needed.

## Behind a reverse proxy

In this setup, the proxy handles HTTPS and certificates, HTTP/2 (and HTTP/3, unless Vitesse serves it itself: see [HTTP/3](#http3)), and talks plain HTTP/1.1 to Vitesse over keep-alive connections on the local machine. Compression can happen on either side (`encode` in Caddy, `gzip` in Nginx, or `middleware::compress()` in Vitesse).

Vitesse closes a connection after about 60 seconds without a request (`app.keep_alive`), and about 30 seconds if a client never finishes sending its headers (`app.header_timeout`). Let the proxy reuse its connections to the app, and have it drop idle ones a bit sooner than that. Declare the proxy with `app.trust_proxy(...)` so `req.ip()` returns the client's address ([below](#getting-the-clients-ip-address)).

### Caddy

[Caddy](https://caddyserver.com) is the simplest option: it gets HTTPS certificates automatically and enables HTTP/2 and HTTP/3 by default. A complete `Caddyfile`:

```text
example.com {
	encode zstd gzip

	reverse_proxy 127.0.0.1:3000 {
		transport http {
			keepalive 30s
		}
	}
}
```

Caddy reuses its connections to the app by default and sets `X-Forwarded-For`, `X-Forwarded-Proto` and `X-Forwarded-Host` itself (replacing the values a client may have sent). Apply changes with `sudo systemctl reload caddy`.

### Nginx

With [Nginx](https://nginx.org), keep-alive towards the app requires an `upstream` block, `proxy_http_version 1.1` and an empty `Connection` header:

```nginx
upstream vitesse {
    server 127.0.0.1:3000;
    keepalive 64;            # idle connections kept open to the app
    keepalive_timeout 30s;   # closed before Vitesse closes them (app.keep_alive, 60 s)
}

server {
    listen 80;
    listen [::]:80;
    server_name example.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    listen [::]:443 ssl;
    http2 on;                # Nginx < 1.25.1: "listen 443 ssl http2;" instead
    server_name example.com;

    ssl_certificate     /etc/letsencrypt/live/example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/example.com/privkey.pem;

    client_max_body_size 1m; # in line with app.body_limit

    location / {
        proxy_pass http://vitesse;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

The certificates can come from Let's Encrypt, for example with `sudo certbot --nginx -d example.com`. Check the configuration with `sudo nginx -t`, then apply it with `sudo systemctl reload nginx`.

### WebSocket behind Nginx

Caddy passes [WebSocket](websocket.md) connections through without any configuration. Nginx needs the `Upgrade` and `Connection` headers forwarded explicitly, and a long `proxy_read_timeout`: by default, it closes a connection that has been silent for 60 seconds. Add a `location` for your WebSocket routes (here, everything under `/ws/`) to the `server` block above:

```nginx
location /ws/ {
    proxy_pass http://vitesse;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_read_timeout 1h;
    proxy_send_timeout 1h;
}
```

Each WebSocket keeps its own connection between Nginx and Vitesse open for its whole life, outside the `keepalive` pool. For connections that can stay silent for a long time, have the server send a ping regularly (see [WebSocket](websocket.md#behind-a-reverse-proxy)).

### Over a Unix socket

When the proxy runs on the same machine, Vitesse can listen on a Unix domain socket instead of a TCP port (Linux and macOS): no port to keep closed, and the file's permissions decide who may connect. Open it with `bind_unix`, so you can set the permissions before serving:

```rust
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;

use vitesse::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.trust_proxy(1); // one proxy in front: the last X-Forwarded-For address is the client
    app.get("/", |req: Request| async move { format!("Hello {:?}!", req.ip()) });

    let server = app.bind_unix("/run/my-app/app.sock").await?;
    // Read and write for the app's user and its group: add Nginx's user to that group.
    std::fs::set_permissions(server.path(), Permissions::from_mode(0o660))?;
    server.run().await // until SIGTERM; the socket file is then removed
}
```

On the Nginx side, point the `upstream` at the socket and keep the rest of the configuration above:

```nginx
upstream vitesse {
    server unix:/run/my-app/app.sock;
    keepalive 64;
    keepalive_timeout 30s;
}
```

(Without an `upstream` block, `proxy_pass http://unix:/run/my-app/app.sock;` works too, without keep-alive.) Caddy connects with `reverse_proxy unix//run/my-app/app.sock`.

- Under systemd, `RuntimeDirectory=my-app` creates `/run/my-app`, owned by the service's user, at each start; add Nginx's user to the app's group with `sudo usermod -aG vitesse www-data`.
- A Unix socket has no client address: without `trust_proxy`, `req.ip()` is `None`. With it, the proxy's headers give the client's address, protocol and host.
- A socket file left over by a crash is replaced at the next start, and the file is removed on a clean shutdown. [HTTP/3](#http3) needs a UDP port: it isn't opened with a Unix socket.

See [Server configuration](server.md#unix-domain-sockets) for the details.

### Getting the client's IP address

Behind a proxy, the connection comes from the proxy: by default, `req.ip()` returns its address (`127.0.0.1`), and `req.protocol()` says `"http"` even for visitors on HTTPS. The proxy passes the real values in the `X-Forwarded-For`, `X-Forwarded-Proto` and `X-Forwarded-Host` headers. Tell Vitesse which proxies to believe with `app.trust_proxy`, like the `trust proxy` setting of Express:

```rust
let mut app = App::new();
app.trust_proxy("loopback"); // Caddy or Nginx on the same machine

app.get("/whoami", |req: Request| async move {
    Json(json!({
        "ip": req.ip(),             // the client, from X-Forwarded-For
        "protocol": req.protocol(), // "https", from X-Forwarded-Proto
        "host": req.hostname(),     // from X-Forwarded-Host, or Host
    }))
});
```

| `trust_proxy(...)` | Trusts | Use |
|---|---|---|
| `false` (the default) | Nobody: `req.ip()` is the address of the connection | No proxy |
| `"loopback"` | The proxies at `127.0.0.1` or `::1` | A proxy on the same machine |
| `"10.0.0.0/8, 192.168.1.10"` | The proxies at these addresses or in these networks (CIDR) | A load balancer in a private network |
| `"uniquelocal"`, `"linklocal"` | Private networks (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `fc00::/7`), link-local addresses | Containers, cloud private networks |
| `1`, `2`… | The `n` proxies closest to the app, whatever their address | A chain of known length, or a [Unix socket](#over-a-unix-socket) |
| `true` | Every proxy: the client is the left-most address of `X-Forwarded-For` | Only behind a proxy that replaces `X-Forwarded-For` (Caddy) instead of appending to it |

Names and networks can be combined: `"loopback, 10.0.0.0/8"`. An invalid address stops the app at startup; to handle the error yourself, parse the value with `"…".parse::<vitesse::TrustProxy>()`.

What changes for the requests that come through a trusted proxy:

- `req.ip()` walks `X-Forwarded-For` from the right, skipping the trusted proxies: the first address that isn't one is the client. `middleware::rate_limit` then counts per client, not per proxy;
- `req.protocol()` and `req.secure()` follow `X-Forwarded-Proto`;
- `req.host()` and `req.hostname()` read `X-Forwarded-Host`, when the proxy sends it (otherwise the `Host` header, which the Nginx configuration above passes on);
- `req.remote_addr()` stays the address of the connection itself: the proxy's.

> [!IMPORTANT]
> Only trust your own proxies. These headers are plain text: a client can send them too. With `true` or a number of hops, make sure the app can be reached **only** through the proxy (listen on `127.0.0.1` or a Unix socket); a list of addresses is safer, since the headers of any other peer are ignored.

If Vitesse also serves HTTP/3 (below), those requests reach it directly, without the proxy: their peer is the client itself. With a list of addresses such as `"loopback"`, that's handled for you: the client isn't a trusted proxy, so its `X-Forwarded-*` headers are ignored and `req.ip()` is its real address.

## HTTP/3

Browsers use HTTP/3 when the site announces it. Three ways to offer it:

- **Let the proxy do it.** Caddy enables HTTP/3 by default, Nginx with `listen 443 quic` (version 1.25 or later). Nothing changes for Vitesse.
- **Let Vitesse do it behind the proxy**, with the `http3` feature: the proxy keeps TCP port 443, and Vitesse receives UDP port 443 directly, with the same certificate. Its HTTP/1.1 responses carry the `alt-svc` header that the proxy passes on to browsers.
- **Let Vitesse do everything**, with the `tls` and `http3` features: HTTPS on TCP 443 and HTTP/3 on UDP 443, no proxy (see [HTTPS and HTTP/3 together](https.md#https-and-http3-together)).

The second case looks like this:

```rust
use vitesse::http3::Http3;

app.http3(
    Http3::from_pem_files("/etc/my-app/tls/fullchain.pem", "/etc/my-app/tls/privkey.pem")?
        .port(443), // HTTP/3 on UDP 443, announced to browsers by alt-svc
);
app.run("0.0.0.0:3000") // TCP 3000 for the proxy; the UDP socket uses the same IP
```

In that case:

- open **UDP** port 443 in the firewall (`sudo ufw allow 443/udp`), and keep TCP port 3000 closed;
- under systemd, add `AmbientCapabilities=CAP_NET_BIND_SERVICE` so the service's user can open port 443;
- Nginx must not listen on `443 quic`, and Caddy needs `protocols h1 h2` in its global options, to leave UDP port 443 to Vitesse;
- the certificate is loaded at startup: restart the app after each renewal.

Everything is detailed in [HTTP/3 and QUIC](http3.md#in-production-behind-caddy-or-nginx).

## Run it as a service with systemd

On a Linux server, systemd starts the app at boot, restarts it if it crashes and collects its logs. Create a dedicated user and copy the binary:

```sh
sudo useradd --system --no-create-home vitesse
sudo mkdir -p /opt/my-app
sudo cp target/release/my-app /opt/my-app/server
```

Then create `/etc/systemd/system/my-app.service`:

```ini
[Unit]
Description=My Vitesse app
After=network.target

[Service]
User=vitesse
Group=vitesse
WorkingDirectory=/opt/my-app
ExecStart=/opt/my-app/server
Environment=HOST=127.0.0.1
Environment=PORT=3000
# Or keep the settings in a separate file:
# EnvironmentFile=/etc/my-app.env
Restart=on-failure
RestartSec=2
# Vitesse lets in-flight requests finish for up to 10 s after SIGTERM
# (app.shutdown_timeout): keep this a few seconds longer.
TimeoutStopSec=15
# Only to open ports below 1024 (HTTPS on 443, HTTP/3 on UDP 443):
# AmbientCapabilities=CAP_NET_BIND_SERVICE
# Only for a Unix socket in /run/my-app:
# RuntimeDirectory=my-app
# One file descriptor per connection.
LimitNOFILE=65536
# Hardening (add ReadWritePaths=... if the app writes files).
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now my-app
systemctl status my-app
journalctl -u my-app -f        # follow the logs
```

To deploy a new version, replace the binary (copy it next to the old one, then rename it: copying over a running binary fails) and restart:

```sh
sudo cp target/release/my-app /opt/my-app/server.new
sudo mv /opt/my-app/server.new /opt/my-app/server
sudo systemctl restart my-app
```

A restart takes a fraction of a second, but connections attempted during it fail. For zero-downtime deployments, run two instances on two ports behind the proxy and restart them one after the other.

## Graceful shutdown

`app.run`, `app.listen(port).await` and `Server::run` listen for `Ctrl+C` (`SIGINT`) and `SIGTERM`, which systemd, Docker, Kubernetes or Heroku send before stopping an app. Vitesse then:

1. stops accepting new connections;
2. lets in-flight requests finish, for up to **10 seconds** by default;
3. closes the remaining connections and returns from `app.run` (or `app.listen`).

HTTP/2 and HTTP/3 connections stop at the same time, after a `GOAWAY`, with the same grace period for their requests. WebSocket connections are closed at the latest at the end of the grace period: make your clients reconnect automatically.

Change the grace period with `app.shutdown_timeout`, for example if some requests take longer than 10 seconds:

```rust
app.shutdown_timeout(Duration::from_secs(25)); // std::time::Duration
```

Make sure your platform waits a little longer than that before killing the process: systemd waits 90 seconds by default (`TimeoutStopSec`), Kubernetes 30 seconds (`terminationGracePeriodSeconds`), Heroku 30 seconds, but Docker only 10 seconds (`--stop-timeout 15` with the default grace period, `--stop-timeout 30` with the 25 seconds above).

To run some code of your own when the signal arrives (a log line, flushing a buffer), or to stop on another event, open the port with `app.bind` and pass your own future to `with_graceful_shutdown`. It replaces `Ctrl+C` and `SIGTERM`, so include them if you still need them:

```rust
use tokio::signal::unix::{SignalKind, signal};
use vitesse::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "ok" });

    let server = app.bind("0.0.0.0:3000").await?;
    println!("listening on http://{}", server.local_addr());

    let mut sigterm = signal(SignalKind::terminate())?;
    server
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = sigterm.recv() => {}
            }
            println!("shutting down…");
        })
        .await
}
```

This example (Unix only) needs `tokio = { version = "1", features = ["macros", "rt-multi-thread", "signal"] }` in your dependencies.

## Health checks

A cheap route that answers quickly is enough for load balancers, Kubernetes probes and uptime monitors:

```rust
app.get("/health", |_| async { "ok" });
```

If the app depends on a database, add a separate route (for example `/ready`) that checks it, and keep `/health` independent so a database outage doesn't get every instance restarted. Global middlewares, such as the logger, also run for these requests.

## Logging

- `middleware::logger()` writes one line per request on standard output, such as `GET /users/42 200 0.084 ms`. Colours are only used when the output is a terminal, so journald, Docker and platform logs stay clean.
- When a `5xx` error has a cause (a Rust error converted with `?`, or `Error::with_source`), the cause is printed on standard error with a `[vitesse]` prefix; the client only gets a generic message.
- A panic in a handler or a middleware becomes a `500`, and Rust prints the panic message on standard error.
- `middleware::request_id()` gives every request an id: the `X-Request-Id` sent by the proxy or the calling service, or a new UUID. It is sent back in the response's `X-Request-Id` header, so a user who reports an error can give you the id to look for in the logs.

Need JSON logs for a log collector? Write your own middleware (see [Middleware](middleware.md)). `req.route()` gives the pattern of the matched route (`/users/:id` rather than `/users/42`), which is easier to group and to search than the path (see [The matched route](routing.md#the-matched-route-reqroute)):

```rust
use vitesse::middleware::RequestId;

app.middleware(middleware::request_id()); // first, so the next middleware sees the id
app.middleware(|req: Request, next: Next| async move {
    let start = std::time::Instant::now();
    let method = req.method().clone();
    let path = req.path().to_owned();
    let route = req.route(); // before next.run, which takes the request
    let id = req.get::<RequestId>().map(|id| id.to_string());
    let res = next.run(req).await;
    println!(
        "{}",
        json!({
            "id": id,
            "method": method.as_str(),
            "path": path,
            "route": route,
            "status": res.status_code().as_u16(),
            "ms": start.elapsed().as_secs_f64() * 1000.0,
        })
    );
    res
});
```

## Limits

| What | Default | How to change it |
|---|---|---|
| Body read in memory (`json`, `form`, `text`, `bytes`, `multipart`) | 1 MiB, then `413` | `app.body_limit(bytes)`; per route or router: `middleware::body_limit(bytes)` |
| Request head (request line and headers) | 60 KiB and 64 headers, then `431` | Fixed |
| Incomplete request head | About 30 seconds, then `408` | `app.header_timeout(duration)` |
| Idle keep-alive connection | Closed after about 60 seconds | `app.keep_alive(duration)` |
| Open connections | Unlimited | `app.max_connections(n)`, per accept loop (per thread in thread-per-core mode) |
| Time spent in a handler | Unlimited | `middleware::timeout(duration)`, which answers `503` |
| Requests handled at the same time | Unlimited | `middleware::concurrency_limit(n)`: the others wait, or get `503` with `.max_wait(duration)` |
| Requests per client | Unlimited | `middleware::rate_limit(max, period)`, which answers `429` with `Retry-After` |
| Grace period at shutdown | 10 seconds | `app.shutdown_timeout(duration)` |

```rust
use std::time::Duration;

app.header_timeout(Duration::from_secs(10)); // facing the internet without a proxy
app.middleware(middleware::rate_limit(300, Duration::from_secs(60))); // 300 requests a minute per IP
app.post("/upload", upload.with(middleware::body_limit(20 * 1024 * 1024))); // 20 MiB on this route only
app.get(
    "/report",
    report.with(middleware::concurrency_limit(4).max_wait(Duration::from_secs(2))),
);
```

Lower `body_limit` to what your routes really need, raise it only on the routes that receive files, and keep the proxy's limit (`client_max_body_size` in Nginx) consistent with it.

`rate_limit` identifies clients by `req.ip()`: behind a proxy, set [`trust_proxy`](#getting-the-clients-ip-address), or every client shares the proxy's address and its quota. Its counters live in the process's memory: with several instances, each one counts on its own; for a limit shared across instances, rate-limit at the proxy (`limit_req` in Nginx) or in a shared store. See [Middleware](middleware.md) for every option.

## Workers and threads

- By default, `app.run` starts **one thread per core**. On Linux, each thread has its own event loop and its own `SO_REUSEPORT` socket: the kernel spreads connections across threads, and a request never changes thread. It's the fastest mode.
- `app.workers(n)` sets the number of threads explicitly: useful in a container whose CPU quota is lower than the number of cores it sees, or to leave cores to other processes (a database on the same machine, for instance).
- `app.thread_per_core(false)` switches to tokio's multi-threaded runtime, which rebalances work between threads. Prefer it if handlers run long computations, or if connections are few and very uneven. Behind a proxy, all traffic arrives through the proxy's pool of keep-alive connections: give it enough connections (`keepalive 64` in the Nginx example) so they spread over every thread, or switch this mode off if one core is saturated while the others stay idle.

Never block a thread inside a handler (`std::thread::sleep`, a long computation, a synchronous database driver…): every connection handled by that thread would wait. Move that work to tokio's blocking thread pool:

```rust
app.get("/report", |_| async {
    let report = vitesse::tokio::task::spawn_blocking(expensive_computation)
        .await
        .map_err(|e| Error::internal("the task failed").with_source(e))?;
    Ok::<_, Error>(report)
});
```

See [Server configuration](server.md) and [Performance](performance.md) for more.

## Security checklist

- **HTTPS everywhere**, served by [Vitesse itself](https.md) or by the proxy, with a redirect from HTTP to HTTPS.
- **Behind a proxy, the app is not exposed directly**: it listens on `127.0.0.1`, a private network or a [Unix socket](#over-a-unix-socket), and the firewall only opens ports 80 and 443 (TCP, plus UDP 443 if Vitesse serves [HTTP/3](#http3): it then listens on every interface, and the firewall keeps its TCP port closed).
- **`trust_proxy` lists only your proxies**, so that nobody can choose their own `req.ip()` with a fake `X-Forwarded-For` (see [above](#getting-the-clients-ip-address)).
- **An unprivileged user** runs the process (`User=` in systemd; the provided Docker image already uses a `vitesse` user).
- **Security headers** with `middleware::helmet()`: `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, `Strict-Transport-Security`… The last one tells browsers to only use HTTPS for one year, subdomains included: enable it once HTTPS works for the domain and its subdomains.
- **CORS** with an explicit list of allowed origins, and **cookies** marked `http_only`, `secure` and `same_site` (see below).
- **Limits**: a `body_limit` suited to your routes (raised only where files are uploaded), a `timeout`, and [rate limiting](#limits) with `middleware::rate_limit` on sensitive routes (login, password reset, expensive APIs), or at the proxy (`limit_req` in Nginx).
- **Authentication**: `middleware::basic_auth` and `middleware::bearer_auth` send the password or the token with every request: only use them over HTTPS. They compare credentials in constant time; keep the credentials themselves in environment variables. Add a `rate_limit` in front of them to slow down guessing.
- **Request ids** with `middleware::request_id()`, so that a user's error report can be matched with your logs (see [Logging](#logging)).
- **Error messages**: details of `5xx` errors are never sent to clients, but the messages of the errors you create (`Error::bad_request("…")`) are: don't put anything sensitive in them.
- **Static files**: `ServeDir` rejects `../` and hidden files (`.env`, `.git`) by default; still, serve a dedicated folder, never the project root.
- **Secrets** live in environment variables or a secret manager, never in the repository or the image.
- **Dependencies**: update them regularly (`cargo update`) and check security advisories with cargo-audit.

### CORS and cookies

```rust
use std::time::Duration;

use vitesse::SameSite;
use vitesse::prelude::*;

let mut app = App::new();

// Only this origin may call the API with the user's cookies.
app.middleware(
    middleware::cors()
        .allow_origin("https://app.example.com")
        .allow_credentials(true)
        .max_age(Duration::from_secs(600)),
);

app.post("/login", |_| async {
    Response::new()
        .cookie(
            Cookie::new("session", "abc123")
                .http_only(true)
                .secure(true)
                .same_site(SameSite::Lax),
        )
        .json(json!({ "ok": true }))
});
```

> [!WARNING]
> `cors().allow_credentials(true)` **without** `allow_origin` accepts every origin: any website could then send requests carrying your users' cookies.

## Monitoring

- **Logs**: `journalctl -u my-app`, `docker logs`, or your platform's log viewer; forward them to a log service if you need search and alerts.
- **Availability**: an external uptime monitor that calls `/health` every minute.
- **System**: watch CPU, memory and the number of open file descriptors (one per connection, limited by `LimitNOFILE` under systemd).

Vitesse has no built-in metrics, but a middleware and a few atomic counters cover the basics:

```rust
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

#[derive(Default)]
struct Metrics {
    requests: AtomicU64,
    server_errors: AtomicU64,
}

app.state(Metrics::default());

app.middleware(|req: Request, next: Next| async move {
    let metrics = req.state::<Metrics>();
    let res = next.run(req).await;
    metrics.requests.fetch_add(1, Relaxed);
    if res.status_code().is_server_error() {
        metrics.server_errors.fetch_add(1, Relaxed);
    }
    res
});

app.get("/metrics", |req: Request| async move {
    let m = req.state::<Metrics>();
    format!(
        "http_requests_total {}\nhttp_server_errors_total {}\n",
        m.requests.load(Relaxed),
        m.server_errors.load(Relaxed),
    )
});
```

Prometheus can scrape this text format. Don't expose `/metrics` publicly: block it at the proxy, or protect the route with `middleware::bearer_auth(token)`, added with `.with()`, and give the token to Prometheus.

To count requests or measure durations per route, label them with `req.route()`: `/users/:id` gives one series for every user, where the path would create a new series for each id. Read it before `next.run(req)`, and `None` means no route matched (a `404`). See [The matched route](routing.md#the-matched-route-reqroute).
