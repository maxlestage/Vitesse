//! WebSocket, sur un vrai port TCP avec le client tokio-tungstenite.
#![cfg(feature = "ws")]

use std::net::SocketAddr;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as Client;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use vitesse::prelude::*;
use vitesse::test::TestClient;
use vitesse::ws::{Message, Upgrade};

async fn start(app: App) -> SocketAddr {
    let server = app.bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());
    addr
}

fn echo_app() -> App {
    let mut app = App::new();
    app.ws("/echo/:name", |req, mut socket| async move {
        let name = req.param("name").unwrap_or("?").to_owned();
        socket.send(format!("bonjour {name}")).await.unwrap();
        while let Some(Ok(message)) = socket.recv().await {
            match message {
                Message::Text(text) => socket.send(format!("écho : {text}")).await.unwrap(),
                Message::Binary(data) => {
                    let mut reversed = data.to_vec();
                    reversed.reverse();
                    socket.send(reversed).await.unwrap();
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });
    app.get("/", |_| async { "http" });
    app
}

#[tokio::test]
async fn echo_text_and_binary_with_route_params() {
    let addr = start(echo_app()).await;
    let (mut ws, response) = connect_async(format!("ws://{addr}/echo/ada"))
        .await
        .unwrap();
    assert_eq!(response.status(), 101);

    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Client::text("bonjour ada")
    );
    ws.send(Client::text("salut")).await.unwrap();
    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Client::text("écho : salut")
    );
    ws.send(Client::binary(vec![1u8, 2, 3])).await.unwrap();
    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Client::binary(vec![3u8, 2, 1])
    );
    // Les pings reçoivent un pong automatique.
    ws.send(Client::Ping(b"p".to_vec().into())).await.unwrap();
    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Client::Pong(b"p".to_vec().into())
    );
    ws.close(None).await.unwrap();
    // Le serveur répond au close, puis la connexion se termine.
    while let Some(Ok(_)) = ws.next().await {}

    // Le reste de l'application répond toujours en HTTP.
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream
        .write_all(b"GET / HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut out = String::new();
    stream.read_to_string(&mut out).await.unwrap();
    assert!(out.ends_with("http"), "{out}");
}

#[tokio::test]
async fn plain_http_on_a_websocket_route_gets_426() {
    let c = TestClient::new(echo_app());
    let r = c.get("/echo/ada").await;
    assert_eq!(r.status(), 426);
    // Sans `Sec-WebSocket-Version: 13`.
    let r = c
        .get("/echo/ada")
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .await;
    assert_eq!(r.status(), 426);
    // Clé invalide.
    let r = c
        .get("/echo/ada")
        .header("connection", "keep-alive, Upgrade")
        .header("upgrade", "WebSocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "court")
        .await;
    assert_eq!(r.status(), 400);
    // Une poignée de main valide répond 101 avec la bonne clé (RFC 6455, 1.3).
    let r = c
        .get("/echo/ada")
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .await;
    assert_eq!(r.status(), 101);
    assert_eq!(
        r.header("sec-websocket-accept"),
        Some("s3pPLMBiTxaQ9kYGzzhZRbK+xOo=")
    );
}

#[tokio::test]
async fn subprotocols_size_limit_and_split() {
    let mut app = App::new();
    app.get("/chat", |req: Request| async move {
        let upgrade = Upgrade::new(&req)?
            .protocols(["chat.v2", "chat.v1"])
            .max_message_size(1024);
        Ok::<_, Error>(upgrade.on_upgrade(req, |_req, socket| async move {
            let protocol = socket.protocol().unwrap_or("aucun").to_owned();
            let (mut tx, mut rx) = socket.split();
            // Une tâche envoie pendant que l'autre reçoit.
            let ticker = tokio::spawn(async move {
                tx.send(format!("protocole {protocol}")).await.unwrap();
                for i in 0..3 {
                    tx.send(format!("tick {i}")).await.unwrap();
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                tx
            });
            let mut received = 0;
            while let Some(Ok(message)) = rx.recv().await {
                if message.is_close() {
                    break;
                }
                received += message.as_bytes().len();
            }
            let mut tx = ticker.await.unwrap();
            let _ = tx.send(format!("reçu {received}")).await;
        }))
    });
    let addr = start(app).await;

    let mut request = format!("ws://{addr}/chat").into_client_request().unwrap();
    request.headers_mut().insert(
        "sec-websocket-protocol",
        "chat.v1, chat.v3".parse().unwrap(),
    );
    let (mut ws, response) = connect_async(request).await.unwrap();
    assert_eq!(
        response.headers().get("sec-websocket-protocol").unwrap(),
        "chat.v1"
    );
    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Client::text("protocole chat.v1")
    );
    for i in 0..3 {
        assert_eq!(
            ws.next().await.unwrap().unwrap(),
            Client::text(format!("tick {i}"))
        );
    }
    // Au-delà de 1 Kio, le serveur coupe la connexion.
    ws.send(Client::binary(vec![0u8; 4096])).await.unwrap();
    let mut closed = false;
    while let Some(message) = ws.next().await {
        match message {
            Ok(Client::Close(_)) | Err(_) => {
                closed = true;
                break;
            }
            Ok(_) => {}
        }
    }
    assert!(closed);
}
