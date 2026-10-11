//! Un serveur sur un socket Unix, comme derrière Nginx sur la même machine.
#![cfg(unix)]

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use vitesse::prelude::*;

#[tokio::test]
async fn serves_over_a_unix_socket() {
    let dir = std::env::temp_dir().join(format!("vitesse-unix-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("app.sock");
    // Un socket laissé par une exécution précédente est remplacé.
    drop(std::os::unix::net::UnixListener::bind(&path).unwrap());

    let mut app = App::new();
    app.get("/", |req: Request| async move {
        format!("{:?} {:?}", req.ip(), req.header("host"))
    });
    let server = app.bind_unix(&path).await.unwrap();
    assert_eq!(server.path(), path);
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(server.with_graceful_shutdown(async {
        let _ = stopped.await;
    }));

    // Un deuxième serveur sur le même socket est refusé tant que le premier tourne.
    let mut other = App::new();
    other.get("/", |_| async { "other" });
    let err = other.bind_unix(&path).await.err().unwrap();
    assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);

    let mut stream = UnixStream::connect(&path).await.unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut out = String::new();
    stream.read_to_string(&mut out).await.unwrap();
    assert!(out.starts_with("HTTP/1.1 200 OK\r\n"), "{out}");
    assert!(out.ends_with("None Some(\"localhost\")"), "{out}");

    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    // Le fichier du socket est retiré à l'arrêt.
    assert!(!path.exists());
    // Un fichier ordinaire n'est jamais supprimé.
    std::fs::write(&path, "not a socket").unwrap();
    let mut app = App::new();
    app.get("/", |_| async { "x" });
    assert!(app.bind_unix(&path).await.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "not a socket");
    std::fs::remove_dir_all(&dir).unwrap();
}
