//! A WebSocket chat room: every message sent by one client is broadcast to
//! all the others.
//!
//! ```sh
//! cargo run --release --example chat
//!
//! # In two terminals (websocat is a Rust tool: cargo install websocat)
//! websocat ws://localhost:3000/chat/ada
//! websocat ws://localhost:3000/chat/grace
//! ```

use tokio::sync::broadcast;
use vitesse::prelude::*;
use vitesse::ws::Message;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    // Every connected client listens to this channel.
    let (room, _) = broadcast::channel::<String>(256);
    app.state(room);

    app.get("/", |_| async {
        "Connect with a WebSocket client: websocat ws://localhost:3000/chat/<name>"
    });

    app.ws("/chat/:name", |req, socket| async move {
        let name = req.param("name").unwrap_or("anonymous").to_owned();
        let room = req.state::<broadcast::Sender<String>>();
        let mut inbox = room.subscribe();
        let (mut tx, mut rx) = socket.split();

        let _ = room.send(format!("* {name} joined"));
        // Forward the room's messages to this client…
        let forward = tokio::spawn(async move {
            while let Ok(line) = inbox.recv().await {
                if tx.send(line).await.is_err() {
                    break;
                }
            }
        });
        // …and broadcast what this client says.
        while let Some(Ok(message)) = rx.recv().await {
            match message {
                Message::Text(text) => {
                    let _ = room.send(format!("{name}: {}", text.trim_end()));
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        forward.abort();
        let _ = room.send(format!("* {name} left"));
    });

    println!("⚡ Chat on ws://localhost:3000/chat/<name>");
    app.run(3000)
}
