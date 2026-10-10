# HTTP/3 and QUIC

HTTP/3 carries HTTP over QUIC, a transport built on UDP with TLS 1.3 built in: connections open faster, a lost packet only delays the request it belongs to, and a connection survives a change of network (from Wi-Fi to mobile data, say). Vitesse can serve your application over HTTP/3 next to HTTP/1.1: the same routes, the same middleware and the same handlers, with one more line of configuration.

## Enable the `http3` feature

HTTP/3 is an opt-in Cargo feature:

```toml
[dependencies]
vitesse = { version = "0.1", features = ["http3"] }
```

It brings in [quinn](https://github.com/quinn-rs/quinn) (QUIC), [h3](https://github.com/hyperium/h3) (HTTP/3) and [rustls](https://github.com/rustls/rustls) (TLS 1.3, with the ring cryptography library): all Rust, no OpenSSL to install. The feature is off by default because it adds compilation time that most applications behind a proxy don't need.

## A complete example

```rust
use vitesse::http3::Http3;
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());
    app.get("/", |req: Request| async move {
        format!("Hello over {:?}!", req.version()) // HTTP/1.1 or HTTP/3.0
    });

    // QUIC always uses TLS: a certificate and its private key, in PEM.
    app.http3(Http3::from_pem_files("fullchain.pem", "privkey.pem")?);

    // HTTP/1.1 on TCP port 443, HTTP/3 on UDP port 443.
    app.run(443)
}
```

`app.http3(...)` adds a UDP socket next to the TCP one. Every request, whatever its protocol, goes through the same global middleware, routes, router middleware and `app.on_error`. In a handler, `req.version()` is `HTTP/3.0` for a request that came over HTTP/3, and the `Host` header is rebuilt from the `:authority` pseudo-header, so `req.header("host")` and `req.hostname()` work as with HTTP/1.1.

HTTP/3 works with the three ways of starting the server: `app.run(addr)`, `app.listen(addr).await` and `app.bind(addr).await?` (see [below](#bind-and-http3_addr)). The UDP socket listens on the same IP address as TCP.

## Certificates

QUIC can't run without TLS, so Vitesse needs a certificate for your domain and its private key. Three constructors:

| Constructor | Use |
|---|---|
| `Http3::from_pem_files(chain, key)` | Two PEM files, e.g. Let's Encrypt's `fullchain.pem` and `privkey.pem` |
| `Http3::from_pem(chain_bytes, key_bytes)` | The same, from memory (a secret manager, an environment variable…) |
| `Http3::from_rustls(config)` | Your own `rustls::ServerConfig` |

The chain contains the server certificate first, then the intermediate certificates: that is exactly what `fullchain.pem` holds. The private key can be in PKCS#8, SEC1 (EC) or PKCS#1 (RSA) format, in PEM. A file that can't be read or parsed makes `from_pem_files` return an `io::Error`, before the server starts.

> [!IMPORTANT]
> The certificate is loaded once, at startup. Let's Encrypt renews certificates every two months or so: restart the application after each renewal, for example from a certbot hook (`certbot renew --deploy-hook "systemctl restart my-app"`).

### Your own rustls configuration

For client certificates (mutual TLS), several domains on one server, or certificate reloading without restarting, build the rustls configuration yourself. `vitesse::http3::rustls` re-exports the version of rustls that Vitesse uses, so the types always match:

```rust
use std::sync::Arc;

use vitesse::http3::rustls::pki_types::pem::PemObject;
use vitesse::http3::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use vitesse::http3::{Http3, rustls};
use vitesse::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let chain = CertificateDer::pem_file_iter("fullchain.pem")?.collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_file("privkey.pem")?;

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])? // QUIC requires TLS 1.3
        .with_no_client_auth()
        .with_single_cert(chain, key)?;

    let mut app = App::new();
    app.get("/", |_| async { "Hello!" });
    app.http3(Http3::from_rustls(tls)); // the "h3" ALPN protocol is added if missing
    app.run(443)?;
    Ok(())
}
```

### In development

To try HTTP/3 on your machine, generate a self-signed certificate for `localhost`:

- in Rust, with the [rcgen](https://crates.io/crates/rcgen) crate, at startup (as a dev-dependency or in an example);
- or with [mkcert](https://github.com/FiloSottile/mkcert), which creates a certificate trusted by your own machine: `mkcert -install`, then `mkcert localhost` writes `localhost.pem` and `localhost-key.pem`.

```rust
// Cargo.toml: rcgen = "0.14"
let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
    .map_err(std::io::Error::other)?;
app.http3(Http3::from_pem(
    cert.cert.pem().as_bytes(),
    cert.signing_key.serialize_pem().as_bytes(),
)?);
```

The repository's [`examples/http3.rs`](https://github.com/maxlestage/Vitesse/blob/master/examples/http3.rs) does exactly this, and listens on port 4433. Try it with a curl built with HTTP/3 support (`curl --version` lists `HTTP3` among its features):

```sh
cargo run --example http3 --features http3

curl -k --http3-only https://localhost:4433/   # Hello over HTTP/3.0!
curl -i http://localhost:4433/                 # HTTP/1.1, with the alt-svc header
```

`-k` accepts the self-signed certificate.

## Options

| `Http3` method | Default | Effect |
|---|---|---|
| `.port(u16)` | The same number as the TCP port | The UDP port to listen on |
| `.alt_svc(bool)` | `true` | Adds the `alt-svc` header to HTTP/1.1 responses |
| `.alt_svc_port(u16)` | The UDP port actually opened | The public port announced in `alt-svc`, when clients reach the server on another port (behind a proxy, Docker or NAT: usually `443`) |

```rust
app.http3(
    Http3::from_pem_files("fullchain.pem", "privkey.pem")?
        .port(8443)          // Vitesse listens on UDP 8443...
        .alt_svc_port(443),  // ...which clients reach on port 443 (NAT, Docker)
);
```

## How browsers switch to HTTP/3

A browser never starts with HTTP/3. It first connects over TCP with HTTPS, and switches to HTTP/3 for the following requests if the response tells it the service exists, with the `Alt-Svc` header. Vitesse adds this header to every HTTP/1.1 response:

```http
alt-svc: h3=":443"; ma=86400
```

The browser then remembers for a day (`ma=86400`) that this site answers in HTTP/3 on UDP port 443.

Browsers only trust `Alt-Svc` on an HTTPS site. Vitesse doesn't do TLS over TCP, so a browser that reaches it directly in plain HTTP never switches. In production, the setup is therefore:

1. a TLS reverse proxy (Caddy, Nginx) on TCP port 443, which forwards requests to Vitesse in HTTP/1.1;
2. Vitesse directly on UDP port 443, with the same certificate as the proxy;
3. `alt-svc` announcing port `443`: the proxy passes the header on to browsers, which then talk to Vitesse directly over HTTP/3.

> [!TIP]
> Caddy serves HTTP/3 by default, with its own certificates. If Caddy is your proxy, the simplest option is to let it do HTTP/3 itself and not enable the feature. Enable it when your proxy doesn't do HTTP/3, or when you want Vitesse to answer HTTP/3 clients without going through the proxy.

Clients outside a browser (`curl --http3`, mobile apps, other services) don't need `Alt-Svc`: they can use HTTP/3 directly if they know the server supports it.

## In production behind Caddy or Nginx

Vitesse serves HTTP/1.1 to the proxy on a private TCP port, and HTTP/3 to the internet on UDP port 443:

```rust
use vitesse::http3::Http3;
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());
    app.get("/", |_| async { "Hello!" });

    // The same certificate as the proxy (copies readable by the service's user).
    app.http3(
        Http3::from_pem_files("/etc/my-app/tls/fullchain.pem", "/etc/my-app/tls/privkey.pem")?
            .port(443),         // HTTP/3: UDP 443, straight from the internet
    );

    // HTTP/1.1 for the proxy on TCP 3000. The UDP socket uses the same IP,
    // so listen on 0.0.0.0, and keep TCP 3000 closed in the firewall.
    app.run("0.0.0.0:3000")
}
```

`alt-svc` announces the UDP port that is actually open, here `443`. If clients reach Vitesse through a port translation (Docker `-p 443:8443/udp`, a NAT, a firewall redirect), listen on the inside port with `.port(8443)` and announce the public one with `.alt_svc_port(443)`.

On the proxy side, keep the usual configuration (see [Going to production](production.md#behind-a-reverse-proxy)):

- **Nginx**: TLS on TCP 443 and `proxy_pass` to `127.0.0.1:3000`, without `listen 443 quic`, so UDP port 443 stays free for Vitesse. Nginx passes Vitesse's `alt-svc` header on to browsers.
- **Caddy**: turn off its own HTTP/3 to free UDP port 443, in the global options at the top of the `Caddyfile`:

  ```text
  {
  	servers {
  		protocols h1 h2
  	}
  }
  ```

Under systemd, an unprivileged user can't open port 443: add `AmbientCapabilities=CAP_NET_BIND_SERVICE` to the `[Service]` section. Let's Encrypt keys are only readable by root: copy them to a folder the service's user can read (from the certbot hook that also restarts the app).

> [!WARNING]
> HTTP/3 requests don't go through the proxy. `req.ip()` is then the real client address, but the `X-Real-IP` and `X-Forwarded-For` headers come straight from the client: only trust them on HTTP/1.1 requests (`req.version()`). Likewise, what the proxy does (compression, rate limiting, access logs, body size limit) doesn't apply to HTTP/3 requests: `app.body_limit` does.

## `bind` and `http3_addr`

With `app.bind`, `server.http3_addr()` returns the UDP address (`None` without `app.http3`), which is handy with port `0`:

```rust
use vitesse::http3::Http3;
use vitesse::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "ok" });
    app.http3(Http3::from_pem_files("cert.pem", "key.pem")?);

    let server = app.bind("127.0.0.1:0").await?;
    println!("HTTP/1.1 on tcp://{}", server.local_addr());
    if let Some(udp) = server.http3_addr() {
        println!("HTTP/3 on udp://{udp}"); // the same port number as TCP
    }

    server.run().await
}
```

`server.run()` and `server.with_graceful_shutdown(signal)`, like `app.run` and `app.listen`, stop both protocols together: on shutdown, HTTP/3 connections receive a `GOAWAY` (the client opens no new request on them) and requests in progress get up to 10 seconds to finish, as in HTTP/1.1 (see [Server configuration](server.md#graceful-shutdown)).

## What changes, and what isn't supported (yet)

- **Request bodies** are read in full before the handler runs, up to `app.body_limit` (`413` beyond). `req.take_body()` therefore gives a body that is already in memory, not a stream: streaming request bodies over HTTP/3 is not supported yet.
- **Responses** are streamed as in HTTP/1.1 (`Body::from_stream`, files…). Vitesse adds `date` and `content-length` (when the size is known), and removes the headers specific to HTTP/1.1 (`connection`, `transfer-encoding`, `upgrade`, `keep-alive`).
- **Threads**: with `app.run` in thread-per-core mode (the default on Linux), HTTP/3 is served by a single thread, next to the HTTP/1.1 threads. If most of your traffic is HTTP/3, compare with `app.thread_per_core(false)`.
- **Not supported**: [WebSocket](websocket.md) over HTTP/3 (clients open WebSocket connections over HTTP/1.1), HTTP/2, 0-RTT (early data), and streaming request bodies.

## Firewall and hosting

QUIC uses **UDP**: opening TCP port 443 is not enough.

- **Firewall**: open the UDP port, for example `sudo ufw allow 443/udp`, or a UDP rule in your cloud provider's security group.
- **Docker**: declare the port with `EXPOSE 443/udp` and publish it with `-p 443:443/udp` (see [Docker](docker.md#http3-publish-the-udp-port)).
- **Heroku**: its router doesn't forward UDP to dynos, so HTTP/3 isn't available on Heroku. Leave the feature off there.
- **Cloud Run, Render and similar platforms** terminate HTTP/3 at their edge, when they offer it, and talk HTTP/1.1 to your container: no need for the feature there either.

If HTTP/3 is blocked somewhere along the way (a corporate firewall that drops UDP, for example), browsers silently fall back to HTTP/1.1 or HTTP/2 through the proxy: enabling HTTP/3 never cuts anyone off.
