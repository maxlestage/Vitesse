# Shared state

Configuration, a database connection pool, a cache, counters: some data must be shared by all requests. In Vitesse you register it once with `app.state(value)` and read it from any handler or middleware with `req.state::<T>()`, the typed equivalent of Express's `app.locals`.

## Registering and reading state

```rust
use vitesse::prelude::*;

struct Config {
    app_name: String,
    max_items: usize,
}

let mut app = App::new();
app.state(Config { app_name: "My API".into(), max_items: 50 });

app.get("/", |req: Request| async move {
    let config = req.state::<Config>();
    format!("{} (max {} items)", config.app_name, config.max_items)
});
```

State is looked up **by type**: `req.state::<Config>()` returns the `Config` you registered. A few rules follow from that:

- **One value per type.** Calling `app.state(...)` twice with the same type replaces the first value. Rather than registering a bare `String` or `u32`, give each piece of state its own type (a struct, or a newtype like `struct ApiKey(String)`), or group everything into one `AppState` struct.
- **The type must be `Send + Sync + 'static`**, because it is shared by all the server's threads.
- **`req.state::<T>()` returns a `&'static T`**: no `Arc`, no clone, no lock. You can keep the reference across `.await`s.
- **Forgetting to register it** is a programming error: `req.state::<T>()` panics, the request gets a `500` and the server logs show the message "no state of type `my_app::Config`: call `app.state(...)` at startup". When the state is optional, use `req.try_state::<T>()`, which returns an `Option<&T>`.

State is also available in middleware, through the same `req.state::<T>()`.

## Mutable state

The state is shared by every thread, so Rust only lets you modify it through types designed for concurrent access ("interior mutability").

| Need | Type |
|---|---|
| A counter, a flag | `AtomicU64`, `AtomicBool`… (`std::sync::atomic`) |
| A collection read often, written sometimes | `std::sync::RwLock` |
| Short, simple writes | `std::sync::Mutex` |
| A lock held across an `.await` | `tokio::sync::Mutex` or `tokio::sync::RwLock` |

### Atomics

The fastest way to count:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

app.state(AtomicU64::new(0));
app.get("/visits", |req: Request| async move {
    let n = req.state::<AtomicU64>().fetch_add(1, Ordering::Relaxed) + 1;
    format!("visit #{n}")
});
```

### `Mutex` and `RwLock`

An in-memory store, shared by all requests:

```rust
use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use vitesse::prelude::*;

#[derive(Clone, Serialize)]
struct Todo {
    id: u64,
    title: String,
}

#[derive(Deserialize)]
struct NewTodo {
    title: String,
}

#[derive(Default)]
struct Store {
    next_id: AtomicU64,
    todos: RwLock<HashMap<u64, Todo>>,
}

async fn create(req: Request) -> vitesse::Result<(StatusCode, Json<Todo>)> {
    let new: NewTodo = req.json().await?;
    let store = req.state::<Store>();
    let id = store.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let todo = Todo { id, title: new.title };
    store.todos.write().unwrap().insert(id, todo.clone());
    Ok((StatusCode::CREATED, Json(todo)))
}

async fn list(req: Request) -> Json<Vec<Todo>> {
    let todos = req.state::<Store>().todos.read().unwrap();
    Json(todos.values().cloned().collect())
}

let mut app = App::new();
app.state(Store::default());
app.post("/todos", create);
app.get("/todos", list);
```

> [!WARNING]
> Never hold a `std::sync::Mutex` or `RwLock` guard across an `.await`: the handler would no longer compile (`future cannot be sent between threads safely`), and that's a good thing, because the lock would block other requests. Read the body (`req.json().await?`) *before* taking the lock, as above. If you really need to keep a lock while awaiting, use `tokio::sync::Mutex`, whose `lock()` is itself awaited: `let mut guard = req.state::<MyState>().items.lock().await;`.

## One struct for the whole application

In a real application, group everything into one struct: a single `app.state(...)` call, and a single type to look up.

```rust
struct AppState {
    config: Config,
    visits: AtomicU64,
    store: Store,
}

