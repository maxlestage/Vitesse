# Routing

Routing picks the handler that answers a request, based on its method and path. Vitesse uses Express's vocabulary (`app.get`, `:id`, `app.all`), but its router is a tree: the order in which you declare routes doesn't matter, and a wrong method gets a proper `405`.

## Defining routes

Each HTTP method has its own helper. They exist on `App` and on [`Router`](routers.md):

| Vitesse | Express | Matches |
|---|---|---|
| `app.get(path, handler)` | `app.get()` | `GET`, and `HEAD` too (see below) |
| `app.post(path, handler)` | `app.post()` | `POST` |
| `app.put(path, handler)` | `app.put()` | `PUT` |
| `app.patch(path, handler)` | `app.patch()` | `PATCH` |
| `app.delete(path, handler)` | `app.delete()` | `DELETE` |
| `app.head(path, handler)` | `app.head()` | `HEAD` |
| `app.options(path, handler)` | `app.options()` | `OPTIONS` |
| `app.all(path, handler)` | `app.all()` | every method |
| `app.route(method, path, handler)` | `app[method]()` | the method you pass, including non-standard ones |

```rust
app.get("/articles", |_| async { "list articles" });
app.post("/articles", |_| async { (201, "article created") });
app.put("/articles/:id", |_| async { "article replaced" });
app.patch("/articles/:id", |_| async { "article updated" });
app.delete("/articles/:id", |_| async { StatusCode::NO_CONTENT });
app.all("/ping", |_| async { "pong" });

// A non-standard method
let purge = Method::from_bytes(b"PURGE").unwrap();
app.route(purge, "/cache", |_| async { "cache purged" });
```

> [!WARNING]
> `app.route` is not Express's `app.route(path)`. In Vitesse it registers a single route for the `Method` you pass. To put several methods on the same path, chain the helpers instead (see [Chaining](#chaining)).

### Handlers

A handler is an `async` function or closure that takes a `Request` and returns anything that implements `IntoResponse`: text, `Json(...)`, a `(status, body)` tuple, a `Result`, and so on (see [Sending responses](responses.md)).

```rust
// A closure that ignores the request
app.get("/", |_| async { "Home" });

// A closure that uses it: give its type and add `async move`
app.get("/hello/:name", |req: Request| async move {
    format!("Hello, {}!", req.param("name").unwrap_or("stranger"))
});

// A named function
async fn list_users(_req: Request) -> Json<Vec<&'static str>> {
    Json(vec!["ada", "grace"])
}
app.get("/users", list_users);
```

### Chaining

Every routing method returns `&mut Self`, so you can chain calls. It reads a bit like `app.route('/users').get(...).post(...)` in Express:

```rust
app.get("/users", list_users)
    .post("/users", create_user)
    .get("/users/:id", show_user)
    .delete("/users/:id", delete_user);
```

Chain on your variable, after `let mut app = App::new();`. Chaining straight onto `App::new()` would leave you with a reference to a temporary value.

## Path parameters

A segment that starts with `:` captures that part of the path. `req.param(name)` reads it and returns an `Option<&str>`:

```rust
app.get("/users/:id/posts/:post", |req: Request| async move {
    let user = req.param("id").unwrap();
    let post = req.param("post").unwrap();
    format!("post {post} of user {user}")
});
```

`GET /users/42/posts/7` answers `post 7 of user 42`. The `unwrap()` can't fail here because the route only matches when both parameters are present.

- `req.params()` iterates over every `(name, value)` pair, in order.
- Values are percent-decoded: `/users/Fran%C3%A7ois` gives `François`. A `+` stays a `+`, because it only means a space in query strings. `req.path()` still returns the raw path.
- A parameter takes a whole segment. It never matches an empty segment or anything containing a `/`.
- Names can only contain letters, digits and `_`. Partial segments such as `/flights/:from-:to` and regular expressions aren't supported. Capture the whole segment and split it yourself.

## Typed parameters

`req.param_as::<T>(name)` converts a parameter into any type that implements `FromStr`: integers, `bool`, `f64`, `IpAddr`, your own types and so on. If the parameter is missing or doesn't convert, it returns a `400 Bad Request` error, so a `?` is all you need:

```rust
async fn show_user(req: Request) -> vitesse::Result<String> {
    let id: u64 = req.param_as("id")?;
    Ok(format!("user #{id}"))
}

app.get("/users/:id", show_user);
```

```text
GET /users/42   → 200 user #42
GET /users/abc  → 400 {"error":"invalid parameter 'id': 'abc'"}
GET /users/-1   → 400 (a u64 can't be negative)
```

For your own types, implement `FromStr`:

```rust
use std::str::FromStr;

enum Format {
    Json,
    Csv,
}

impl FromStr for Format {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "json" => Ok(Format::Json),
            "csv" => Ok(Format::Csv),
            _ => Err(()),
        }
    }
}

app.get("/export/:format", |req: Request| async move {
    let message = match req.param_as::<Format>("format")? {
        Format::Json => "exporting as JSON",
        Format::Csv => "exporting as CSV",
    };
    Ok::<_, Error>(message)
});
```

