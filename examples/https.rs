//! HTTPS with HTTP/2, with a self-signed certificate for `localhost`
//! generated at startup (in production, use a real one:
//! `Tls::from_pem_files("fullchain.pem", "privkey.pem")` with Let's Encrypt).
//!
//! ```sh
//! cargo run --release --example https --features tls,http2
//!
//! curl -k https://localhost:3443/            # HTTP/2 over TLS (ALPN)
//! curl -k --http1.1 https://localhost:3443/  # HTTP/1.1 over TLS
//! ```

use vitesse::prelude::*;
use vitesse::tls::Tls;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());
    app.middleware(middleware::helmet());
    app.get("/", |req: Request| async move {
        format!(
            "Hello over {:?} ({}), from {}!\n",
            req.version(),
            req.protocol(),
            req.ip().map_or("?".into(), |ip| ip.to_string()),
        )
    });

    let self_signed = rcgen::generate_simple_self_signed(vec!["localhost".into()])
        .map_err(std::io::Error::other)?;
    app.tls(Tls::from_pem(
        self_signed.cert.pem().as_bytes(),
        self_signed.signing_key.serialize_pem().as_bytes(),
    )?);

    println!("⚡ HTTPS (HTTP/2 and HTTP/1.1) on https://localhost:3443");
    app.run(3443)
}