app.state(AppState {
    config: Config { app_name: "My API".into(), max_items: 50 },
    visits: AtomicU64::new(0),
    store: Store::default(),
});

app.get("/about", |req: Request| async move {
    let state = req.state::<AppState>();
    state.visits.fetch_add(1, Ordering::Relaxed);
    state.config.app_name.clone()
});
```

## Sharing state outside the server

`app.state(value)` moves the value into the application. If other code also needs it (a background task, for instance), wrap it in an `Arc`, register a clone, and read it with `req.state::<Arc<T>>()`:

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use vitesse::prelude::*;

#[derive(Default)]
struct Stats {
    requests: AtomicU64,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let stats = Arc::new(Stats::default());

    let mut app = App::new();
    app.state(stats.clone());
    app.middleware(|req: Request, next: Next| async move {
        req.state::<Arc<Stats>>().requests.fetch_add(1, Ordering::Relaxed);
        next.run(req).await
    });
    app.get("/", |_| async { "ok" });

    // A background task that reads the same state.
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            println!("{} requests so far", stats.requests.load(Ordering::Relaxed));
        }
    });

    app.listen(3000).await
}
```

Here the server is started with `app.listen(...).await` inside `#[tokio::main]` so that the background task and the server share the same tokio runtime (see [Server](server.md)).

You can also capture values in a closure, as you would in JavaScript. Handlers can be called many times, so clone what you capture for each request:

```rust
let greeting = Arc::new(String::from("Hello"));
app.get("/hello", move |_| {
    let greeting = greeting.clone();
    async move { format!("{greeting}!") }
});
```

## Database connection pools

A connection pool is the typical piece of state: create it at startup, register it, and use it from handlers. Here is an illustration with [sqlx](https://docs.rs/sqlx) and PostgreSQL (sqlx is an external crate, and this snippet is a sketch to adapt, not tested code); the pattern is the same with deadpool, diesel-async, or a Redis client.

```toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres"] }
```

```rust
use sqlx::PgPool;
use vitesse::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPool::connect(&std::env::var("DATABASE_URL")?).await?;

    let mut app = App::new();
    app.state(pool);
    app.get("/users/:id", |req: Request| async move {
        let id: i64 = req.param_as("id")?;
        let name: Option<String> = sqlx::query_scalar("SELECT name FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(req.state::<PgPool>())
            .await?; // a database error becomes a 500
        let name = name.ok_or_else(|| Error::not_found("user not found"))?;
        Ok::<_, Error>(Json(json!({ "id": id, "name": name })))
    });

    app.listen(3000).await?;
    Ok(())
}
```

> [!TIP]
> Creating a pool is asynchronous: do it inside `#[tokio::main]`, then start the server with `app.listen(...).await` on that same runtime rather than with `app.run(...)`, which creates its own runtimes.

## Per-request data

`app.state` is shared by *all* requests. For data that belongs to *one* request (the authenticated user, a request ID…), a middleware attaches it with `req.set(value)` and handlers read it with `req.get::<T>()`, the equivalent of `res.locals` or `req.user` in Express:

```rust
#[derive(Clone)]
struct CurrentUser {
    id: u64,
}

app.middleware(|mut req: Request, next: Next| async move {
    if let Some(id) = req.header("x-user-id").and_then(|v| v.parse().ok()) {
        req.set(CurrentUser { id });
    }
    next.run(req).await
});

app.get("/me", |req: Request| async move {
    match req.get::<CurrentUser>() {
        Some(user) => format!("user {}", user.id),
        None => "anonymous".to_string(),
    }
});
```

Values are stored by type, like the state, and must implement `Clone + Send + Sync`. For lower-level access, `req.extensions()` and `req.extensions_mut()` expose the underlying `http::Extensions`.

| | `app.state(value)` | `req.set(value)` |
|---|---|---|
| Lifetime | the whole application | one request |
| Set by | your startup code | a middleware |
| Read with | `req.state::<T>()` / `req.try_state::<T>()` | `req.get::<T>()` |
| Express equivalent | `app.locals` | `res.locals`, `req.user` |
