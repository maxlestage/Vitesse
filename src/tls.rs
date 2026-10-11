//! HTTPS: TLS over TCP, with [rustls](https://crates.io/crates/rustls) (all
//! in Rust, no OpenSSL to install). Needs the `tls` feature.
//!
//! ```no_run
//! use vitesse::prelude::*;
//! use vitesse::tls::Tls;
//!
//! fn main() -> std::io::Result<()> {
//!     let mut app = App::new();
//!     app.get("/", |req: Request| async move {
//!         format!("Hello over {}!", req.protocol())
//!     });
//!
//!     // A certificate and its private key, in PEM (Let's Encrypt:
//!     // fullchain.pem and privkey.pem).
//!     app.tls(Tls::from_pem_files("cert.pem", "key.pem")?);
//!
//!     app.run(443)
//! }
//! ```
//!
//! Every connection on the port then speaks TLS 1.2 or 1.3. With the `http2`
//! feature, clients that offer HTTP/2 (every browser) get it, through ALPN;
//! the others stay on HTTP/1.1. WebSocket works too (`wss://`).

use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio_rustls::TlsAcceptor;

pub use rustls;

use crate::app::AppService;
use crate::http1::{ServerState, serve_connection};
use crate::pem::invalid;
use crate::upgrade::Io;

/// Time given to a client to complete the TLS handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// The TLS configuration of an application (see [`App::tls`](crate::App::tls)).
#[derive(Clone)]
pub struct Tls {
    config: Arc<rustls::ServerConfig>,
}

impl std::fmt::Debug for Tls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let alpn: Vec<String> = self
            .config
            .alpn_protocols
            .iter()
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect();
        f.debug_struct("Tls")
            .field("alpn", &alpn)
            .finish_non_exhaustive()
    }
}

impl Tls {
    /// From a certificate chain and a private key in PEM (the server
    /// certificate first, then the intermediates).
    pub fn from_pem(cert_chain: &[u8], private_key: &[u8]) -> io::Result<Self> {
        let (certs, key) = crate::pem::load(cert_chain, private_key)?;
        let config = rustls::ServerConfig::builder_with_provider(crate::pem::provider())
            .with_safe_default_protocol_versions()
            .map_err(invalid)?
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(invalid)?;
        Ok(Self::from_rustls(config))
    }

    /// From PEM files (`fullchain.pem` and `privkey.pem` with Let's Encrypt).
    pub fn from_pem_files(
        cert_chain: impl AsRef<Path>,
        private_key: impl AsRef<Path>,
    ) -> io::Result<Self> {
        Self::from_pem(&std::fs::read(cert_chain)?, &std::fs::read(private_key)?)
    }

    /// From a rustls configuration of your own: client certificates (mutual
    /// TLS), several domains, certificate reloading… `rustls` is re-exported
    /// as [`vitesse::tls::rustls`](rustls).
    ///
    /// If the configuration lists no ALPN protocol, Vitesse offers `h2`
    /// (with the `http2` feature) and `http/1.1`.
    pub fn from_rustls(mut config: rustls::ServerConfig) -> Self {
        if config.alpn_protocols.is_empty() {
            #[cfg(feature = "http2")]
            config.alpn_protocols.push(b"h2".to_vec());
            config.alpn_protocols.push(b"http/1.1".to_vec());
        }
        Tls {
            config: Arc::new(config),
        }
    }

    /// The rustls configuration.
    pub fn config(&self) -> &Arc<rustls::ServerConfig> {
        &self.config
    }

    pub(crate) fn acceptor(&self) -> TlsAcceptor {
        TlsAcceptor::from(self.config.clone())
    }
}

/// Performs the TLS handshake, then serves the connection in HTTP/2 if the
/// client chose it (ALPN), in HTTP/1.1 otherwise.
pub(crate) async fn serve<IO: Io>(
    acceptor: TlsAcceptor,
    stream: IO,
    peer: Option<SocketAddr>,
    app: &'static AppService,
    state: &'static ServerState,
) {
    let Ok(Ok(tls)) = tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await else {
        return;
    };
    #[cfg(feature = "http2")]
    if tls.get_ref().1.alpn_protocol() == Some(b"h2") {
        return crate::http2::serve(tls, peer, true, app, state).await;
    }
    serve_connection(tls, peer, true, app, state).await
}
