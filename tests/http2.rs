//! HTTP/2 en clair (h2c, « prior knowledge »), avec un vrai client h2 : les
//! mêmes routes qu'en HTTP/1.1, sur le même port.
#![cfg(feature = "http2")]

use std::net::SocketAddr;
use std::time::Duration;

use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use vitesse::prelude::*;

fn app() -> App {
    let mut app = App::new();
    app.get("/", |req: Request| async move {
        format!("{:?} {}", req.version(), req.header("host").unwrap_or("-"))
    });
    app.post("/echo", |req: Request| async move {
        let body = req.bytes().await?;
        Ok::<_, Error>(body)
    });
    app.get("/slow", |_| async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        "lent"
    });
    app.get("/stream", |_| async {
        let chunks = (0..3).map(|i| Ok::<_, std::io::Error>(format!("morceau {i}\n")));
        Body::from_stream(futures_util::stream::iter(chunks))
    });
    app.get("/big", |_| async { "x".repeat(300_000) });
    app.get("/cookies", |req: Request| async move {
        format!("{:?} {:?}", req.cookie("a"), req.cookie("b"))
    });
    app.body_limit(64 * 1024);
    app
}

async fn server(app: App) -> SocketAddr {
    let server = app.bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());
    addr
}

type Client = h2::client::SendRequest<Bytes>;

async fn client(addr: SocketAddr) -> Client {
    let tcp = TcpStream::connect(addr).await.unwrap();
    let (client, conn) = h2::client::handshake(tcp).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
}

