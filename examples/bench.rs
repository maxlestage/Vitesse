//! Serveur utilisé par `bench/run.sh` (mêmes routes que `bench/express/server.js`).
//!
//! Variables d'environnement : `PORT`, `WORKERS`, et `VITESSE_MODE=mt` pour
//! utiliser le runtime multi-thread de tokio au lieu d'un thread par cœur.

use vitesse::prelude::*;

#[derive(serde::Serialize)]
struct Message {
    message: &'static str,
}

fn app() -> App {
    let mut app = App::new();
    app.get("/", |_| async { "Hello, World!" });
    app.get("/json", |_| async {
        Json(Message {
            message: "Hello, World!",
        })
    });
    app.get("/users/:id", |req: Request| async move {
        Json(json!({ "id": req.param("id"), "name": "Ada" }))
    });
    app.post("/echo", |req: Request| async move {
        let body: serde_json::Value = req.json().await?;
        Ok::<_, Error>(Json(body))
    });
    app
}

fn main() -> std::io::Result<()> {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let mut app = app();
    if let Some(n) = std::env::var("WORKERS").ok().and_then(|w| w.parse().ok()) {
        app.workers(n);
    }
    if std::env::var("VITESSE_MODE").as_deref() == Ok("mt") {
        app.thread_per_core(false);
    }
    app.run(port)
}
