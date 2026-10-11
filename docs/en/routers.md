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

A `Router` has the same routing methods as `App`: `get`, `post`, `put`, `patch`, `delete`, `head`, `options`, `all`, `route(method, path, handler)`, as well as `ws`, `mount`, `static_dir` and `serve_dir`. Like on `App`, they return `&mut Router`, so you can chain them:

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

Router middleware also runs for the requests under the router's prefix that none of its routes answers: the `404` (the router's own [fallback](#router-fallback) if it has one, or else the app's), the `405 Method Not Allowed` and the automatic `OPTIONS` response (`204` with `Allow`). In the example above, `GET /admin/unknown` goes through `require_token` too: without a token, the visitor gets a `401` and doesn't even learn whether the page exists.

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
- A prefix can contain parameters: a router mounted at `/users/:user_id/posts` covers `/users/42/posts/...` whatever the value of `:user_id`. Constraints in the prefix are checked for these requests too: a router mounted at `/docs/:lang(en|fr|es)` doesn't cover `/docs/de/...`, whose `404` comes from the application, without the router's middleware or fallback.

## Router fallback

By default, an unknown path under a router's prefix gets the application's `404`: your `app.fallback`, or the default `{"error":"Cannot GET /nope"}`. `router.fallback(handler)` answers them instead. That lets an API keep JSON errors while the rest of the site shows an HTML page:

```rust
let mut api = Router::new();
api.get("/users", |_| async { Json(vec!["ada", "grace"]) });
api.fallback(|req: Request| async move {
    (404, Json(json!({ "error": format!("no API route for {}", req.path()) })))
});

let mut app = App::new();
app.mount("/api", api);
app.fallback(|_| async { (404, Html("<h1>Page not found</h1>")) });
```

| Request | Answer |
|---|---|
| `GET /api/users` | the route |
| `GET /api/nope` | the router's fallback: `404 {"error":"no API route for /api/nope"}` |
| `POST /api/users` | `405 Method Not Allowed`: the path exists, so no fallback is called |
| `GET /nope` | the app's fallback: the HTML page |

- The fallback runs after the router's middleware, like one of its routes: a router protected by authentication protects its fallback too, and the headers its middleware adds are on the fallback's responses.
- With nested routers, the deepest router that has a fallback wins: a fallback on the router mounted at `/api/admin` answers `/api/admin/nope`, the one on `/api` answers the rest of `/api`. A router without a fallback uses its parent's, and in the end the app's.
- Like `app.fallback`, it can answer anything, not just a `404`: a redirect, an HTML page, or the `index.html` of a single-page app that lives under the prefix.

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

Everything the [route syntax](routing.md#path-parameters) offers works in a prefix too: several parameters in a segment (`/api/v:version`), [constraints](routing.md#constraints-with-regular-expressions) and [optional parameters](routing.md#optional-parameters). Here, one router serves the documentation in three languages, with a home page per language:

```rust
let mut docs = Router::new();
docs.get("/:page?", |req: Request| async move {
    let lang = req.param("lang").unwrap_or("en");
    let page = req.param("page").unwrap_or("index");
    format!("page {page} in {lang}")
});

app.mount("/docs/:lang(en|fr|es)", docs);
```

| Request | Answer |
|---|---|
| `GET /docs/fr` | `page index in fr` |
| `GET /docs/es/routing` | `page routing in es` |
| `GET /docs/de/routing` | `404`: `de` doesn't satisfy the constraint |

`req.route()` gives the full pattern, prefix included: `/docs/:lang(en|fr|es)/:page?`. Optional parameters must stay at the end of the full path, prefix and route included: with a prefix such as `/:lang?`, any route other than the router's `/` makes `mount` panic. Put the optional parameter in the router's routes instead, as above.

## WebSocket routes

`router.ws(path, handler)` declares a [WebSocket](websocket.md) route, exactly like `app.ws`. The router's prefix applies, and its middleware runs on the handshake request, before the connection is accepted: a middleware that answers `401` refuses the connection.

```rust
let mut live = Router::new();
live.middleware(require_token); // also checks the WebSocket handshake
live.ws("/feed", |_req, mut socket| async move {
    let _ = socket.send("welcome to the live feed").await;
});

app.mount("/live", live); // ws://example.com/live/feed
```

## Named routes in routers

A route declared in a router can get a [name](routing.md#named-routes-and-url-generation) with `.name(...)`, exactly as on `App`. The name belongs to the whole application, and the path it builds includes the mount prefix:

```rust
let mut api = Router::new();
api.get(r"/items/:id(\d+)", show_item).name("item");
api.post("/items", |req: Request| async move {
    // The prefix's parameters come first: ":version", then ":id".
    let link = req.path_for("item", &[&"v1", &7])?; // "/api/v1/items/7"
    Ok::<_, Error>((201, Json(json!({ "link": link }))))
});

app.mount("/api/:version", api);
```

- Any handler can use the name, inside the router or elsewhere in the app: `req.path_for("item", …)` works from `app.get("/", …)` too.
- The values follow the order of the parameters in the full path: those of the prefix first, then those of the route.
- Names must be unique across the application: two routers that both name a route `"show"` make the second `mount` panic at startup. Prefix them (`"users.show"`, `"posts.show"`).
- `name` follows a route: calling it right after `mount` panics, since `mount` adds routes but isn't one.

## How mounting works

`mount` copies the router's routes into the application's routing tree, once, at startup. A few consequences:

- **Zero cost per request**: a mounted route is exactly as fast as a route declared directly on `App`.
- **The router is consumed** by `mount`. To mount the same routes twice (`/api/v1` and `/api/latest`, say), build the router in a function and call it twice.
- **Conflicts are caught at startup**: if the same method and path are defined twice, even across routers, the program panics as soon as it starts, with a message such as `duplicate route: GET /api/x is already defined`.
- **Route priority** is global: a static segment wins over a constrained or composite one, which wins over a plain parameter, which wins over a wildcard (`/users/new`, then `/users/:id(\d+)`, then `/users/:name`), whatever router they come from. See [How routes are matched](routing.md#how-routes-are-matched).
- **Route names** are global too: see [Named routes in routers](#named-routes-in-routers).
- The error handler (`app.on_error`) and the state (`app.state`) belong to the application, not to routers. A router can have its own [fallback](#router-fallback), though, which answers before `app.fallback`.

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
