//! Serveur actix-web de référence, mêmes routes que `examples/bench.rs`.

use actix_web::{App, HttpResponse, HttpServer, web};
use serde_json::{Value, json};

#[derive(serde::Serialize)]
struct Message {
    message: &'static str,
}

async fn hello() -> &'static str {
    "Hello, World!"
}

async fn message() -> HttpResponse {
    HttpResponse::Ok().json(Message {
        message: "Hello, World!",
    })
}

async fn user(id: web::Path<String>) -> HttpResponse {
    HttpResponse::Ok().json(json!({ "id": id.into_inner(), "name": "Ada" }))
}

async fn echo(body: web::Json<Value>) -> HttpResponse {
    HttpResponse::Ok().json(body.into_inner())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let workers = std::env::var("WORKERS")
        .ok()
        .and_then(|w| w.parse().ok())
        .unwrap_or(2);
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3004);
    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(hello))
            .route("/json", web::get().to(message))
            .route("/users/{id}", web::get().to(user))
            .route("/echo", web::post().to(echo))
    })
    .workers(workers)
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
