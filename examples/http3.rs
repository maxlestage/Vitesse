//! HTTP/1.1 and HTTP/3 (QUIC) on the same port, with a self-signed
//! certificate for `localhost` generated at startup (in production, use a
//! real one: `Http3::from_pem_files("fullchain.pem", "privkey.pem")` with
//! Let's Encrypt).
//!
//! ```sh
//! cargo run --release --example http3 --features http3
//!
//! curl -k https://localhost:4433/ --http3-only   # HTTP/3 over UDP
//! curl -i http://localhost:4433/                 # HTTP/1.1, with Alt-Svc
//! ```

use vitesse::http3::Http3;
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());
    app.get("/", |req: Request| async move {
        format!("Hello over {:?}!\n", req.version())
    });

    let self_signed = rcgen::generate_simple_self_signed(vec!["localhost".into()])
        .map_err(std::io::Error::other)?;
    app.http3(Http3::from_pem(
        self_signed.cert.pem().as_bytes(),
        self_signed.signing_key.serialize_pem().as_bytes(),
    )?);

    println!("⚡ HTTP/1.1 on tcp://localhost:4433, HTTP/3 on udp://localhost:4433");
    app.run(4433)
}
