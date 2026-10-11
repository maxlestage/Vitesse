//! HTTPS : TLS sur TCP avec rustls, un certificat auto-signé pour
//! `localhost` généré au lancement des tests, et de vrais clients (rustls,
//! h2, tungstenite).
#![cfg(feature = "tls")]

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use vitesse::prelude::*;
use vitesse::tls::Tls;
use vitesse::tls::rustls;
use vitesse::tls::rustls::pki_types::pem::PemObject;
use vitesse::tls::rustls::pki_types::{CertificateDer, ServerName};

/// Certificat et clé (PEM), générés une seule fois.
fn pem() -> &'static (String, String) {
    static PEM: OnceLock<(String, String)> = OnceLock::new();
    PEM.get_or_init(|| {
        let key = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        (key.cert.pem(), key.signing_key.serialize_pem())
    })
}

fn app() -> App {
    let (cert, key) = pem();
    let mut app = App::new();
    app.get("/", |req: Request| async move {
        format!("{:?} {} {}", req.version(), req.protocol(), req.secure())
    });
    app.post("/echo", |req: Request| async move {
        let body = req.bytes().await?;
        Ok::<_, Error>(body)
    });
    #[cfg(feature = "ws")]
    app.ws("/ws", |_req, mut socket| async move {
        while let Some(Ok(message)) = socket.recv().await {
            if socket.send(message).await.is_err() {
                break;
            }
        }
    });
    app.tls(Tls::from_pem(cert.as_bytes(), key.as_bytes()).unwrap());
    app
}

async fn server() -> SocketAddr {
    let server = app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());
    addr
}

async fn connect(addr: SocketAddr, alpn: &[&[u8]]) -> TlsStream<TcpStream> {
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from_pem_slice(pem().0.as_bytes()).unwrap())
        .unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = alpn.iter().map(|p| p.to_vec()).collect();
    let tcp = TcpStream::connect(addr).await.unwrap();
    TlsConnector::from(Arc::new(config))
        .connect(ServerName::try_from("localhost").unwrap(), tcp)
        .await
        .unwrap()
}

#[tokio::test]
async fn https_over_http1() {
    let addr = server().await;
    for alpn in [&[][..], &[&b"http/1.1"[..]][..]] {
        let mut tls = connect(addr, alpn).await;
        tls.write_all(b"GET / HTTP/1.1\r\nhost: localhost\r\n\r\n")
            .await
            .unwrap();
        tls.write_all(
            b"POST /echo HTTP/1.1\r\nhost: localhost\r\ncontent-length: 5\r\nconnection: close\r\n\r\nhello",
        )
        .await
        .unwrap();
        let mut out = String::new();
        let _ = tls.read_to_string(&mut out).await;
        assert!(out.starts_with("HTTP/1.1 200 OK\r\n"), "{out}");
        assert!(out.contains("\r\n\r\nHTTP/1.1 https true"), "{out}");
        assert!(out.ends_with("\r\n\r\nhello"), "{out}");
    }
}

#[tokio::test]
async fn plain_http_on_the_tls_port_is_refused_without_harm() {
    let addr = server().await;
    let mut tcp = TcpStream::connect(addr).await.unwrap();
    tcp.write_all(b"GET / HTTP/1.1\r\nhost: localhost\r\n\r\n")
        .await
        .unwrap();
    let mut out = Vec::new();
    let _ = tcp.read_to_end(&mut out).await;
    assert!(!out.starts_with(b"HTTP/1.1 200"));
    // Le serveur répond toujours en HTTPS.
    let mut tls = connect(addr, &[]).await;
    tls.write_all(b"GET / HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut out = String::new();
    let _ = tls.read_to_string(&mut out).await;
    assert!(out.starts_with("HTTP/1.1 200 OK\r\n"), "{out}");
}

#[cfg(feature = "http2")]
#[tokio::test]
async fn browsers_get_http2_through_alpn() {
    let addr = server().await;
    let tls = connect(addr, &[b"h2", b"http/1.1"]).await;
    assert_eq!(tls.get_ref().1.alpn_protocol(), Some(&b"h2"[..]));
    let (client, conn) = h2::client::handshake(tls).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let mut client = client.ready().await.unwrap();
    let req = http::Request::get("https://localhost/").body(()).unwrap();
    let (res, _) = client.send_request(req, true).unwrap();
    let (parts, mut body) = res.await.unwrap().into_parts();
    assert_eq!(parts.status, 200);
    let mut out = Vec::new();
    while let Some(chunk) = body.data().await {
        out.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(String::from_utf8(out).unwrap(), "HTTP/2.0 https true");
}

#[cfg(feature = "ws")]
#[tokio::test]
async fn websocket_over_tls() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let addr = server().await;
    let tls = connect(addr, &[b"http/1.1"]).await;
    let (mut ws, res) = tokio_tungstenite::client_async("wss://localhost/ws", tls)
        .await
        .unwrap();
    assert_eq!(res.status(), 101);
    ws.send(Message::text("salut")).await.unwrap();
    let echo = ws.next().await.unwrap().unwrap();
    assert_eq!(echo.into_text().unwrap().as_str(), "salut");
}
