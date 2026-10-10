# Coming from Express

If you know Express, you already know most of Vitesse: the same routes, the same `req`, the same middleware with `next`, the same routers. This guide maps every Express concept to its Vitesse equivalent, shows the same CRUD API written with both, and lists the differences that tend to surprise newcomers.

## Five ideas to keep in mind

1. **A handler returns its response.** There is no `res` object to mutate: you return a string, `Json(...)`, a status code, or a `Response` you built.
2. **Everything is async.** A handler is an `async` function (or an `async` closure) that receives the `Request`.
3. **Data is typed.** JSON bodies and query strings are deserialised into Rust structs with [serde](https://serde.rs); invalid input is rejected with a `400` before your code even runs.
4. **Errors go up with `?`.** A handler can return a `Result`: `?` stops at the first error and turns it into an HTTP response.
5. **Mistakes are caught early.** The compiler checks types, and an invalid or duplicate route stops the program at startup, pointing at the faulty line.

## Concept by concept

### Application and server

| Express | Vitesse |
|---|---|
| `const app = express()` | `let mut app = App::new();` |
| `app.listen(3000, callback)` | `app.run(3000)` (blocks; print your message before) |
| `app.listen(3000)` inside an existing async setup | `app.listen(3000).await` |
| `process.env.PORT` | `std::env::var("PORT")` |
| `app.locals.db = db` | `app.state(db)`, read with `req.state::<Db>()` |
| `app.use((req, res) => res.status(404)…)` | `app.fallback(handler)` |

### Routing

| Express | Vitesse |
|---|---|
| `app.get('/users/:id', handler)` | `app.get("/users/:id", handler)` |
| `app.post` / `put` / `patch` / `delete` / `all` | Same names |
| `app.route('/todos').get(a).post(b)` | `app.get("/todos", a).post("/todos", b)` (calls can be chained) |
| Any other method | `app.route(Method::from_bytes(b"PURGE").unwrap(), "/cache", handler)` |
| `/files/*path` (Express 5) | `/files/*path`, read with `req.param("path")` |
| `express.Router()` | `Router::new()` |
| `app.use('/api', router)` | `app.mount("/api", router)` |
| `router.use(mw)` | `router.middleware(mw)`, which also runs for the `404`s under the router's prefix |
| `app.ws('/echo', (ws, req) => …)` (express-ws) | `app.ws("/echo", \|req, socket\| async move { … })`: the request comes first, and a loop replaces `ws.on('message')` (see [WebSocket](websocket.md)) |

### Request

| Express | Vitesse |
|---|---|
| `req.params.id` | `req.param("id")` (an `Option<&str>`), or `req.param_as::<u64>("id")?` |
| `req.query.page` | `req.query("page")`, or a whole struct with `req.query_as::<T>()?` |
| `express.json()` + `req.body` | `req.json::<T>().await?` |
| `express.urlencoded()` + `req.body` | `req.form::<T>().await?` |
| `express.text()` / `express.raw()` | `req.text().await?` / `req.bytes().await?` |
| `req.get('host')` | `req.header("host")` |
| `req.cookies.session` (cookie-parser) | `req.cookie("session")`, built in |
| `req.ip` / `req.hostname` | `req.ip()` / `req.hostname()` |
| `req.method` / `req.path` / `req.originalUrl` | `req.method()` / `req.path()` / `req.uri()` |
| `req.is('json')` | `req.is("json")` |
| `res.locals.user = user` | `req.set(user)`, read with `req.get::<User>()` |

### Response

| Express | Vitesse |
|---|---|
| `res.send('text')` | Return `"text"` (or a `String`) |
| `res.json(obj)` | Return `Json(obj)` or `json!({ ... })` |
| `res.status(201).json(obj)` | Return `(201, Json(obj))` or `res::status(201).json(obj)` |
| `res.sendStatus(204)` | Return `StatusCode::NO_CONTENT` |
| `res.redirect('/login')` | Return `Redirect::to("/login")` (302) |
| `res.redirect(301, '/new')` | Return `Redirect::permanent("/new")` |
| `res.set('X-Foo', 'bar')` | `Response::new().header("x-foo", "bar")` |
| `res.type('text/csv')` | `.content_type("text/csv")` |
| `res.cookie('theme', 'dark', { httpOnly: true })` | `.cookie(Cookie::new("theme", "dark").http_only(true))` |
| `res.clearCookie('theme')` | `.clear_cookie("theme")` |
| `res.attachment('report.csv')` | `.attachment("report.csv")` |
| `res.sendFile(path)` | `res::file(path).await` |
| `res.download(path, 'report.pdf')` | `res::download(path, "report.pdf").await` |

### Middleware and errors

| Express | Vitesse |
|---|---|
| `app.use(fn)` | `app.middleware(fn)` |
| `function mw(req, res, next)` | `async fn mw(req: Request, next: Next) -> Response` |
| `next()` | `next.run(req).await` |
| `app.get('/admin', auth, handler)` | `app.get("/admin", handler.with(auth))` |
| `next(err)` / `throw err` | `return Err(Error::bad_request("…"))`, or `?` |
| `app.use((err, req, res, next) => …)` | `app.on_error(f)`, with `f` a function `Fn(Error) -> impl IntoResponse` |
| `morgan('dev')` | `middleware::logger()` |
| `cors()` | `middleware::cors()` |
| `helmet()` | `middleware::helmet()` |
| `connect-timeout` | `middleware::timeout(duration)` |
| `express.static('public')` | `app.middleware(ServeDir::new("public"))` |
| `app.use('/static', express.static('public'))` | `app.static_dir("/static", "public")` |

### Tests

| supertest | Vitesse |
|---|---|
| `request(app).get('/')` | `TestClient::new(app()).get("/").await` |
| `.set('authorization', 'Bearer x')` | `.header("authorization", "Bearer x")` |
| `.send({ title: 'x' })` | `.json(&json!({ "title": "x" }))` |
| `expect(res.status).toBe(200)` | `assert_eq!(res.status(), 200)` |
| `res.body` | `res.json::<T>()` |

## A CRUD API, side by side

The same in-memory to-do API, first with Express:

```js
const express = require('express');

const app = express();
app.use(express.json());

const todos = new Map();
let nextId = 0;

app.get('/todos', (req, res) => {
  res.json([...todos.values()]);
});

app.get('/todos/:id', (req, res) => {
  const todo = todos.get(Number(req.params.id));
  if (!todo) return res.status(404).json({ error: 'todo not found' });
  res.json(todo);
});

app.post('/todos', (req, res) => {
  const { title } = req.body ?? {};
  if (!title || !title.trim()) {
    return res.status(422).json({ error: 'title is required' });
  }
  const todo = { id: ++nextId, title, done: false };
  todos.set(todo.id, todo);
  res.status(201).json(todo);
});

app.patch('/todos/:id', (req, res) => {
  const todo = todos.get(Number(req.params.id));
  if (!todo) return res.status(404).json({ error: 'todo not found' });
  const { title, done } = req.body ?? {};
  if (title !== undefined) todo.title = title;
  if (done !== undefined) todo.done = done;
  res.json(todo);
});

app.delete('/todos/:id', (req, res) => {
  if (!todos.delete(Number(req.params.id))) {
    return res.status(404).json({ error: 'todo not found' });
  }
  res.sendStatus(204);
});

app.listen(3000, () => console.log('Listening on http://localhost:3000'));
```

Then with Vitesse:

```rust
use std::collections::BTreeMap;
use std::sync::Mutex;

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

/// The in-memory store, shared by every request (the `Mutex` makes it thread-safe).
#[derive(Default)]
struct Store {
    next_id: u64,
    todos: BTreeMap<u64, Todo>,
}

type Db = Mutex<Store>;

fn not_found() -> Error {
    Error::not_found("todo not found")
}

async fn list(req: Request) -> Json<Vec<Todo>> {
    let store = req.state::<Db>().lock().unwrap();
    Json(store.todos.values().cloned().collect())
}

async fn show(req: Request) -> vitesse::Result<Json<Todo>> {
    let id: u64 = req.param_as("id")?; // 400 if it is not a number
    let store = req.state::<Db>().lock().unwrap();
    let todo = store.todos.get(&id).ok_or_else(not_found)?;
    Ok(Json(todo.clone()))
}

async fn create(req: Request) -> vitesse::Result<(StatusCode, Json<Todo>)> {
    let new: NewTodo = req.json().await?; // 400 if the JSON is invalid or `title` is missing
    if new.title.trim().is_empty() {
        return Err(Error::unprocessable("title is required"));
    }
    let mut store = req.state::<Db>().lock().unwrap();
    store.next_id += 1;
    let todo = Todo { id: store.next_id, title: new.title, done: false };
    store.todos.insert(todo.id, todo.clone());
    Ok((StatusCode::CREATED, Json(todo)))
}

async fn update(req: Request) -> vitesse::Result<Json<Todo>> {
    let id: u64 = req.param_as("id")?;
    let patch: PatchTodo = req.json().await?;
    let mut store = req.state::<Db>().lock().unwrap();
    let todo = store.todos.get_mut(&id).ok_or_else(not_found)?;
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
    let mut store = req.state::<Db>().lock().unwrap();
    store.todos.remove(&id).ok_or_else(not_found)?;
    Ok(StatusCode::NO_CONTENT)
}

fn app() -> App {
    let mut app = App::new();
    app.state(Db::default());

    app.get("/todos", list)
        .post("/todos", create)
        .get("/todos/:id", show)
        .patch("/todos/:id", update)
        .delete("/todos/:id", remove);

    app
}

fn main() -> std::io::Result<()> {
    println!("Listening on http://localhost:3000");
    app().run(3000)
}
```

What changed:

- **The shape of the data is declared once.** `NewTodo` says that `title` is a required string: `{}` or `{"title": 42}` is rejected with a `400` and a message explaining what is wrong. In Express, that validation is up to you.
- **Parameters are parsed, not just read.** `Number('abc')` silently gives `NaN` (and a 404 here); `req.param_as::<u64>("id")?` answers `400` right away.
- **"Not found" is a value.** `ok_or_else(not_found)?` turns a missing entry into a `404` and leaves the function, instead of an early `return res.status(404)…`.
- **The global `Map` becomes state.** Requests run in parallel on several threads, so the store is registered with `app.state` and protected by a `Mutex`.
- **Statuses are in the return type.** `(StatusCode::CREATED, Json(todo))` for a 201, `StatusCode::NO_CONTENT` for a 204: you cannot forget to send a response.

## Middleware, side by side

```js
// Log every request with its duration.
app.use((req, res, next) => {
  const start = Date.now();
  res.on('finish', () => console.log(`${req.method} ${req.path} ${Date.now() - start} ms`));
  next();
});

// Protect one route.
function auth(req, res, next) {
  if (req.get('authorization') !== 'Bearer secret') {
    return res.status(401).json({ error: 'please log in' });
  }
  res.locals.user = { name: 'ada' };
  next();
}

app.get('/admin', auth, (req, res) => res.send(`Welcome, ${res.locals.user.name}`));
```

```rust
use std::time::Instant;
use vitesse::prelude::*;

#[derive(Clone)]
struct User {
    name: String,
}

async fn auth(mut req: Request, next: Next) -> Response {
    if req.header("authorization") != Some("Bearer secret") {
        return Error::unauthorized("please log in").into_response();
    }
    req.set(User { name: "ada".into() }); // res.locals.user = …
    next.run(req).await                    // next()
}

async fn admin(req: Request) -> String {
    let user = req.get::<User>().unwrap(); // set by `auth`
    format!("Welcome, {}", user.name)
}

fn app() -> App {
    let mut app = App::new();

    // Log every request with its duration.
    app.middleware(|req: Request, next: Next| async move {
        let start = Instant::now();
        let line = format!("{} {}", req.method(), req.path());
        let res = next.run(req).await;
        println!("{line} {:?}", start.elapsed());
        res
    });

    // Protect one route.
    app.get("/admin", admin.with(auth));
    app
}
```

There is no `res.on('finish')`: the code after `next.run(req).await` runs once the rest of the chain has produced the response, and it can even modify it (`res.header(...)`) before returning it. A middleware that wants to stop the request simply returns a response without calling `next`. See [Middleware](middleware.md).

## Errors, side by side

```js
app.get('/users/:id', async (req, res) => {
  const user = await findUser(req.params.id);
  if (!user) return res.status(404).json({ error: 'user not found' });
  res.json(user);
});

app.use((err, req, res, next) => {
  console.error(err);
  res.status(err.status || 500).json({ message: err.message });
});
```

```rust
// `find_user` is your database call: it returns `Result<Option<User>, _>`.
async fn show_user(req: Request) -> vitesse::Result<Json<User>> {
    let id: u64 = req.param_as("id")?;
    let user = find_user(id).await?.ok_or_else(|| Error::not_found("user not found"))?;
    Ok(Json(user))
}

app.get("/users/:id", show_user);

app.on_error(|err: Error| {
    res::status(err.status()).json(json!({ "message": err.message() }))
});
```

`Error` carries an HTTP status and a message for the client. Any other error (database, I/O, parsing…) converts automatically with `?` into a `500` whose details are only logged, never sent to the client. `app.on_error` applies to every error response, wherever it was registered: your errors, 404s, 405s, invalid bodies and panics. See [Errors](errors.md).

## Tests, side by side

```js
const request = require('supertest');
const app = require('./app');

test('creates a todo', async () => {
  const res = await request(app).post('/todos').send({ title: 'Learn Rust' });
  expect(res.status).toBe(201);
  expect(res.body.title).toBe('Learn Rust');
});
```

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use vitesse::serde_json::Value;
    use vitesse::test::TestClient;

    #[tokio::test]
    async fn creates_a_todo() {
        let client = TestClient::new(app());

        let res = client.post("/todos").json(&json!({ "title": "Learn Rust" })).await;

        assert_eq!(res.status(), 201);
        let body: Value = res.json();
        assert_eq!(body["title"], "Learn Rust");
    }
}
```

`TestClient` calls the application in memory, without opening a port. Everything about it is in [Testing](testing.md).

## Key differences

### `async move` and closures

```rust
app.get("/", |_| async { "Hello!" });                  // request not used
app.get("/hello/:name", |req: Request| async move {    // request used
    format!("Hello {}!", req.param("name").unwrap_or("stranger"))
});
```

A closure receives the request and returns an `async` block. When the block uses `req`, write `async move` so that it takes ownership of the request, and annotate the type (`req: Request`) so the compiler knows it. When a handler grows, a named `async fn` is often easier to read than a closure.

### Typed JSON with serde

Derive `Deserialize` for what you receive and `Serialize` for what you send. Optional fields are `Option<T>`; unknown fields are ignored by default. When the structure is not known in advance, use `serde_json::Value`, the equivalent of a plain JavaScript object (`vitesse::serde_json` is re-exported).

### Errors with `?`

`?` replaces both `try/catch` and `next(err)`. The functions that can fail in Vitesse (`req.json()`, `req.param_as()`, `req.query_as()`…) already return a `vitesse::Error` with the right status. For a closure that uses `?`, tell the compiler the error type with `Ok::<_, Error>(value)` as the last expression, or use a named function returning `vitesse::Result<T>`.

### Checked at compile time, and at startup

A handler that returns something that is not a response, a typo in a method name, a missing `.await`: the compiler refuses to build. Routes are checked as soon as they are added: an invalid pattern or the same route defined twice makes the program panic at startup, with the faulty line.

### Shared state must be thread-safe

Requests are handled in parallel on several threads, so there are no mutable global variables: register your data with `app.state(...)` and protect what changes with an atomic, a `Mutex` or an `RwLock`. Reading the state costs nothing (`req.state::<T>()` returns a plain reference). See [State](state.md).

## Gotchas

- **Global middleware runs before routing, for every request.** Whether `app.middleware(...)` is called before or after your routes does not matter: all global middleware runs, in the order it was added, before routing, including for 404s. To target some routes only, use a [router](routers.md) (whose middleware covers its whole prefix, like `router.use`) or `handler.with(mw)`.
- **Routes are case-sensitive.** `/Users` does not match `/users` (Express ignores case by default). A trailing slash is ignored: `/users/` matches `/users`, like in Express.
- **A wrong method answers `405`, not `404`,** with an `Allow` header. `HEAD` uses the `GET` route and `OPTIONS` answers `204` automatically.
- **One handler per method and path.** Defining the same route twice panics at startup; the Express habit of chaining several handlers on one route with `next()` becomes middleware.
- **`req.json()` does not check the `Content-Type`.** `express.json()` ignores bodies that are not JSON; Vitesse parses whatever it receives. To require the header, check `req.is("json")` and return a `415`.
- **Query strings are flat.** `?user[name]=ada` does not become a nested object. For a repeated key such as `?tag=a&tag=b`, iterate over `req.query_pairs()`.
- **No optional or regex parameters,** and no partial parameters in a segment (`/:id?`, `/:id(\d+)`, `/flights/:from-:to`): declare separate routes, or parse the segment yourself.
- **Errors are JSON by default** (`{"error": "..."}`), and internal details never reach the client, not even in development. Use `app.on_error` for another format.
- **`req.set(value)` requires `Clone`.** Add `#[derive(Clone)]` to the types you attach to a request.
- **The body is read on demand, and only once from the network.** The result is cached: calling `req.text()` and then `req.bytes()` works.
- **Reloading on change.** `cargo run` recompiles what changed; for a `nodemon`-style loop, use a tool such as [cargo-watch](https://github.com/watchexec/cargo-watch) (`cargo watch -x run`). Use `cargo build --release` for production.

To go further, the [cheat sheet](cheatsheet.md) lists the whole API on one page.
