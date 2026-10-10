//! HTTP/3 sur QUIC, avec un vrai client quinn + h3 et un certificat
//! auto-signé pour `localhost`, généré au lancement des tests.
#![cfg(feature = "http3")]

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use bytes::{Buf, Bytes};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vitesse::http3::Http3;
use vitesse::http3::rustls;
use vitesse::http3::rustls::pki_types::CertificateDer;
use vitesse::http3::rustls::pki_types::pem::PemObject;
use vitesse::prelude::*;

/// Certificat et clé (PEM), générés une seule fois.
fn pem() -> &'static (String, String) {
    static PEM: OnceLock<(String, String)> = OnceLock::new();
    PEM.get_or_init(|| {
        let key = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        (key.cert.pem(), key.signing_key.serialize_pem())
    })
}

fn http3() -> Http3 {
    let (cert, key) = pem();
    Http3::from_pem(cert.as_bytes(), key.as_bytes()).unwrap()
}

fn app() -> App {
    let mut app = App::new();
    app.get("/", |_| async { "bonjour en HTTP/3" });
    app.get("/hello/:name", |req: Request| async move {
        Json(json!({
            "hello": req.param("name"),
            "host": req.header("host"),
        }))
    });
    app.post("/echo", |req: Request| async move {
        let body = req.bytes().await?;
        Ok::<_, Error>(body)
    });
    app.get("/boom", |_| async {
        if true {
            panic!("boum en HTTP/3");
        }
        "jamais"
    });
    app.body_limit(1024);
    app.http3(http3());
    app
}

struct Client {
    send: h3::client::SendRequest<h3_quinn::OpenStreams, Bytes>,
    _endpoint: quinn::Endpoint,
}

async fn client(addr: SocketAddr) -> Client {
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from_pem_slice(pem().0.as_bytes()).unwrap())
        .unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls.alpn_protocols = vec![b"h3".to_vec()];
    let config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap(),
    ));
    let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    endpoint.set_default_client_config(config);
    let conn = endpoint.connect(addr, "localhost").unwrap().await.unwrap();
    let (mut driver, send) = h3::client::new(h3_quinn::Connection::new(conn))
        .await
        .unwrap();
    tokio::spawn(async move {
        let _ = std::future::poll_fn(|cx| driver.poll_close(cx)).await;
    });
    Client {
        send,
        _endpoint: endpoint,
    }
}

impl Client {
    async fn request(
        &mut self,
        method: Method,
        path: &str,
        body: &[u8],
    ) -> (http::Response<()>, Vec<u8>) {
        let req = http::Request::builder()
            .method(method)
            .uri(format!("https://localhost{path}"))
            .body(())
            .unwrap();
        let mut stream = self.send.send_request(req).await.unwrap();
        if !body.is_empty() {
            stream
                .send_data(Bytes::copy_from_slice(body))
                .await
                .unwrap();
        }
        stream.finish().await.unwrap();
        let res = stream.recv_response().await.unwrap();
        let mut out = Vec::new();
        while let Some(mut chunk) = stream.recv_data().await.unwrap() {
            while chunk.has_remaining() {
                let part = chunk.chunk();
                out.extend_from_slice(part);
                let n = part.len();
                chunk.advance(n);
            }
        }
        (res, out)
    }
}

#[tokio::test]
async fn serves_the_same_routes_over_http3_and_advertises_it() {
    let server = app().bind("127.0.0.1:0").await.unwrap();
    let tcp = server.local_addr();
    let udp = server.http3_addr().expect("HTTP/3 activé");
    assert_eq!(
        tcp.port(),
        udp.port(),
        "même numéro de port en TCP et en UDP"
    );
    tokio::spawn(server.run());

    let mut c = client(udp).await;
    let (res, body) = c.request(Method::GET, "/", b"").await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.version(), http::Version::HTTP_3);
    assert_eq!(body, "bonjour en HTTP/3".as_bytes());
    assert!(res.headers().contains_key("date"));
    assert_eq!(res.headers()["content-length"], "17");

    // Paramètres de route, et `Host` reconstruit depuis `:authority`.
    let (res, body) = c.request(Method::GET, "/hello/ada", b"").await;
    assert_eq!(res.status(), 200);
    assert_eq!(
        String::from_utf8(body).unwrap(),
        r#"{"hello":"ada","host":"localhost"}"#
    );

    // Corps de requête, et limite de taille.
    let (res, body) = c.request(Method::POST, "/echo", b"ping").await;
    assert_eq!(
        (res.status().as_u16(), body.as_slice()),
        (200, b"ping".as_slice())
    );
    let (res, _) = c.request(Method::POST, "/echo", &[b'x'; 4096]).await;
    assert_eq!(res.status(), 413);

    // 404, HEAD et panique : comme en HTTP/1.1.
    let (res, _) = c.request(Method::GET, "/nope", b"").await;
    assert_eq!(res.status(), 404);
    let (res, body) = c.request(Method::HEAD, "/", b"").await;
    assert_eq!(res.status(), 200);
    assert!(body.is_empty());
    let (res, _) = c.request(Method::GET, "/boom", b"").await;
    assert_eq!(res.status(), 500);

    // Les réponses HTTP/1.1 annoncent HTTP/3 aux navigateurs.
    let mut stream = tokio::net::TcpStream::connect(tcp).await.unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut out = String::new();
    stream.read_to_string(&mut out).await.unwrap();
    assert!(
        out.contains(&format!("alt-svc: h3=\":{}\"; ma=86400\r\n", udp.port())),
        "{out}"
    );
}

#[tokio::test]
async fn alt_svc_port_and_graceful_shutdown() {
    let mut app = App::new();
    app.get("/slow", |_| async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        "fini"
    });
    app.http3(http3().alt_svc_port(443));
    let server = app.bind("127.0.0.1:0").await.unwrap();
    let (tcp, udp) = (server.local_addr(), server.http3_addr().unwrap());
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(server.with_graceful_shutdown(async {
        let _ = stopped.await;
    }));

    let mut stream = tokio::net::TcpStream::connect(tcp).await.unwrap();
    stream
        .write_all(b"GET /slow HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut out = String::new();
    stream.read_to_string(&mut out).await.unwrap();
    assert!(out.contains("alt-svc: h3=\":443\"; ma=86400\r\n"), "{out}");

    // Une requête HTTP/3 en cours au moment de l'arrêt se termine.
    let mut c = client(udp).await;
    let in_flight = tokio::spawn(async move { c.request(Method::GET, "/slow", b"").await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let _ = stop.send(());
    let (res, body) = in_flight.await.unwrap();
    assert_eq!(
        (res.status().as_u16(), body.as_slice()),
        (200, b"fini".as_slice())
    );
    let stopped = tokio::time::timeout(Duration::from_secs(15), running).await;
    assert!(matches!(stopped, Ok(Ok(Ok(())))), "{stopped:?}");
}

#[test]
fn rejects_invalid_pem() {
    let (cert, key) = pem();
    assert!(Http3::from_pem(b"pas un certificat", key.as_bytes()).is_err());
    assert!(Http3::from_pem(cert.as_bytes(), b"pas une cle").is_err());
}
