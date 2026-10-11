# HTTPS and HTTP/2

Vitesse can serve HTTPS itself, with TLS over TCP, and speak HTTP/2 to the clients that offer it: every browser does. No reverse proxy is needed for either one, although a proxy remains a good option (see [the end of this page](#tls-directly-or-a-reverse-proxy)). The routes, middleware and handlers stay the same: only the configuration changes.

## Enable the `tls` and `http2` features

Both are opt-in Cargo features:

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tls", "http2"] }
```

| Feature | Adds | Brings in |
|---|---|---|
| `tls` | `app.tls(...)` and the `vitesse::tls` module: HTTPS, TLS 1.2 and 1.3 | [rustls](https://crates.io/crates/rustls) (with the ring cryptography library) and tokio-rustls: all Rust, no OpenSSL to install |
| `http2` | HTTP/2: through TLS with `tls`, and in clear text for the clients that start in HTTP/2 | [h2](https://crates.io/crates/h2), the HTTP/2 implementation used by hyper |

The two features are independent. `tls` alone serves HTTPS in HTTP/1.1; `http2` alone serves HTTP/2 without TLS ([h2c](#http2-in-clear-text-h2c)); together, they give browsers what they expect from a modern site.

## A complete example

```rust
use vitesse::prelude::*;
use vitesse::tls::Tls;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());
    app.get("/", |req: Request| async move {
        // "Hello over HTTP/2.0 (https)!" in a browser
        format!("Hello over {:?} ({})!", req.version(), req.protocol())
    });

    // A certificate and its private key, in PEM.
    app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?);

    app.run(443) // HTTPS on TCP port 443
}
```

Once `app.tls(...)` is set, every TCP connection on the port starts with a TLS handshake (a client gets 10 seconds to complete it). With the `http2` feature, the clients that offer HTTP/2 during the handshake (ALPN) get it, and the others stay on HTTP/1.1, on the same port.

HTTPS works with the three ways of starting the server: `app.run(addr)`, `app.listen(addr).await` and `app.bind(addr).await?` (see [Server configuration](server.md#three-ways-to-start-the-server)). The port only speaks TLS: a plain `http://` request to it gets no answer. To redirect HTTP to HTTPS, open port 80 as well ([below](#redirect-http-to-https)).

## Certificates

Vitesse needs a certificate for your domain and its private key. Three constructors:

| Constructor | Use |
|---|---|
| `Tls::from_pem_files(chain, key)` | Two PEM files, e.g. Let's Encrypt's `fullchain.pem` and `privkey.pem` |
| `Tls::from_pem(chain_bytes, key_bytes)` | The same, from memory (a secret manager, an environment variable…) |
| `Tls::from_rustls(config)` | Your own `rustls::ServerConfig`: client certificates, several domains… |

`tls.config()` gives back the rustls configuration in use. The chain contains the server certificate first, then the intermediate certificates: that is exactly what `fullchain.pem` holds. The private key can be in PKCS#8, SEC1 (EC) or PKCS#1 (RSA) format, in PEM. A file that can't be read or parsed makes `from_pem_files` return an `io::Error`, before the server starts. `from_pem` and `from_pem_files` accept TLS 1.2 and 1.3.

### Let's Encrypt

