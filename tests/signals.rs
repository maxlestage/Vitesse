//! Arrêt propre sur `SIGTERM` avec `Server::run` (et donc `app.listen`).
//! Dans son propre binaire de test : le signal vise tout le processus.
#![cfg(unix)]

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vitesse::prelude::*;

#[tokio::test]
async fn server_run_stops_gracefully_on_sigterm() {
    let mut app = App::new();
    app.get("/slow", |_| async {
        tokio::time::sleep(Duration::from_millis(400)).await;
        "done"
    });
    let server = app.bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    let running = tokio::spawn(server.run());

    // Une requête en cours au moment du signal…
    let in_flight = tokio::spawn(async move {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET /slow HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut out = String::new();
        stream.read_to_string(&mut out).await.unwrap();
        out
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    let status = std::process::Command::new("kill")
        .args(["-TERM", &std::process::id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());

    // … se termine normalement, puis le serveur s'arrête de lui-même.
    let response = in_flight.await.unwrap();
    assert!(response.ends_with("done"), "{response}");
    let stopped = tokio::time::timeout(Duration::from_secs(5), running).await;
    assert!(matches!(stopped, Ok(Ok(Ok(())))), "{stopped:?}");
}
