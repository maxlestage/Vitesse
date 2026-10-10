# Your first app

In this tutorial you'll build a small JSON API one step at a time: a Hello World, a JSON route, a route with a parameter, a `POST` route that reads a JSON body, and request logging. It takes about ten minutes.

You'll need Rust installed first. See [Installation](installation.md).

## 1. Create the project

```sh
cargo new hello-vitesse
cd hello-vitesse
cargo add vitesse
cargo add serde --features derive
```

If Vitesse isn't on crates.io yet, use `cargo add vitesse --git https://github.com/maxlestage/Vitesse` instead (see [Installation](installation.md#add-vitesse)).

## 2. Hello World

Replace `src/main.rs` with:

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    println!("Vitesse listening on http://localhost:3000");
    app.run(3000)
}
```

Start it with `cargo run`, then in another terminal:

```sh
curl http://localhost:3000
```

```text
Hello World!
```

What each line does:

- `use vitesse::prelude::*;` imports the types you'll use all the time: `App`, `Request`, `Json`, `Error`, `StatusCode` and so on.
- `App::new()` is the equivalent of `express()`. `app` is declared `mut` because adding routes modifies it.
- `app.get(path, handler)` registers a route. The handler is an async closure. It receives the request (ignored here with `_`) and returns anything that can be turned into a response. A `&str` becomes a `200` with `Content-Type: text/plain`.
- `app.run(3000)` starts the server on port 3000, on all network interfaces and using every CPU core. It blocks until you press `Ctrl+C`. It returns a `std::io::Result<()>`, which is why `main` returns the same type. If the port is taken, the program exits with the error.

In Express, the same thing would be:

```js
const app = express();
app.get('/', (req, res) => res.send('Hello World!'));
app.listen(3000);
```

Stop the server with `Ctrl+C` before each of the next steps, then start it again with `cargo run`.

## 3. A JSON route

Add this route below the first one:

```rust
app.get("/api/status", |_| async {
    json!({ "status": "ok", "framework": "Vitesse" })
});
```

`json!` builds a JSON value with a syntax close to JavaScript. Returning it sends a `200` with `Content-Type: application/json`.

```sh
curl http://localhost:3000/api/status
```

```json
{"framework":"Vitesse","status":"ok"}
```

> [!NOTE]
> The keys come back sorted alphabetically because `json!` stores objects in a sorted map. When you serialise your own struct with `Json(...)` (step 5), fields keep their declaration order.

## 4. A route with a parameter

```rust
app.get("/hello/:name", |req: Request| async move {
    let name = req.param("name").unwrap_or("stranger");
    format!("Hello, {name}!")
});
```

- `:name` is a path parameter, as in Express. `req.param("name")` returns an `Option<&str>`, which is `Some("Ada")` for `/hello/Ada`. The value is already decoded: `/hello/Fran%C3%A7ois` gives `François`.
- When the handler uses the request, write `|req: Request| async move { ... }`. `move` moves the request into the async block so the block can use it. When it doesn't, `|_| async { ... }` is enough.
- `format!` builds a `String`, which is also a valid `text/plain` response.

```sh
curl http://localhost:3000/hello/Ada
```

```text
Hello, Ada!
```

Need a number? `req.param_as::<u64>("id")?` converts the parameter and answers `400 Bad Request` by itself if it isn't a number. See [Routing](routing.md#typed-parameters).

## 5. Read a JSON body

Now add a `POST /users` route. It receives `{"name": "Ada"}` and answers `201 Created` with the new user. Above `main`, add the types and the handler:

```rust
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

#[derive(Serialize)]
struct User {
    id: u64,
    name: String,
}

async fn create_user(req: Request) -> vitesse::Result<(StatusCode, Json<User>)> {
    let input: NewUser = req.json().await?;
    if input.name.trim().is_empty() {
        return Err(Error::bad_request("name must not be empty"));
    }
    let user = User { id: 1, name: input.name };
    Ok((StatusCode::CREATED, Json(user)))
}
```

Then register it in `main`:

```rust
app.post("/users", create_user);
```

- A handler can be a named `async fn` that takes a `Request`. That reads better once a handler is more than a few lines long.
- `req.json().await` reads the body and deserialises it into the type you asked for (`NewUser`). You don't have to install anything like `express.json()`: the body is parsed when you ask for it. If the JSON is invalid or a field is missing, `?` returns a `400 Bad Request`. If the body is larger than the limit (1 MiB by default), it returns `413 Payload Too Large`.
- `vitesse::Result<T>` is short for `Result<T, vitesse::Error>`. `Error::bad_request(...)` creates a `400` error, and its message is sent to the client as `{"error": "..."}`.
- `(StatusCode::CREATED, Json(user))` sends the struct as JSON with the `201` status. It's the equivalent of `res.status(201).json(user)`.

Try a valid request, an empty name and a missing field:

```sh
curl -i -X POST http://localhost:3000/users \
  -H 'content-type: application/json' -d '{"name":"Ada"}'