[certbot](https://certbot.eff.org) gets free certificates from Let's Encrypt. Since Vitesse holds port 80 (for the [redirect](#redirect-http-to-https)), use the webroot mode: certbot writes a file in a folder, and Let's Encrypt fetches it over HTTP.

```sh
sudo certbot certonly --webroot -w /var/www/acme -d example.com -d www.example.com
```

The redirect app below serves that folder under `/.well-known/acme-challenge/`. The certificate covers every name given with `-d`, and lands in `/etc/letsencrypt/live/example.com/` (`fullchain.pem` and `privkey.pem`).

> [!IMPORTANT]
> The certificate is loaded once, at startup. Let's Encrypt renews certificates every two months or so: restart the application after each renewal, from a certbot hook (`certbot renew --deploy-hook "systemctl restart my-app"`). Let's Encrypt keys are only readable by root: copy them to a folder the service's user can read, from the same hook.

### Your own rustls configuration

For client certificates (mutual TLS), one certificate per domain, or certificate reloading without restarting, build the rustls configuration yourself. `vitesse::tls::rustls` re-exports the version of rustls that Vitesse uses, so the types always match. Here, only clients with a certificate signed by your own authority get in:

```rust
use std::sync::Arc;

use vitesse::prelude::*;
use vitesse::tls::rustls::pki_types::pem::PemObject;
use vitesse::tls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use vitesse::tls::rustls::server::WebPkiClientVerifier;
use vitesse::tls::{Tls, rustls};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let chain = CertificateDer::pem_file_iter("fullchain.pem")?.collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_file("privkey.pem")?;

    // The authority that signs your clients' certificates.
    let mut client_ca = rustls::RootCertStore::empty();
    for cert in CertificateDer::pem_file_iter("clients-ca.pem")? {
        client_ca.add(cert?)?;
    }

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier =
        WebPkiClientVerifier::builder_with_provider(Arc::new(client_ca), provider.clone()).build()?;
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()? // TLS 1.2 and 1.3
        .with_client_cert_verifier(verifier) // no valid certificate, no connection
        .with_single_cert(chain, key)?;

    let mut app = App::new();
    app.get("/", |_| async { "Hello, trusted client!" });
    app.tls(Tls::from_rustls(config)); // ALPN "h2" and "http/1.1" added if missing
    app.run(443)?;
    Ok(())
}
```

If your configuration lists no ALPN protocol, `Tls::from_rustls` offers `h2` (with the `http2` feature) and `http/1.1`. If you set `alpn_protocols` yourself, include `b"h2"` to keep HTTP/2.

**Several domains.** A single certificate can cover several names (`-d example.com -d example.org` with certbot): that is the simplest option. To use one certificate per domain, let rustls choose it from the name the client asks for (SNI):

```rust
use std::sync::Arc;

use vitesse::prelude::*;
use vitesse::tls::rustls::pki_types::pem::PemObject;
use vitesse::tls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use vitesse::tls::rustls::server::ResolvesServerCertUsingSni;
use vitesse::tls::rustls::sign::CertifiedKey;
use vitesse::tls::{Tls, rustls};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let load = |dir: &str| -> Result<CertifiedKey, Box<dyn std::error::Error>> {
        let chain = CertificateDer::pem_file_iter(format!("{dir}/fullchain.pem"))?
            .collect::<Result<Vec<_>, _>>()?;
        let key = PrivateKeyDer::from_pem_file(format!("{dir}/privkey.pem"))?;
        Ok(CertifiedKey::from_der(chain, key, &provider)?)
    };

    let mut certificates = ResolvesServerCertUsingSni::new();
    certificates.add("example.com", load("/etc/my-app/tls/example.com")?)?;
    certificates.add("example.org", load("/etc/my-app/tls/example.org")?)?;

    let config = rustls::ServerConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(certificates));

    let mut app = App::new();
    app.get("/", |req: Request| async move {
        format!("Hello from {}!", req.hostname().unwrap_or("?"))
    });
    app.tls(Tls::from_rustls(config));
    app.run(443)?;
    Ok(())
}
```

To reload certificates without restarting, write your own resolver: a type that implements `rustls::server::ResolvesServerCert` and rereads the files when they change.

### In development

To try HTTPS on your machine, generate a self-signed certificate for `localhost`:

- in Rust, with the [rcgen](https://crates.io/crates/rcgen) crate, at startup (as a dev-dependency or in an example);
- or with mkcert, which creates a certificate trusted by your own machine: `mkcert -install`, then `mkcert localhost` writes `localhost.pem` and `localhost-key.pem`, to load with `Tls::from_pem_files("localhost.pem", "localhost-key.pem")`.

```rust
// Cargo.toml: rcgen = "0.14"
let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
    .map_err(std::io::Error::other)?;
app.tls(Tls::from_pem(
    cert.cert.pem().as_bytes(),
    cert.signing_key.serialize_pem().as_bytes(),
)?);
```

## Port 443 and redirecting HTTP to HTTPS

Browsers expect HTTPS on port **443**, and send people who type `example.com` to port **80**, in plain HTTP. Serve the application on 443, and run a second, tiny app on port 80 that redirects everything to HTTPS.

### Redirect HTTP to HTTPS

```rust
use vitesse::prelude::*;
use vitesse::tls::Tls;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // The application, over HTTPS.
    let mut app = App::new();
    app.middleware(middleware::helmet()); // Strict-Transport-Security, among others
    app.get("/", |_| async { "Hello over HTTPS!" });
    app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?);

    // A tiny app on port 80 that sends everyone to HTTPS.
    let mut redirect = App::new();
    // Let's Encrypt's challenges (certbot --webroot -w /var/www/acme).
    redirect.static_dir("/.well-known/acme-challenge", "/var/www/acme/.well-known/acme-challenge");
    redirect.fallback(|req: Request| async move {
        let host = req.hostname().unwrap_or("example.com").to_owned();
        let path = req.uri().path_and_query().map_or("/", |pq| pq.as_str());
        Redirect::permanent(format!("https://{host}{path}"))
    });

    // Both stop together on Ctrl+C / SIGTERM; if one can't start, the error is returned.
    tokio::try_join!(app.listen(443), redirect.listen(80))?;
    Ok(())
}
```

This needs tokio in your `Cargo.toml` (`tokio = { version = "1", features = ["full"] }`). `app.listen` runs the app on your runtime; with `app.bind(...).await?`, you get two `Server`s and join `https.run()` and `http.run()` the same way.

> [!TIP]
> `listen` doesn't use `app.run`'s thread-per-core mode. To keep it for the main app, start the redirect on its own thread instead: `std::thread::spawn(move || redirect.run(80));`, then `app.run(443)`.

`helmet()` adds `Strict-Transport-Security`: browsers then go straight to HTTPS for a year, without going through the redirect.

### Ports below 1024

On Linux, only root may open ports below 1024. Don't run the app as root: under systemd, add `AmbientCapabilities=CAP_NET_BIND_SERVICE` to the `[Service]` section (see [Going to production](production.md#run-it-as-a-service-with-systemd)). In a container, the image's user can't open them either: listen on 8443 and 8080, and publish them on 443 and 80 (`docker run -p 443:8443 -p 80:8080`).

## HTTP/2 over TLS: browsers

Browsers only use HTTP/2 over TLS, and choose it during the TLS handshake through ALPN. With the `tls` and `http2` features, there is nothing else to configure: Vitesse offers `h2` and `http/1.1`, and each client picks the best one it knows.

What HTTP/2 changes for your app:

- several requests travel at the same time on one connection (multiplexing): a page loads its styles, scripts and images in parallel, without opening six connections;
- each request is handled in its own task, by the same global middleware, routes, router middleware and `app.on_error` as HTTP/1.1;
- `req.version()` is `HTTP/2.0` (`vitesse::http::Version::HTTP_2`), and the `Host` header is rebuilt from the `:authority` pseudo-header, so `req.header("host")`, `req.host()` and `req.hostname()` work as in HTTP/1.1;
- request bodies are streamed to the handler, as in HTTP/1.1, within `app.body_limit` (or the [route's limit](requests.md#body-size-limit-and-413));
- responses are streamed too ([Server-Sent Events](responses.md), files, `Body::from_stream`); Vitesse adds `date` and `content-length` (when the size is known), and removes the headers specific to HTTP/1.1 (`connection`, `transfer-encoding`, `upgrade`, `keep-alive`).

## HTTP/2 in clear text (h2c)

With the `http2` feature and without TLS, a client that opens the connection directly in HTTP/2 (with the HTTP/2 preface, "prior knowledge") gets HTTP/2, on the same port as HTTP/1.1. Browsers never do this, but some clients and proxies do:

- gRPC clients, and Envoy;
- Caddy with an `h2c://` upstream (`reverse_proxy h2c://127.0.0.1:3000`), Nginx with `grpc_pass`;
- platforms that talk HTTP/2 to the container, such as Cloud Run's end-to-end HTTP/2 option;
- `curl --http2-prior-knowledge`.

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |req: Request| async move { format!("{:?}", req.version()) });
    app.run(3000) // HTTP/1.1 and h2c on the same port (`http2` feature)
}
```

```sh
curl http://localhost:3000/                            # HTTP/1.1
curl --http2-prior-knowledge http://localhost:3000/    # HTTP/2.0
```

The other way to start HTTP/2 in clear text, an HTTP/1.1 request with `Upgrade: h2c`, is not supported: those requests are simply answered in HTTP/1.1, which the protocol allows.

## What the request tells you

| | `req.version()` | `req.protocol()` | `req.secure()` |
|---|---|---|---|
| HTTP/1.1 | `HTTP/1.1` | `"http"` | `false` |
| HTTPS, HTTP/1.1 | `HTTP/1.1` | `"https"` | `true` |
| HTTPS, HTTP/2 | `HTTP/2.0` | `"https"` | `true` |
| HTTP/2 in clear text (h2c) | `HTTP/2.0` | `"http"` | `false` |
| [HTTP/3](http3.md) | `HTTP/3.0` | `"https"` | `true` |

Behind a reverse proxy that handles TLS, the connection between the proxy and Vitesse is plain HTTP: declare the proxy with `app.trust_proxy(...)`, and `req.protocol()` and `req.secure()` follow its `X-Forwarded-Proto` header (see [Going to production](production.md#getting-the-clients-ip-address)).

## WebSocket over TLS

[WebSocket](websocket.md) routes work over HTTPS without any change: clients connect with `wss://` instead of `ws://`.

```js
const socket = new WebSocket("wss://example.com/chat");
```

The WebSocket connection itself uses HTTP/1.1 inside TLS: browsers open a separate HTTP/1.1 connection for it. WebSocket over HTTP/2 (RFC 8441) is not supported: a `101 Switching Protocols` answer on an HTTP/2 connection becomes `505 HTTP Version Not Supported`.

## HTTPS and HTTP/3 together

With the `http3` feature as well, one binary serves every protocol. Responses sent over HTTPS (HTTP/1.1 and HTTP/2) then carry the `alt-svc` header, and browsers switch to [HTTP/3](http3.md) directly, with no proxy in front:

```toml
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tls", "http2", "http3"] }
```

```rust
use vitesse::http3::Http3;
use vitesse::prelude::*;
use vitesse::tls::Tls;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |req: Request| async move {
        format!("Hello over {:?}!", req.version()) // HTTP/1.1, HTTP/2.0 or HTTP/3.0
    });

    // The same certificate for both.
    let (cert, key) = ("/etc/my-app/tls/fullchain.pem", "/etc/my-app/tls/privkey.pem");
    app.tls(Tls::from_pem_files(cert, key)?); // TCP 443: HTTPS, HTTP/2 and HTTP/1.1
    app.http3(Http3::from_pem_files(cert, key)?); // UDP 443: HTTP/3

    app.run(443)
}
```

Open TCP port 443 **and** UDP port 443 in the firewall (`sudo ufw allow 443`, which covers both). See [HTTP/3 and QUIC](http3.md) for the options.

## Limits and what isn't supported

| HTTP/2 | Value |
|---|---|
| Requests in flight on one connection | 256 |
| Size of a request's headers | 64 KiB |
| Body a client may send before the handler reads it (flow control) | 256 KiB per request, 1 MiB per connection |
| Idle connection | Closed after the keep-alive delay ([`app.keep_alive`](server.md#timeouts-and-connections), 60 s by default) |
| Graceful shutdown | The client receives a `GOAWAY`, and requests in progress get the [`shutdown_timeout`](server.md#graceful-shutdown) to finish |
| TLS handshake | 10 seconds at most |

`app.max_connections` counts connections, not requests: with HTTP/2, one connection can carry 256 requests at once. To cap the requests themselves, use `middleware::concurrency_limit` (see [Middleware](middleware.md)).

Not supported:

- **`Upgrade: h2c`**: those requests are answered in HTTP/1.1; use prior knowledge, or TLS;
- **server push**: browsers have dropped it anyway;
- **WebSocket over HTTP/2**: WebSocket uses HTTP/1.1, over TLS or not.

## Try it locally

The repository's `examples/https.rs` generates a self-signed certificate for `localhost` at startup and listens on port 3443:

```sh
cargo run --example https --features tls,http2

curl -k https://localhost:3443/            # Hello over HTTP/2.0 (https), from 127.0.0.1!
curl -k --http1.1 https://localhost:3443/  # Hello over HTTP/1.1 (https), from 127.0.0.1!
```

`-k` accepts the self-signed certificate, and `--http1.1` forbids HTTP/2. With `curl -v`, the handshake shows the chosen protocol (`ALPN: server accepted h2`). In a browser, open `https://localhost:3443/` and accept the warning.

## TLS directly or a reverse proxy?

Both work; choose with your hosting in mind.

**Vitesse serves HTTPS itself** (this page):

- one process less, one network hop less, and nothing else to configure;
- HTTPS, HTTP/2 and HTTP/3 from one binary, with `alt-svc` sent directly to browsers;
- `req.ip()` is the client's real address, without `trust_proxy`;
- but you handle the certificates: renewal with certbot, then a restart (a few hundredths of a second, during which new connections fail), or your own rustls resolver to reload them;
- and the redirect from HTTP, the privileged ports, and rate limiting and compression with Vitesse's middleware (`rate_limit`, `compress`).

**A reverse proxy (Caddy, Nginx) in front**:

- Caddy gets and renews certificates automatically, with no restart of anything;
- one machine can host several applications and domains behind the same ports 80 and 443;
- the proxy can balance the load across several instances and allow zero-downtime deployments;
- but it is one more component, and Vitesse needs [`trust_proxy`](production.md#getting-the-clients-ip-address) to know the client's address.

**On a platform** (Heroku, Cloud Run, Render, Fly.io…), TLS is handled at the platform's edge: leave the `tls` feature off, and listen on the `PORT` it gives you.

The reverse proxy setup is described in [Going to production](production.md#behind-a-reverse-proxy).
