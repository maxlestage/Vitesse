//! Server-Sent Events, sur un vrai serveur : les événements arrivent au fil
//! de l'eau, et l'émetteur apprend que le navigateur est parti.

use std::convert::Infallible;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use vitesse::prelude::*;
use vitesse::sse::{Event, KeepAlive, Sse};
use vitesse::test::TestClient;

#[tokio::test]
async fn stream_of_events() {
    let mut app = App::new();
    app.get("/count", |_| async {
        let events = (1..=3)
            .map(|n| Ok::<_, Infallible>(Event::new().id(n.to_string()).data(format!("n={n}"))));
        Sse::new(futures_util::stream::iter(events))
    });
    let r = TestClient::new(app).get("/count").await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.header("content-type"), Some("text/event-stream"));
    assert_eq!(r.header("cache-control"), Some("no-cache"));
    assert_eq!(
        r.text(),
        "id: 1\ndata: n=1\n\nid: 2\ndata: n=2\n\nid: 3\ndata: n=3\n\n"
    );
}

#[tokio::test]
async fn channel_over_tcp_with_keep_alive() {
    let (gone_tx, gone_rx) = tokio::sync::oneshot::channel::<()>();
    let gone_tx = std::sync::Mutex::new(Some(gone_tx));
    let mut app = App::new();
    app.state(gone_tx);
    app.get("/events", |req: Request| async move {
        let gone = req
            .state::<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>>()
            .lock()
            .unwrap()
            .take();
        let (tx, sse) = sse::channel(4);
        tokio::spawn(async move {
            tx.send(Event::new().event("hello").data("first"))
                .await
                .unwrap();
            // Pas d'événement pendant 150 ms : un commentaire de maintien part.
            tokio::time::sleep(Duration::from_millis(150)).await;
            tx.send(Event::new().json_data(&json!({ "n": 2 })).unwrap())
                .await
                .unwrap();
            // Le navigateur ferme : l'émetteur le voit.
            tx.closed().await;
            if let Some(gone) = gone {
                let _ = gone.send(());
            }
        });
        sse.keep_alive(
            KeepAlive::new()
                .interval(Duration::from_millis(100))
                .text("ping"),
        )
    });
    let server = app.bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());

    let mut tcp = TcpStream::connect(addr).await.unwrap();
    tcp.write_all(b"GET /events HTTP/1.1\r\nhost: x\r\n\r\n")
        .await
        .unwrap();
    let mut received = String::new();
    let mut buf = [0u8; 1024];
    while !received.contains("data: {\"n\":2}\n\n") {
        let n = tokio::time::timeout(Duration::from_secs(5), tcp.read(&mut buf))
            .await
            .expect("events arrive as they are sent")
            .unwrap();
        assert!(n > 0, "{received}");
        received.push_str(std::str::from_utf8(&buf[..n]).unwrap());
    }
    assert!(received.starts_with("HTTP/1.1 200 OK\r\n"), "{received}");
    assert!(
        received.contains("transfer-encoding: chunked\r\n"),
        "{received}"
    );
    assert!(
        received.contains("event: hello\ndata: first\n\n"),
        "{received}"
    );
    assert!(received.contains(": ping\n\n"), "{received}");
    drop(tcp);
    tokio::time::timeout(Duration::from_secs(5), gone_rx)
        .await
        .expect("the sender learns that the client is gone")
        .unwrap();
}
