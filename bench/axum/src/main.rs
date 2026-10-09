//! Serveur axum de référence, mêmes routes que `examples/bench.rs`.

use axum::extract::Path;
use axum::routing::{get, post};
use axum::serve::ListenerExt;
use axum::{Json, Router};
use serde_json::{Value, json};

#[derive(serde::Serialize)]
struct Message {
    message: &'static str,
}

fn main() {
    let workers = std::env::var("WORKERS")
        .ok()
        .and_then(|w| w.parse().ok())
        .unwrap_or(2);
    let port = std::env::var("PORT").unwrap_or_else(|_| "3002".into());
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let app = Router::new()
                .route("/", get(|| async { "Hello, World!" }))
                .route(
                    "/json",
                    get(|| async {
                        Json(Message {
                            message: "Hello, World!",
                        })
                    }),
                )
                .route(
                    "/users/{id}",
                    get(|Path(id): Path<String>| async move {
                        Json(json!({ "id": id, "name": "Ada" }))
                    }),
                )
                .route(
                    "/echo",
                    post(|Json(body): Json<Value>| async move { Json(body) }),
                );
            let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
                .await
                .unwrap()
                // Sans TCP_NODELAY, Nagle retarde les réponses pipelinées.
                .tap_io(|tcp| {
                    let _ = tcp.set_nodelay(true);
                });
            axum::serve(listener, app).await.unwrap();
        });
}
