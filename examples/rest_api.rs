//! A complete REST API (CRUD for tasks): router, middlewares, shared state,
//! validation and errors.
//!
//! ```sh
//! cargo run --release --example rest_api
//!
//! curl -X POST localhost:3000/api/todos -H 'content-type: application/json' \
//!      -H 'authorization: Bearer secret' -d '{"title":"Learn Rust"}'
//! curl localhost:3000/api/todos
//! curl -X PATCH localhost:3000/api/todos/1 -H 'authorization: Bearer secret' \
//!      -H 'content-type: application/json' -d '{"done":true}'
//! curl -X DELETE localhost:3000/api/todos/1 -H 'authorization: Bearer secret'
//! ```

use std::collections::BTreeMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use vitesse::prelude::*;

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

#[derive(Deserialize)]
struct PatchTodo {
    title: Option<String>,
    done: Option<bool>,
}

#[derive(Deserialize)]
struct ListQuery {
    done: Option<bool>,
}

/// The "database": shared by all requests.
#[derive(Default)]
struct Db {
    next_id: AtomicU64,
    todos: RwLock<BTreeMap<u64, Todo>>,
}

/// The authenticated user, attached to the request by the `auth` middleware.
#[derive(Clone)]
struct User(String);

async fn auth(mut req: Request, next: Next) -> Response {
    // Reads are public, writes require a token.
    if *req.method() != Method::GET {
        match req.header("authorization") {
            Some("Bearer secret") => {
                req.set(User("ada".into()));
            }
            _ => return Error::unauthorized("missing or invalid token").into_response(),
        }
    }
    next.run(req).await
}

async fn list(req: Request) -> vitesse::Result<Json<Vec<Todo>>> {
    let query: ListQuery = req.query_as()?;
    let db = req.state::<Db>();
    let todos = db.todos.read().unwrap();
    Ok(Json(
        todos
            .values()
            .filter(|t| query.done.is_none_or(|d| t.done == d))
            .cloned()
            .collect(),
    ))
}

async fn show(req: Request) -> vitesse::Result<Json<Todo>> {
    let id: u64 = req.param_as("id")?;
    let todos = req.state::<Db>().todos.read().unwrap();
    todos
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| Error::not_found(format!("task {id} not found")))
}

async fn create(req: Request) -> vitesse::Result<Response> {
    let new: NewTodo = req.json().await?;
    if new.title.trim().is_empty() {
        return Err(Error::unprocessable("title is required"));
    }
    let db = req.state::<Db>();
    let id = db.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let todo = Todo {
        id,
        title: new.title,
        done: false,
    };
    db.todos.write().unwrap().insert(id, todo.clone());
    let author = req.get::<User>().map_or("?", |u| u.0.as_str());
    Ok(res::status(201)
        .header("location", format!("/api/todos/{id}"))
        .header("x-created-by", author)
        .json(todo))
}

async fn update(req: Request) -> vitesse::Result<Json<Todo>> {
    let id: u64 = req.param_as("id")?;
    let patch: PatchTodo = req.json().await?;
    let mut todos = req.state::<Db>().todos.write().unwrap();
    let todo = todos
        .get_mut(&id)
        .ok_or_else(|| Error::not_found(format!("task {id} not found")))?;
    if let Some(title) = patch.title {
        todo.title = title;
    }
    if let Some(done) = patch.done {
        todo.done = done;
    }
    Ok(Json(todo.clone()))
}

async fn remove(req: Request) -> vitesse::Result<StatusCode> {
    let id: u64 = req.param_as("id")?;
    match req.state::<Db>().todos.write().unwrap().remove(&id) {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(Error::not_found(format!("task {id} not found"))),
    }
}

fn main() -> std::io::Result<()> {
    let mut todos = Router::new();
    todos.middleware(auth);
    todos
        .get("/", list)
        .post("/", create)
        .get("/:id", show)
        .patch("/:id", update)
        .delete("/:id", remove);

    let mut app = App::new();
    app.state(Db::default());
    app.middleware(middleware::logger());
    app.middleware(middleware::cors());
    app.middleware(middleware::helmet());

    app.get("/", |_| async {
        Html("<h1>Task API</h1><p>Try <a href=\"/api/todos\">/api/todos</a></p>")
    });
    app.get("/health", |_| async { json!({ "status": "ok" }) });
    app.mount("/api/todos", todos);

    println!("⚡ API running at http://localhost:3000/api/todos");
    app.run(3000)
}
