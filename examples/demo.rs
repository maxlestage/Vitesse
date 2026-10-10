//! The demo app deployed by the "Deploy to Heroku" button and the Docker
//! image: a landing page, a health check and a small in-memory REST API.
//!
//! It listens on the `PORT` environment variable (set by Heroku, Render,
//! Cloud Run…), or on port 3000 when it is not set.
//!
//! ```sh
//! cargo run --release --example demo
//!
//! curl localhost:3000/api/hello/Ada
//! curl -X POST localhost:3000/api/todos -H 'content-type: application/json' \
//!      -d '{"title":"Deploy from my phone"}'
//! curl localhost:3000/api/todos
//! curl -X DELETE localhost:3000/api/todos/1
//! ```

use std::collections::BTreeMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use vitesse::prelude::*;

const INDEX: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Vitesse is running</title>
  <style>
    :root { color-scheme: dark; }
    body { margin: 0; min-height: 100vh; display: grid; place-items: center;
           font: 16px/1.6 system-ui, sans-serif; background: #07070b; color: #f4f4f8; }
    main { max-width: 34rem; padding: 2rem 1.25rem; }
    h1 { font-size: clamp(2rem, 8vw, 3.2rem); line-height: 1.05; margin: 0 0 1rem; }
    h1 span { background: linear-gradient(90deg, #ffd23f, #ff7a1a, #ff2e63);
              -webkit-background-clip: text; background-clip: text; color: transparent; }
    p { color: #a6a6b8; }
    code { font: 0.9em ui-monospace, monospace; background: #15151d; padding: 0.15em 0.4em; border-radius: 6px; }
    li { margin: 0.35rem 0; }
    a { color: #ffd23f; }
  </style>
</head>
<body>
  <main>
    <h1>⚡ Vitesse is <span>running</span>.</h1>
    <p>This server is written in Rust with Vitesse, an Express-style web framework.</p>
    <ul>
      <li><a href="/api/hello/world"><code>GET /api/hello/:name</code></a></li>
      <li><a href="/api/todos"><code>GET /api/todos</code></a>, <code>POST /api/todos</code></li>
      <li><code>GET</code> and <code>DELETE /api/todos/:id</code></li>
      <li><a href="/health"><code>GET /health</code></a></li>
    </ul>
    <p><a href="https://maxlestage.github.io/Vitesse/#/docs">Read the documentation</a></p>
  </main>
</body>
</html>
"#;

#[derive(Clone, Serialize)]
struct Todo {
    id: u64,
    title: String,
    done: bool,
}

#[derive(Deserialize)]
struct NewTodo {
    title: String,
}

/// The "database", shared by every request (it lives as long as the dyno).
#[derive(Default)]
struct Db {
    next_id: AtomicU64,
    todos: RwLock<BTreeMap<u64, Todo>>,
}

async fn list(req: Request) -> Json<Vec<Todo>> {
    let todos = req.state::<Db>().todos.read().unwrap();
    Json(todos.values().cloned().collect())
}

async fn create(req: Request) -> vitesse::Result<Response> {
    let new: NewTodo = req.json().await?;
    let title = new.title.trim();
    if title.is_empty() {
        return Err(Error::unprocessable("the title is required"));
    }
    let db = req.state::<Db>();
    let id = db.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let todo = Todo {
        id,
        title: title.to_owned(),
        done: false,
    };
    db.todos.write().unwrap().insert(id, todo.clone());
    Ok(res::status(201)
        .header("location", format!("/api/todos/{id}"))
        .json(todo))
}

async fn show(req: Request) -> vitesse::Result<Json<Todo>> {
    let id: u64 = req.param_as("id")?;
    let todos = req.state::<Db>().todos.read().unwrap();
    todos
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| Error::not_found(format!("todo {id} not found")))
}

async fn remove(req: Request) -> vitesse::Result<StatusCode> {
    let id: u64 = req.param_as("id")?;
    match req.state::<Db>().todos.write().unwrap().remove(&id) {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(Error::not_found(format!("todo {id} not found"))),
    }
}

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.state(Db::default());
    app.middleware(middleware::logger());

    app.get("/", |_| async { Html(INDEX) });
    app.get("/health", |_| async { "ok" });
    app.get("/api/hello/:name", |req: Request| async move {
        let name = req.param("name").unwrap_or("world");
        Json(json!({ "message": format!("Hello, {name}!") }))
    });
    app.get("/api/todos", list);
    app.post("/api/todos", create);
    app.get("/api/todos/:id", show);
    app.delete("/api/todos/:id", remove);

    // Heroku (and most platforms) tell the app which port to use.
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
    println!("⚡ Vitesse demo listening on http://0.0.0.0:{port}");
    app.run(port)
}