`GET /export/csv` answers `exporting as CSV`, and `GET /export/xml` gets a `400`.

## Wildcards

A `*name` segment captures the rest of the path, slashes included. Express 5 uses the same syntax:

```rust
app.get("/files/*path", |req: Request| async move {
    format!("you asked for {}", req.param("path").unwrap())
});
```

| Request | `req.param("path")` |
|---|---|
| `/files/report.pdf` | `"report.pdf"` |
| `/files/2024/q1/report.pdf` | `"2024/q1/report.pdf"` |
| `/files` or `/files/` | `""` (empty) |

- A wildcard must be the last segment. `/files/*path/edit` is rejected at startup.
- An anonymous wildcard `*` works too. Its value is stored under the name `"*"`, so you read it with `req.param("*")`.
- `/*` on its own catches every path that no more specific route matches.

## How routes are matched

### The most specific route wins

When several routes could match, Vitesse compares them segment by segment: **static > parameter > wildcard**. The order in which you declare them doesn't matter. In Express, the first registered route wins.

```rust
app.get("/users/new", |_| async { "new-user form" });
app.get("/users/:id", |_| async { "one user" });
app.get("/users/*rest", |_| async { "anything else under /users" });
```

| Request | Route used |
|---|---|
| `/users/new` | `/users/new` (static beats parameter) |
| `/users/42` | `/users/:id` |
| `/users/42/settings` | `/users/*rest` |
| `/users` | `/users/*rest` (empty wildcard) |

If a branch leads nowhere, the router backs up and tries the next option. With `/a/:x/c` and `/a/b/d`, a request to `/a/b/c` matches `/a/:x/c` with `x = "b"`.

### Trailing slashes, case and query string

- A trailing slash is ignored: `/users/` matches `/users`. Express does the same by default.
- Paths are **case-sensitive**: `/Users` doesn't match `/users`. Express ignores case by default.
- The query string plays no part in matching: `/search?q=rust` matches `/search`.

### Invalid and duplicate routes

Vitesse catches mistakes in route definitions when the app starts, not when a request comes in. `app.get(...)` panics with an explicit message if:

- the path doesn't start with `/`;
- it has an empty segment (`/a//b`);
- a parameter name is invalid;
- a wildcard isn't the last segment;
- the same method is registered twice on the same path.

Two patterns that differ only in their parameter names are the same route, so `/users/:id` and `/users/:name` collide:

```text
thread 'main' panicked at src/main.rs:12:9:
duplicate route: GET /users/:name is already defined
```

## HEAD, OPTIONS and 405

Vitesse takes care of standard HTTP behaviour:

- **HEAD**: a `HEAD` request uses the `GET` route and gets the same headers with no body, unless you register `app.head(...)`.
- **405 Method Not Allowed**: when the path exists but not for this method, the response is a `405` with an `Allow` header listing the methods that are available. Express answers `404` in this case.
- **OPTIONS**: without an explicit `app.options(...)` route, an `OPTIONS` request gets `204 No Content` with the same `Allow` header.

```rust
app.get("/items", list_items);
app.post("/items", create_item);
```

```http
DELETE /items HTTP/1.1

HTTP/1.1 405 Method Not Allowed
content-type: application/json
allow: GET, HEAD, POST, OPTIONS

{"error":"Method Not Allowed"}
```

On the same path, `OPTIONS /items` gets a `204 No Content` with `allow: GET, HEAD, POST, OPTIONS`.

`app.all(...)` accepts every method on its path, so it never produces a `405`. If the same path also has a route for a specific method, that route wins over `all`.

> [!TIP]
> Browsers send their CORS preflight requests as `OPTIONS`. To answer them, use `middleware::cors()`, which handles them before routing. See [Middleware](middleware.md).

## Fallback: customising the 404

When no route matches the path, Vitesse answers `404` with `{"error":"Cannot GET /nope"}`, the same message as Express. `app.fallback` replaces that handler. It plays the role of the last `app.use((req, res) => ...)` in an Express app:

```rust
app.fallback(|req: Request| async move {
    (404, format!("Nothing here: {}", req.path()))
});
```

A common use is a single-page app: send `index.html` for every unknown path and let the front-end router take over.

```rust
app.fallback(|_| async { res::file("dist/index.html").await });
```

- The fallback runs after global middleware, like any route.
- It isn't called for a `405`, since the path does exist, just not for that method.
- To change the format of *every* error response at once (404, 405, 400, 500 and so on), use `app.on_error` instead. See [Error handling](errors.md).

## Organising routes

As your app grows, group related routes in a `Router` and mount it under a prefix, as you would with `express.Router()`:

```rust
let mut api = Router::new();
api.get("/users", list_users).post("/users", create_user);

app.mount("/api", api); // GET /api/users, POST /api/users
```

Routers can have their own middleware and can be nested, see [Routers](routers.md). For middleware on a single route (`handler.with(auth)`), see [Middleware](middleware.md).
