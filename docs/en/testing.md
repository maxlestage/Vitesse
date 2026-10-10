# Testing

Vitesse ships with an in-memory test client, `vitesse::test::TestClient`, that sends requests straight to your application without opening a port. It plays the role of [supertest](https://github.com/ladjs/supertest) in the Express world: tests are fast, run in parallel and need no running server.

## Setup

`TestClient` is part of Vitesse itself: you only need an async runtime for your tests. Add tokio as a dev-dependency (skip this step if tokio is already in your `[dependencies]` with the `macros` and `rt` features):

```toml
[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt"] }
```

Each test is an `async fn` marked with `#[tokio::test]`, which starts a small runtime just for that test.

## Make your app testable

Tests must build the same application as `main`, without starting the server. The simplest way is a function that returns the `App`, in `src/lib.rs`:

```rust
// src/lib.rs
use vitesse::prelude::*;

pub fn app() -> App {
    let mut app = App::new();
    app.get("/hello/:name", |req: Request| async move {
        format!("Hello {}!", req.param("name").unwrap_or("stranger"))
    });
    app
}
```

```rust
// src/main.rs
fn main() -> std::io::Result<()> {
    my_api::app().run(3000) // `my_api` is the name of your package
}
```

Integration tests in the `tests/` folder can now call `my_api::app()`.

> [!TIP]
> It is the same pattern as `module.exports = app` in `app.js` with `app.listen()` in `server.js`: defining the app and starting the server are two separate steps.

## Your first test

```rust
// tests/hello.rs
use vitesse::test::TestClient;

#[tokio::test]
async fn says_hello() {
    let client = TestClient::new(my_api::app());

    let res = client.get("/hello/Ada").await;

    assert_eq!(res.status(), 200);
    assert_eq!(res.header("content-type"), Some("text/plain; charset=utf-8"));
    assert_eq!(res.text(), "Hello Ada!");
}
```

Run it with `cargo test`. `TestClient::new` takes the `App` and freezes it, just as `app.run` would. Each request then goes through everything a real request goes through: global middleware, routing, route middleware, the handler, `app.on_error`, `app.body_limit`, and panics turned into `500` responses.

## Building requests

Start a request from the client:

| Method | Request |
|---|---|
| `client.get(uri)` | `GET` |
| `client.post(uri)` | `POST` |
| `client.put(uri)` | `PUT` |
| `client.patch(uri)` | `PATCH` |
| `client.delete(uri)` | `DELETE` |
| `client.request(Method::HEAD, uri)` | any other method (`HEAD`, `OPTIONS`…) |

Then chain what you need, and send it with `.await`:

| Method | Effect |
|---|---|
| `.header(name, value)` | Adds a header (panics if the name or value is invalid) |
| `.body(data)` | Raw body: `&'static str`, `String`, `Vec<u8>`, `Bytes`… |
| `.json(&value)` | Serialises `value` with serde and sets `content-type: application/json` |
| `.form(&value)` | URL-encoded form, with `content-type: application/x-www-form-urlencoded` |
| `.await` or `.send().await` | Sends the request and returns a `TestResponse` |

```rust
let res = client
    .post("/items")
    .header("authorization", "Bearer secret")
    .json(&json!({ "name": "Ada" }))
    .await;

// The query string goes in the URI, already encoded.
let res = client.get("/search?q=caf%C3%A9+cr%C3%A8me").await;

let res = client.post("/login").form(&[("user", "ada"), ("remember", "true")]).await;
let res = client.request(Method::OPTIONS, "/items").await;
```

> [!NOTE]
> A test request is lazy: nothing is sent until you `.await` it. If you forget the `.await`, the compiler warns you.

## Reading the response

A `TestResponse` already holds its whole body in memory:

| Method | Returns |
|---|---|
| `res.status()` | The `StatusCode`, which compares with plain numbers: `assert_eq!(res.status(), 404)` |
| `res.header(name)` | One header as `Option<&str>` |
| `res.headers()` | All headers (`&HeaderMap`) |
| `res.text()` | The body as a `String` |
| `res.bytes()` | The raw body (`&Bytes`) |
| `res.json::<T>()` | The body deserialised from JSON (panics, showing the body, if it is not valid) |

```rust
#[derive(serde::Deserialize, Debug, PartialEq)]
struct User {
    id: u64,
    name: String,
}

let user: User = res.json();                        // typed
let body: vitesse::serde_json::Value = res.json();  // untyped
assert_eq!(body["name"], "Ada");
```

> [!NOTE]
> No socket is involved, so the headers that the HTTP engine adds while writing to the network (`content-length`, `date`, `connection`, `transfer-encoding`) are not in a `TestResponse`. To check them, test against a real server (see below).

## A complete example

The `hello` app from above, extended with a small user API backed by shared state, and the tests that go with it:

```rust
// src/lib.rs
use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use vitesse::prelude::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
}

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

/// Our "database": an in-memory map shared by every request.
#[derive(Default)]
struct Users(Mutex<HashMap<u64, User>>);

pub fn app() -> App {
    let mut app = App::new();
    app.state(Users::default());

    app.get("/hello/:name", |req: Request| async move {
        format!("Hello {}!", req.param("name").unwrap_or("stranger"))
    });

    app.post("/users", |req: Request| async move {
        let new: NewUser = req.json().await?;
        if new.name.trim().is_empty() {
            return Err(Error::unprocessable("name is required"));
        }
        let mut users = req.state::<Users>().0.lock().unwrap();
        let user = User { id: users.len() as u64 + 1, name: new.name };
        users.insert(user.id, user.clone());
        Ok((201, Json(user)))
    });

    app.get("/users/:id", |req: Request| async move {
        let id: u64 = req.param_as("id")?;
        let users = req.state::<Users>().0.lock().unwrap();
        users
            .get(&id)
            .cloned()
            .map(Json)
            .ok_or_else(|| Error::not_found("user not found"))
    });

    app
}
```

```rust
// tests/api.rs
use my_api::{User, app};
use vitesse::json;
use vitesse::serde_json::Value;
use vitesse::test::TestClient;

#[tokio::test]
async fn creates_then_reads_a_user() {
    let client = TestClient::new(app());

    let res = client.post("/users").json(&json!({ "name": "Ada" })).await;
    assert_eq!(res.status(), 201);
    let created: User = res.json();
    assert_eq!(created.name, "Ada");

    let res = client.get(&format!("/users/{}", created.id)).await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.json::<User>(), created);
}

#[tokio::test]
async fn rejects_bad_input() {
    let client = TestClient::new(app());

    // Not JSON at all: 400, produced by `req.json()`.
    let res = client.post("/users").body("{oops").await;
    assert_eq!(res.status(), 400);

    // Valid JSON, but our own validation fails: 422 with our message.
    let res = client.post("/users").json(&json!({ "name": "" })).await;
    assert_eq!(res.status(), 422);
    let body: Value = res.json();
    assert_eq!(body["error"], "name is required");

    // Not a number: 400. Unknown id: 404.
    assert_eq!(client.get("/users/abc").await.status(), 400);
    assert_eq!(client.get("/users/999").await.status(), 404);
}
```

Every call to `app()` creates a fresh state, so each test starts from an empty "database" and tests never interfere with each other, even when they run in parallel.

## Testing errors

Errors are ordinary responses: check the status and the `{"error": "..."}` body. Panics and internal errors are worth a test too, to make sure nothing leaks to the client:

```rust
use vitesse::prelude::*;
use vitesse::test::TestClient;

async fn boom(_req: Request) -> &'static str {
    panic!("something went wrong")
}

async fn parse(_req: Request) -> vitesse::Result<String> {
    let n: u32 = "not a number".parse()?; // a standard error: becomes a 500
    Ok(n.to_string())
}

#[tokio::test]
async fn errors_are_turned_into_responses() {
    let mut app = App::new();
    app.get("/boom", boom);
    app.get("/parse", parse);
    let client = TestClient::new(app);

    // A panic crashes nothing: it becomes a 500.
    assert_eq!(client.get("/boom").await.status(), 500);

    // Internal details are never sent to the client.
    let res = client.get("/parse").await;
    assert_eq!(res.status(), 500);
    assert_eq!(res.text(), r#"{"error":"Internal Server Error"}"#);

    // The default 404.
    assert_eq!(client.get("/nope").await.text(), r#"{"error":"Cannot GET /nope"}"#);
}
```

A custom `app.on_error` handler can be checked the same way:

```rust
#[tokio::test]
async fn custom_error_pages() {
    let mut app = App::new();
    app.on_error(|err: Error| {
        res::status(err.status()).html(format!("<h1>{}</h1>", err.message()))
    });
    let client = TestClient::new(app);

    let res = client.get("/nope").await;
    assert_eq!(res.status(), 404);
    assert_eq!(res.header("content-type"), Some("text/html; charset=utf-8"));
    assert_eq!(res.text(), "<h1>Cannot GET /nope</h1>");
}
```

> [!TIP]
> Assert on statuses and on your own messages. The wording of the messages Vitesse generates itself (invalid JSON, invalid parameter…) may change between versions.

The panic message is still printed in the test output by Rust's panic hook: this is expected. See [Errors](errors.md) for everything about `Error` and `on_error`.

## Testing middleware

Wrap the middleware in a tiny app with a dummy route, then check both paths: the request that is let through and the one that is stopped.

```rust
use vitesse::prelude::*;
use vitesse::test::TestClient;

async fn require_key(req: Request, next: Next) -> Response {
    match req.header("x-api-key") {
        Some("secret") => next.run(req).await,
        _ => Error::unauthorized("missing API key").into_response(),
    }
}

#[tokio::test]
async fn require_key_blocks_or_lets_through() {
    let mut app = App::new();
    app.middleware(require_key);
    app.get("/", |_| async { "ok" });
    let client = TestClient::new(app);

    let res = client.get("/").await;
    assert_eq!(res.status(), 401);
    assert_eq!(res.text(), r#"{"error":"missing API key"}"#);

    let res = client.get("/").header("x-api-key", "secret").await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "ok");
}
```

To check what a middleware attaches to the request (with `req.set`), let the dummy handler send it back:

```rust
#[derive(Clone)]
struct CurrentUser(String);

async fn fake_auth(mut req: Request, next: Next) -> Response {
    req.set(CurrentUser("ada".into()));
    next.run(req).await
}

async fn me(req: Request) -> String {
    req.get::<CurrentUser>().map(|u| u.0.clone()).unwrap_or_default()
}

#[tokio::test]
async fn handler_sees_what_the_middleware_attached() {
    let mut app = App::new();
    app.get("/me", me.with(fake_auth));
    let client = TestClient::new(app);

    assert_eq!(client.get("/me").await.text(), "ada");
}
```

Built-in middleware is tested the same way, for example `middleware::cors()` by sending an `origin` header and checking `access-control-allow-origin`. More on writing middleware in [Middleware](middleware.md).

## Integration tests on a real port

`TestClient` skips the HTTP/1.1 engine. For what only happens on a real connection (`content-length`, keep-alive, pipelining, `chunked` bodies, `Expect: 100-continue`, header size limits), or to use a real HTTP client, start the server on a real port:

- `app.bind("127.0.0.1:0")` opens the socket; port `0` lets the OS pick a free port, so tests running in parallel never collide;
- `server.local_addr()` gives the address actually used;
- `tokio::spawn(server.run())` serves in the background for the duration of the test.

With [reqwest](https://docs.rs/reqwest) as a client:

```toml
[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt"] }
reqwest = { version = "0.12", default-features = false } # plain HTTP is enough here
```

```rust
// tests/server.rs
#[tokio::test]
async fn serves_over_tcp() {
    let server = my_api::app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr(); // e.g. 127.0.0.1:49213
    tokio::spawn(server.run());

    let res = reqwest::get(format!("http://{addr}/hello/Ada")).await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["content-length"], "10");
    assert_eq!(res.text().await.unwrap(), "Hello Ada!");
}
```

To test the shutdown too, use `with_graceful_shutdown` with a channel:

```rust
#[tokio::test]
async fn stops_gracefully() {
    let server = my_api::app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(server.with_graceful_shutdown(async {
        stopped.await.ok();
    }));

    let res = reqwest::get(format!("http://{addr}/hello/Ada")).await.unwrap();
    assert_eq!(res.status(), 200);

    stop.send(()).unwrap();
    handle.await.unwrap().unwrap(); // the server returned Ok(())
}
```

For low-level behaviour, write raw HTTP on a `TcpStream`, with no extra dependency:

```rust
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn answers_pipelined_requests_in_order() {
    let server = my_api::app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(
            b"GET /hello/A HTTP/1.1\r\nHost: test\r\n\r\n\
              GET /hello/B HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();

    let a = response.find("Hello A!").unwrap();
    let b = response.find("Hello B!").unwrap();
    assert!(a < b);
}
```

See [Server configuration](server.md) for `bind`, `Server` and graceful shutdown.

## Organising your tests

- **`tests/` folder**: one file per area (`tests/users.rs`, `tests/auth.rs`…), each going through `TestClient`. Each file is compiled as its own crate; share helpers in `tests/common/mod.rs`:

  ```rust
  // tests/common/mod.rs
  use vitesse::test::TestClient;

  pub fn client() -> TestClient {
      TestClient::new(my_api::app())
  }
  ```

  ```rust
  // tests/users.rs
  mod common;

  #[tokio::test]
  async fn unknown_user_is_404() {
      let client = common::client();
      assert_eq!(client.get("/users/42").await.status(), 404);
  }
  ```

- **Unit tests** in a `#[cfg(test)] mod tests` block next to your code, for private functions. `Request` has no public constructor, so test handlers through `TestClient`, and keep business logic in plain functions you can call directly.
- **Fresh state**: build a new app per test (`TestClient::new(app())`). The frozen app lives until the end of the test program, which is negligible. `TestClient` is `Copy`: pass it to helper functions freely.
- **Useful commands**: `cargo test` runs everything, `cargo test users` only the tests whose name contains `users`, and `cargo test -- --nocapture` shows `println!` output and the `middleware::logger()` lines.
