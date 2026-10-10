# Routers

A `Router` groups related routes, with their own middleware, and is mounted on the application under a prefix: it's the equivalent of `express.Router()`. Routers keep a growing project organised, one module per resource.

## Creating and mounting a router

```js
const users = express.Router();
users.get('/', listUsers);
users.get('/:id', showUser);
app.use('/users', users);
```

```rust
use vitesse::prelude::*;

let mut users = Router::new();
users.get("/", |_| async { "user list" });
users.get("/:id", |req: Request| async move {
    format!("user {}", req.param("id").unwrap_or("?"))
});

let mut app = App::new();
app.mount("/users", users); // GET /users, GET /users/:id
```

A `Router` has the same routing methods as `App`: `get`, `post`, `put`, `patch`, `delete`, `head`, `options`, `all`, `route(method, path, handler)`, as well as `mount`, `static_dir` and `serve_dir`. Like on `App`, they return `&mut Router`, so you can chain them:

```rust
let mut users = Router::new();
users
    .get("/", list_users)
    .post("/", create_user)
    .get("/:id", show_user)
    .delete("/:id", delete_user);
```

Paths are joined the way you'd expect: the router's `/` becomes `/users`, `/:id` becomes `/users/:id`, and `/users/` is equivalent to `/users`.

> [!WARNING]
> Declare the router with `let mut users = Router::new();`, then call its methods in separate statements. `let users = Router::new().get(...);` doesn't compile: the methods return a `&mut Router` borrowed from a temporary value, not the `Router` itself.

## Router middleware

`router.middleware(...)` adds a middleware to **all the routes of that router** and, as with `router.use()` in Express, to every other request under its prefix:

```rust
async fn require_token(req: Request, next: Next) -> Response {
    match req.header("authorization") {
        Some("Bearer secret") => next.run(req).await,
        _ => Error::unauthorized("missing token").into_response(),
    }
}

let mut admin = Router::new();
admin.middleware(require_token);
admin.get("/stats", |_| async { "secret stats" });

app.mount("/admin", admin);                      // protected
app.get("/", |_| async { "public home page" });  // not affected
```

For a request, the order is: global middleware (`app.middleware`), then router middleware in the order it was added, then route middleware (`.with`), then the handler. A router's middleware applies to all its routes, whether it was added before or after them, as long as it's added before `mount`.

### Requests that no route answers

Router middleware also runs for the requests under the router's prefix that none of its routes answers: the `404` (or your `app.fallback`), the `405 Method Not Allowed` and the automatic `OPTIONS` response (`204` with `Allow`). In the example above, `GET /admin/unknown` goes through `require_token` too: without a token, the visitor gets a `401` and doesn't even learn whether the page exists.

This is what makes a logger or CORS on a router work as you would expect:

```rust
let mut api = Router::new();
api.middleware(middleware::logger()); // also logs the 404s under /api
api.middleware(middleware::cors());   // also answers the preflights of /api/users
api.post("/users", |_| async { (201, "created") });

app.mount("/api", api);
```

Before a cross-origin `POST /api/users`, the browser sends an `OPTIONS /api/users` preflight: `cors()` answers it with a `204` and the CORS headers. A `GET /api/nope` is logged, then gets its `404`.

A few details:

- The prefix is matched segment by segment: a router mounted at `/api` covers `/api` and `/api/...`, but not `/apix`.
- Global middleware (`app.middleware`) still runs first, for every request. With nested routers, the outer router's middleware then runs first, followed by the inner router's.
- If a router middleware rewrites the path of such a request with `req.set_uri(...)`, routing runs again on the new path.
- Only prefixes made of fixed segments are covered this way: under a prefix that contains a parameter (`/users/:user_id/posts`), router middleware only runs for the router's own routes.

## Nesting routers

A router can mount other routers. The middleware of the parent router also wraps the routes of its children:

```rust
let mut users = Router::new();
users.get("/", |_| async { "users" });

let mut posts = Router::new();
posts.get("/", |_| async { "posts" });

let mut api = Router::new();
api.middleware(require_token); // also protects /users and /posts
api.mount("/users", users);
api.mount("/posts", posts);

app.mount("/api/v1", api); // GET /api/v1/users, GET /api/v1/posts
```

## Parameters in the prefix

A mount prefix can contain parameters, which handlers read like any other:

```rust
let mut posts = Router::new();
posts.get("/", |req: Request| async move {
    format!("posts by user {}", req.param("user_id").unwrap_or("?"))
});

app.mount("/users/:user_id/posts", posts); // GET /users/42/posts
```

## How mounting works

`mount` copies the router's routes into the application's routing tree, once, at startup. A few consequences:

- **Zero cost per request**: a mounted route is exactly as fast as a route declared directly on `App`.
- **The router is consumed** by `mount`. To mount the same routes twice (`/api/v1` and `/api/latest`, say), build the router in a function and call it twice.
- **Conflicts are caught at startup**: if the same method and path are defined twice, even across routers, the program panics as soon as it starts, with a message such as `duplicate route: GET /api/x is already defined`.
- **Route priority** is global: a static segment wins over a parameter, which wins over a wildcard (`/users/new` before `/users/:id`), whatever router they come from.
- The fallback (`app.fallback`), the error handler (`app.on_error`) and the state (`app.state`) belong to the application, not to routers.

## Organising a project

Routers make it easy to split an application into modules, one per resource. A typical layout:

```text
my-app/
├── Cargo.toml
└── src/
    ├── main.rs        // builds the App and starts the server
    └── routes/
        ├── mod.rs     // assembles the API
        ├── users.rs   // /api/users
        └── posts.rs   // /api/users/:user_id/posts
```

Each module exposes a function that returns its `Router` (`posts.rs` follows the same pattern as `users.rs`):

```rust
// src/routes/users.rs
use vitesse::prelude::*;

pub fn router() -> Router {
    let mut users = Router::new();
    users.get("/", list).post("/", create).get("/:id", show);
    users
}

async fn list(_req: Request) -> Json<Vec<&'static str>> {
    Json(vec!["ada", "grace"])
}

async fn create(_req: Request) -> (StatusCode, &'static str) {
    (StatusCode::CREATED, "created")
}

async fn show(req: Request) -> vitesse::Result<String> {
    let id: u32 = req.param_as("id")?;
    Ok(format!("user {id}"))
}
```

```rust
// src/routes/mod.rs
use vitesse::prelude::*;

pub mod posts;
pub mod users;

pub fn api() -> Router {
    let mut api = Router::new();
    api.mount("/users", users::router());
    api.mount("/users/:user_id/posts", posts::router());
    api
}
```

```rust
// src/main.rs
use vitesse::prelude::*;

mod routes;

fn build_app() -> App {
    let mut app = App::new();
    app.middleware(middleware::logger());
    app.mount("/api", routes::api());
    app
}

fn main() -> std::io::Result<()> {
    build_app().run(3000)
}
```

Keeping a `build_app()` function separate from `main` lets your tests build the exact same application and call it without a network: see [Testing](testing.md).