async fn request(
    client: &mut Client,
    method: Method,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> (http::response::Parts, Vec<u8>) {
    let mut req = http::Request::builder()
        .method(method)
        .uri(format!("http://localhost{path}"));
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let req = req.body(()).unwrap();
    let ready = client.clone().ready().await.unwrap();
    *client = ready;
    let (res, mut send) = client.send_request(req, body.is_empty()).unwrap();
    if !body.is_empty() {
        send.send_data(Bytes::copy_from_slice(body), true).unwrap();
    }
    let (parts, mut body) = res.await.unwrap().into_parts();
    let mut out = Vec::new();
    while let Some(chunk) = body.data().await {
        let chunk = chunk.unwrap();
        body.flow_control().release_capacity(chunk.len()).unwrap();
        out.extend_from_slice(&chunk);
    }
    (parts, out)
}

#[tokio::test]
async fn h2c_prior_knowledge_serves_the_same_routes() {
    let addr = server(app()).await;
    let mut c = client(addr).await;

    let (res, body) = request(&mut c, Method::GET, "/", &[], b"").await;
    assert_eq!(res.status, 200);
    assert_eq!(res.version, http::Version::HTTP_2);
    // `:authority` devient l'en-tête `Host`.
    assert_eq!(String::from_utf8(body).unwrap(), "HTTP/2.0 localhost");
    assert!(res.headers.contains_key("date"));
    assert_eq!(res.headers["content-type"], "text/plain; charset=utf-8");

    let (res, body) = request(&mut c, Method::POST, "/echo", &[], b"ping h2").await;
    assert_eq!(res.status, 200);
    assert_eq!(body, b"ping h2");

    // Un corps plus grand que la fenêtre de contrôle de flux initiale.
    let big = vec![b'y'; 60 * 1024];
    let (res, body) = request(&mut c, Method::POST, "/echo", &[], &big).await;
    assert_eq!(res.status, 200);
    assert_eq!(body.len(), big.len());

    let (res, _) = request(&mut c, Method::POST, "/echo", &[], &vec![0; 100 * 1024]).await;
    assert_eq!(res.status, 413);

    let (res, body) = request(&mut c, Method::GET, "/big", &[], b"").await;
    assert_eq!(res.status, 200);
    assert_eq!(res.headers["content-length"], "300000");
    assert_eq!(body.len(), 300_000);

    let (res, body) = request(&mut c, Method::HEAD, "/big", &[], b"").await;
    assert_eq!(res.status, 200);
    assert_eq!(res.headers["content-length"], "300000");
    assert!(body.is_empty());

    let (res, body) = request(&mut c, Method::GET, "/stream", &[], b"").await;
    assert_eq!(res.status, 200);
    assert_eq!(
        String::from_utf8(body).unwrap(),
        "morceau 0\nmorceau 1\nmorceau 2\n"
    );

    // HTTP/2 découpe volontiers les cookies en plusieurs en-têtes.
    let (_, body) = request(
        &mut c,
        Method::GET,
        "/cookies",
        &[("cookie", "a=1"), ("cookie", "b=2")],
        b"",
    )
    .await;
    assert_eq!(String::from_utf8(body).unwrap(), r#"Some("1") Some("2")"#);

    let (res, _) = request(&mut c, Method::GET, "/nope", &[], b"").await;
    assert_eq!(res.status, 404);
    let (res, _) = request(&mut c, Method::DELETE, "/", &[], b"").await;
    assert_eq!(res.status, 405);
}

#[tokio::test]
async fn streams_run_concurrently_on_one_connection() {
    let addr = server(app()).await;
    let c = client(addr).await;
    let start = std::time::Instant::now();
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let mut c = c.clone();
        tasks.push(tokio::spawn(async move {
            request(&mut c, Method::GET, "/slow", &[], b"").await
        }));
    }
    for task in tasks {
        let (res, body) = task.await.unwrap();
        assert_eq!(res.status, 200);
        assert_eq!(body, "lent".as_bytes());
    }
    // Dix requêtes de 300 ms en parallèle, pas les unes après les autres.
    assert!(start.elapsed() < Duration::from_millis(1500));
}

#[tokio::test]
async fn http1_still_works_on_the_same_port() {
    let addr = server(app()).await;
    let mut tcp = TcpStream::connect(addr).await.unwrap();
    tcp.write_all(b"GET / HTTP/1.1\r\nhost: example.com\r\nconnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut out = String::new();
    tcp.read_to_string(&mut out).await.unwrap();
    assert!(out.starts_with("HTTP/1.1 200 OK\r\n"), "{out}");
    assert!(out.ends_with("HTTP/1.1 example.com"), "{out}");
}

#[tokio::test]
async fn graceful_shutdown_lets_streams_finish() {
    let server = app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(server.with_graceful_shutdown(async {
        let _ = stopped.await;
    }));
    let mut c = client(addr).await;
    let slow = tokio::spawn(async move { request(&mut c, Method::GET, "/slow", &[], b"").await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    stop.send(()).unwrap();
    let (res, body) = slow.await.unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(body, "lent".as_bytes());
    tokio::time::timeout(Duration::from_secs(5), running)
        .await
        .expect("the server stops once the stream is done")
        .unwrap()
        .unwrap();
}

/// WebSocket sur HTTP/2 (RFC 8441) : un CONNECT étendu, puis les trames
/// WebSocket dans les trames DATA du flux.
#[cfg(feature = "ws")]
mod websocket {
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use bytes::{Buf, Bytes};
    use futures_util::{SinkExt, StreamExt};
    use h2::{RecvStream, SendStream};
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use tokio_tungstenite::WebSocketStream;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::protocol::Role;

    use super::*;

    struct Tunnel {
        send: SendStream<Bytes>,
        recv: RecvStream,
        pending: Bytes,
    }

    impl AsyncRead for Tunnel {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if self.pending.is_empty() {
                match std::task::ready!(self.recv.poll_data(cx)) {
                    Some(Ok(data)) => {
                        let _ = self.recv.flow_control().release_capacity(data.len());
                        self.pending = data;
                    }
                    Some(Err(e)) => return Poll::Ready(Err(io::Error::other(e))),
                    None => return Poll::Ready(Ok(())),
                }
            }
            let n = self.pending.len().min(buf.remaining());
            buf.put_slice(&self.pending[..n]);
            self.pending.advance(n);
            Poll::Ready(Ok(()))
        }
    }

    impl AsyncWrite for Tunnel {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            loop {
                let capacity = self.send.capacity();
                if capacity > 0 {
                    let n = capacity.min(buf.len());
                    self.send
                        .send_data(Bytes::copy_from_slice(&buf[..n]), false)
                        .map_err(io::Error::other)?;
                    return Poll::Ready(Ok(n));
                }
                self.send.reserve_capacity(buf.len());
                match std::task::ready!(self.send.poll_capacity(cx)) {
                    Some(Ok(_)) => continue,
                    Some(Err(e)) => return Poll::Ready(Err(io::Error::other(e))),
                    None => return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
                }
            }
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            let _ = self.send.send_data(Bytes::new(), true);
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn websocket_over_http2() {
        let mut app = App::new();
        app.ws("/echo/:name", |req, mut socket| async move {
            let name = req.param("name").unwrap_or("?").to_owned();
            let _ = socket
                .send(format!("hello {name} over {:?}", req.version()))
                .await;
            while let Some(Ok(message)) = socket.recv().await {
                if socket.send(message).await.is_err() {
                    break;
                }
            }
        });
        let addr = server(app).await;
        let tcp = TcpStream::connect(addr).await.unwrap();
        let (client, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let mut client = client.ready().await.unwrap();
        // Le serveur annonce SETTINGS_ENABLE_CONNECT_PROTOCOL.
        for _ in 0..100 {
            if client.is_extended_connect_protocol_enabled() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(client.is_extended_connect_protocol_enabled());

        let mut req = http::Request::builder()
            .method(Method::CONNECT)
            .uri("http://localhost/echo/ada")
            .header("sec-websocket-version", "13")
            .body(())
            .unwrap();
        req.extensions_mut()
            .insert(h2::ext::Protocol::from_static("websocket"));
        let (res, send) = client.send_request(req, false).unwrap();
        let res = res.await.unwrap();
        assert_eq!(res.status(), 200);
        let recv = res.into_body();
        let tunnel = Tunnel {
            send,
            recv,
            pending: Bytes::new(),
        };
        let mut ws = WebSocketStream::from_raw_socket(tunnel, Role::Client, None).await;
        let hello = ws.next().await.unwrap().unwrap();
        assert_eq!(
            hello.into_text().unwrap().as_str(),
            "hello ada over HTTP/2.0"
        );
        for text in ["un", "deux", &"x".repeat(100_000)] {
            ws.send(Message::text(text)).await.unwrap();
            let echo = ws.next().await.unwrap().unwrap();
            assert_eq!(echo.into_text().unwrap().as_str(), text);
        }
        ws.close(None).await.unwrap();

        // Une route ordinaire refuse le CONNECT étendu comme un GET sans WebSocket.
        let mut req = http::Request::builder()
            .method(Method::CONNECT)
            .uri("http://localhost/nope")
            .header("sec-websocket-version", "13")
            .body(())
            .unwrap();
        req.extensions_mut()
            .insert(h2::ext::Protocol::from_static("websocket"));
        let (res, _send) = client.send_request(req, false).unwrap();
        assert_eq!(res.await.unwrap().status(), 404);
    }
}
