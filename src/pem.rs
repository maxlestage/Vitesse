//! Certificates and private keys in PEM, shared by HTTPS ([`tls`](crate::tls))
//! and HTTP/3.

use std::io;
use std::sync::Arc;

use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

pub(crate) fn invalid(e: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, e.to_string())
}

/// The cryptography of rustls: ring, all in Rust.
pub(crate) fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Reads a certificate chain (the server certificate first, then the
/// intermediates) and its private key.
pub(crate) fn load(
    cert_chain: &[u8],
    private_key: &[u8],
) -> io::Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let certs = CertificateDer::pem_slice_iter(cert_chain)
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if certs.is_empty() {
        return Err(invalid("no certificate found in the PEM data"));
    }
    let key = PrivateKeyDer::from_pem_slice(private_key).map_err(invalid)?;
    Ok((certs, key))
}
