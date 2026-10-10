# Going to production

This page gathers what matters when a Vitesse app leaves your laptop: an optimised build, configuration, a reverse proxy for HTTPS and HTTP/2, a service manager, graceful shutdown, limits, logs, security and monitoring. For containers, see also [Docker](docker.md); for Heroku, [Deploy to Heroku from your phone](heroku-mobile.md).

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

## Behind a reverse proxy

Vitesse speaks HTTP/1.1 without TLS. In production, put a reverse proxy in front of it: it handles HTTPS and certificates, HTTP/2 (and HTTP/3), compression, and talks plain HTTP/1.1 to Vitesse over keep-alive connections on the local machine.

Vitesse closes a connection after about 60 seconds without a request (about 30 seconds if a client never finishes sending its headers). Let the proxy reuse its connections to the app, and have it drop idle ones a bit sooner than that.

### Caddy

[Caddy](https://caddyserver.com) is the simplest option: it gets HTTPS certificates automatically and enables HTTP/2 and HTTP/3 by default. A complete `Caddyfile`:

```text
example.com {
	encode zstd gzip

	reverse_proxy 127.0.0.1:3000 {
		header_up X-Real-IP {remote_host}
		transport http {
			keepalive 30s
		}
	}
}
```

Caddy reuses its connections to the app by default and sets `X-Forwarded-For`, `X-Forwarded-Proto` and `X-Forwarded-Host` itself. Apply changes with `sudo systemctl reload caddy`.

### Nginx

With [Nginx](https://nginx.org), keep-alive towards the app requires an `upstream` block, `proxy_http_version 1.1` and an empty `Connection` header:

```nginx
upstream vitesse {
    server 127.0.0.1:3000;
    keepalive 64;            # idle connections kept open to the app
    keepalive_timeout 30s;   # closed before Vitesse closes them (~60 s)
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
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

The certificates can come from Let's Encrypt, for example with `sudo certbot --nginx -d example.com`. Check the configuration with `sudo nginx -t`, then apply it with `sudo systemctl reload nginx`.

### Getting the client's IP address

Behind a proxy, `req.ip()` returns the proxy's address (`127.0.0.1`). Both configurations above send the real address in `X-Real-IP`:

```rust
app.get("/ip", |req: Request| async move {
    // Set by the reverse proxy; falls back to the TCP peer.
    req.header("x-real-ip")
        .map(str::to_owned)
        .or_else(|| req.ip().map(|ip| ip.to_string()))
        .unwrap_or_default()
});
```

> [!IMPORTANT]
> Only trust this header if the app can be reached **only** through the proxy (listen on `127.0.0.1`). Otherwise, any client can send a fake `X-Real-IP`.

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
# Vitesse lets in-flight requests finish for up to 10 s after SIGTERM.
TimeoutStopSec=15
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
2. lets in-flight requests finish, for up to **10 seconds**;
3. closes the remaining connections and returns from `app.run` (or `app.listen`).

The 10-second grace period is fixed. Make sure your platform waits a little longer before killing the process: systemd waits 90 seconds by default (`TimeoutStopSec`), Kubernetes 30 seconds (`terminationGracePeriodSeconds`), Heroku 30 seconds, but Docker only 10 seconds (use `--stop-timeout 15`).

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

Need JSON logs for a log collector? Write your own middleware (see [Middleware](middleware.md)):

```rust
app.middleware(|req: Request, next: Next| async move {
    let start = std::time::Instant::now();
    let (method, path) = (req.method().clone(), req.path().to_owned());
    let res = next.run(req).await;
    println!(
        "{}",
        json!({
            "method": method.as_str(),
            "path": path,
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
| Body read in memory (`json`, `form`, `text`, `bytes`) | 1 MiB, then `413` | `app.body_limit(bytes)` |
| Request head (request line and headers) | 60 KiB and 64 headers, then `431` | Fixed |
| Incomplete request head | About 30 seconds, then `408` | Fixed |
| Idle keep-alive connection | Closed after about 60 seconds | Fixed |
| Time spent in a handler | Unlimited | `middleware::timeout(duration)`, which answers `503` |

Lower `body_limit` to what your routes really need, and keep the proxy's limit (`client_max_body_size` in Nginx) consistent with it.

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

- **HTTPS everywhere**, handled by the proxy, with a redirect from HTTP to HTTPS.
- **The app is not exposed directly**: it listens on `127.0.0.1` (or a private network), and the firewall only opens ports 80 and 443.
- **An unprivileged user** runs the process (`User=` in systemd; the provided Docker image already uses a `vitesse` user).
- **Security headers** with `middleware::helmet()`: `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, `Strict-Transport-Security`… The last one tells browsers to only use HTTPS for one year, subdomains included: enable it once HTTPS works for the domain and its subdomains.
- **CORS** with an explicit list of allowed origins, and **cookies** marked `http_only`, `secure` and `same_site` (see below).
- **Limits**: a `body_limit` suited to your routes, a `timeout`, and rate limiting at the proxy level if needed (for example `limit_req` in Nginx): Vitesse doesn't include a rate limiter.
- **Error messages**: details of `5xx` errors are never sent to clients, but the messages of the errors you create (`Error::bad_request("…")`) are: don't put anything sensitive in them.
- **Static files**: `ServeDir` rejects `../` and hidden files (`.env`, `.git`) by default; still, serve a dedicated folder, never the project root.
- **Secrets** live in environment variables or a secret manager, never in the repository or the image.
- **Dependencies**: update them regularly (`cargo update`) and check security advisories with [cargo-audit](https://github.com/rustsec/rustsec/tree/main/cargo-audit).

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

Prometheus can scrape this text format. Don't expose `/metrics` publicly: block it at the proxy.