```

```http
HTTP/1.1 201 Created
content-type: application/json
date: Sat, 10 Oct 2026 09:00:00 GMT
content-length: 21

{"id":1,"name":"Ada"}
```

```sh
curl -X POST http://localhost:3000/users \
  -H 'content-type: application/json' -d '{"name":""}'
# {"error":"name must not be empty"}

curl -X POST http://localhost:3000/users \
  -H 'content-type: application/json' -d '{"nom":"Ada"}'
# {"error":"invalid JSON: missing field `name` at line 1 column 13"}
```

> [!TIP]
> You can also write this handler as a closure. A closure has no declared return type, so you have to name the error type on the `Ok`, with `Ok::<_, Error>((StatusCode::CREATED, Json(user)))`. A named function avoids that.

## 6. Log every request

Add this line right after `App::new()`:

```rust
app.middleware(middleware::logger());
```

`app.middleware(...)` is Express's `app.use(fn)`. A global middleware runs for every request, including 404s. `middleware::logger()` works like `morgan('dev')` and prints one line per request:

```text
GET /hello/Ada 200 0.007 ms
POST /users 201 0.008 ms
```

Global middleware runs in the order you add it. You can write your own in a few lines, see [Middleware](middleware.md).

## 7. Read the port from the environment

Many hosting platforms (Heroku, Render, Railway and others) tell your app which port to listen on through the `PORT` environment variable. Replace the last two lines of `main` with:

```rust
let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
println!("Vitesse listening on http://localhost:{port}");
app.run(port)
```

It's the equivalent of `app.listen(process.env.PORT || 3000)`. `app.run` accepts a port number, a string such as `"3000"` or `"127.0.0.1:8080"`, a `SocketAddr` and more, so you can pass the `String` read from the environment as it is.

## The complete program

```rust
use serde::{Deserialize, Serialize};
use vitesse::prelude::*;

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

#[derive(Serialize)]
struct User {
    id: u64,
    name: String,
}

async fn create_user(req: Request) -> vitesse::Result<(StatusCode, Json<User>)> {
    let input: NewUser = req.json().await?;
    if input.name.trim().is_empty() {
        return Err(Error::bad_request("name must not be empty"));
    }
    let user = User { id: 1, name: input.name };
    Ok((StatusCode::CREATED, Json(user)))
}

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.middleware(middleware::logger());

    app.get("/", |_| async { "Hello World!" });

    app.get("/api/status", |_| async {
        json!({ "status": "ok", "framework": "Vitesse" })
    });

    app.get("/hello/:name", |req: Request| async move {
        let name = req.param("name").unwrap_or("stranger");
        format!("Hello, {name}!")
    });

    app.post("/users", create_user);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
    println!("Vitesse listening on http://localhost:{port}");
    app.run(port)
}
```

## Run it in release mode

```sh
cargo run --release
```

The first release build takes a little longer, but the server is much faster afterwards (also add the [recommended release profile](installation.md#recommended-release-profile)). To use another port:

```sh
PORT=8080 cargo run --release
```

Vitesse also handles unknown routes and wrong methods for you:

```sh
curl -i http://localhost:3000/nope
```

```http
HTTP/1.1 404 Not Found
content-type: application/json
date: Sat, 10 Oct 2026 09:00:00 GMT
content-length: 28

{"error":"Cannot GET /nope"}
```

```sh
curl -i -X DELETE http://localhost:3000/users
```

```http
HTTP/1.1 405 Method Not Allowed
content-type: application/json
allow: POST, OPTIONS
date: Sat, 10 Oct 2026 09:00:00 GMT
content-length: 30

{"error":"Method Not Allowed"}
```

## What next?

- [Routing](routing.md): wildcards, typed parameters, matching priority, 404 and 405.
- [Reading requests](requests.md): query strings, headers, cookies, forms and uploads.
- [Sending responses](responses.md): status codes, headers, cookies, redirects and files.
- [Shared state](state.md): replace the hard-coded `id: 1` with an in-memory store or a database pool.
- [Error handling](errors.md), [Testing](testing.md), then deploy with [Docker](docker.md).
