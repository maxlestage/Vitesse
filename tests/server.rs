//! Les réglages du serveur, sur un vrai socket : keep-alive, délai pour
//! recevoir la tête, nombre de connexions et délai d'arrêt.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use vitesse::prelude::*;

async fn start(app: App) -> SocketAddr {
    let server = app.bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());
    addr
}

fn app() -> App {
    let mut app = App::new();
    app.get("/", |_| async { "ok" });
    app.get("/slow", |_| async {
        tokio::time::sleep(Duration::from_secs(3)).await;
        "slow"
    });
    app
}

/// Reads until the server closes the connection.
async fn read_all(tcp: &mut TcpStream) -> String {
    let mut out = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), tcp.read_to_end(&mut out)).await;
    String::from_utf8_lossy(&out).into_owned()
}

#[tokio::test]
async fn keep_alive_can_be_disabled() {
    let mut app = app();
    app.keep_alive(Duration::ZERO);
    let addr = start(app).await;
    let mut tcp = TcpStream::connect(addr).await.unwrap();
    tcp.write_all(b"GET / HTTP/1.1\r\nhost: x\r\n\r\n")
        .await
        .unwrap();
    let out = read_all(&mut tcp).await;
    assert!(out.starts_with("HTTP/1.1 200 OK\r\n"), "{out}");
    assert!(out.contains("connection: close\r\n"), "{out}");
}

#[tokio::test]
async fn idle_connections_are_closed_after_keep_alive() {
    let mut app = app();
    app.keep_alive(Duration::from_millis(300));
    let addr = start(app).await;
    let mut tcp = TcpStream::connect(addr).await.unwrap();
    tcp.write_all(b"GET / HTTP/1.1\r\nhost: x\r\n\r\n")
        .await
        .unwrap();
    let start = Instant::now();
    let out = read_all(&mut tcp).await;
    assert!(out.starts_with("HTTP/1.1 200 OK\r\n"), "{out}");
    assert!(!out.contains("connection: close"), "{out}");
    let idle = start.elapsed();
    assert!(idle >= Duration::from_millis(150), "{idle:?}");
    assert!(idle < Duration::from_secs(2), "{idle:?}");
}

#[tokio::test]
async fn incomplete_heads_time_out() {
    let mut app = app();
    app.header_timeout(Duration::from_millis(200));
    let addr = start(app).await;
    let mut tcp = TcpStream::connect(addr).await.unwrap();
    tcp.write_all(b"GET / HTTP/1.1\r\nhost: x\r\n")
        .await
        .unwrap();
    let start = Instant::now();
    let out = read_all(&mut tcp).await;
    assert!(out.starts_with("HTTP/1.1 408 Request Timeout\r\n"), "{out}");
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn max_connections_queues_new_ones() {
    let mut app = app();
    app.max_connections(1);
    let addr = start(app).await;
    let mut first = TcpStream::connect(addr).await.unwrap();
    first
        .write_all(b"GET / HTTP/1.1\r\nhost: x\r\n\r\n")
        .await
        .unwrap();
    let mut buf = [0u8; 256];
    let n = first.read(&mut buf).await.unwrap();
    assert!(buf[..n].starts_with(b"HTTP/1.1 200 OK"));

    // La deuxième connexion attend que la première se ferme.
    let mut second = TcpStream::connect(addr).await.unwrap();
    second
        .write_all(b"GET / HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
        .await
        .unwrap();
    let waited = tokio::time::timeout(Duration::from_millis(300), second.read(&mut buf)).await;
    assert!(waited.is_err(), "the second connection must wait");
    drop(first);
    let out = read_all(&mut second).await;
    assert!(out.starts_with("HTTP/1.1 200 OK\r\n"), "{out}");
}

#[tokio::test]
async fn shutdown_timeout_bounds_the_wait() {
    let mut app = app();
    app.shutdown_timeout(Duration::from_millis(200));
    let server = app.bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(server.with_graceful_shutdown(async {
        let _ = stopped.await;
    }));
    let mut tcp = TcpStream::connect(addr).await.unwrap();
    tcp.write_all(b"GET /slow HTTP/1.1\r\nhost: x\r\n\r\n")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let start = Instant::now();
    stop.send(()).unwrap();
    running.await.unwrap().unwrap();
    // Le handler de 3 s n'a pas été attendu.
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
}

/// Each core serves its plain connections with an engine of its own, without a
/// task per connection; a request it cannot answer at once (a handler that
/// waits, a body still arriving, a streamed response) hands the connection
/// over to a task. Pipelined on one connection, the responses keep their
/// order through these changes of hands, and the connection stays usable.
#[tokio::test]
async fn the_engine_hands_connections_over_in_order() {
    let mut app = App::new();
    app.get("/", |_| async { "fast" });
    app.get("/wait", |_| async {
        tokio::time::sleep(Duration::from_millis(30)).await;
        "waited"
    });
    app.post("/echo", |req: Request| async move {
        let body = req.text().await?;
        Ok::<_, Error>(format!("echo {body}"))
    });
    app.get("/big", |_| async { "x".repeat(200_000) });
    let addr = start(app).await;
    let mut tcp = TcpStream::connect(addr).await.unwrap();

    tcp.write_all(b"GET / HTTP/1.1\r\nhost: x\r\n\r\nGET /wait HTTP/1.1\r\nhost: x\r\n\r\nGET / HTTP/1.1\r\nhost: x\r\n\r\n")
        .await
        .unwrap();
    // Un corps qui arrive en deux fois.
    tcp.write_all(b"POST /echo HTTP/1.1\r\nhost: x\r\ncontent-length: 10\r\n\r\nhello")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    tcp.write_all(b"world").await.unwrap();
    tcp.write_all(b"GET /big HTTP/1.1\r\nhost: x\r\n\r\nGET / HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
        .await
        .unwrap();
    let out = read_all(&mut tcp).await;
    let bodies: Vec<&str> = out
        .split("HTTP/1.1 200 OK\r\n")
        .skip(1)
        .map(|r| r.split_once("\r\n\r\n").map_or("", |(_, body)| body))
        .collect();
    assert_eq!(bodies.len(), 6, "{out}");
    assert_eq!(&bodies[..4], ["fast", "waited", "fast", "echo helloworld"]);
    assert_eq!(bodies[4].len(), 200_000);
    assert_eq!(bodies[5], "fast");

    // Bien des connexions à la fois, chacune avec plusieurs requêtes.
    let mut clients = Vec::new();
    for i in 0..50 {
        clients.push(tokio::spawn(async move {
            let mut tcp = TcpStream::connect(addr).await.unwrap();
            let path = if i % 5 == 0 { "/wait" } else { "/" };
            let req = format!("GET {path} HTTP/1.1\r\nhost: x\r\n\r\nGET / HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n");
            tcp.write_all(req.as_bytes()).await.unwrap();
            let out = read_all(&mut tcp).await;
            assert_eq!(out.matches("HTTP/1.1 200 OK").count(), 2, "{out}");
        }));
    }
    for client in clients {
        client.await.unwrap();
    }
}
