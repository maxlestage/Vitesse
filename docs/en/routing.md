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

> [!TIP]
> HTML forms can only send `GET` and `POST`. To reach a `PUT` or `DELETE` route from a form, change the method in a global middleware with `req.set_method(...)`, like Express's `method-override`: see [Method override](requests.md#method-override).

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
- A parameter never matches an empty value or anything containing a `/`.
- Names can only contain letters, digits and `_`.

The syntax is Express's (version 4), so a few more forms work: several parameters in one segment, constraints and optional parameters.

### Several parameters in a segment

A segment can mix text and parameters, as long as some text separates them:

```rust
app.get("/flights/:from-:to", |req: Request| async move {
    let from = req.param("from").unwrap();
    let to = req.param("to").unwrap();
    format!("from {from} to {to}")
});
app.get("/files/:name.:ext", show_file);
app.get("/api/v:version/status", api_status);
app.get("/img/:id.png", show_png);
```

| Request | Route | Parameters |
|---|---|---|
| `/flights/LAX-SFO` | `/flights/:from-:to` | `from = "LAX"`, `to = "SFO"` |
| `/files/report.pdf` | `/files/:name.:ext` | `name = "report"`, `ext = "pdf"` |
| `/files/archive.tar.gz` | `/files/:name.:ext` | `name = "archive"`, `ext = "tar.gz"` |
| `/api/v2/status` | `/api/v:version/status` | `version = "2"` |
| `/img/42.png` | `/img/:id.png` | `id = "42"` |
| `/files/README` or `/api/2/status` | none: `404` | |

- Each parameter takes at least one character, and all but the last take as few as possible, as in Express: that is why `archive.tar.gz` gives `archive` and `tar.gz`.
- Two parameters must be separated by text: `/:from:to` is rejected at startup, since nothing says where `from` ends.

### Constraints with regular expressions

A regular expression in parentheses, right after the name, restricts the values a parameter accepts. A request whose value doesn't match simply doesn't match the route: another route can take it, or it gets a `404`.

```rust
app.get(r"/users/:id(\d+)", |req: Request| async move {
    let id: u64 = req.param_as("id")?;
    Ok::<_, Error>(format!("user #{id}"))
});
app.get("/docs/:lang(en|fr|es)/:page", show_doc);
app.get(r"/posts/:year(\d{4})-:slug", show_post);
```

| Request | Answer |
|---|---|
| `/users/42` | `user #42` |
| `/users/ada` | `404` (or another route, see [below](#the-most-specific-route-wins)) |
| `/docs/fr/routing` | `show_doc`, with `lang = "fr"` |
| `/docs/de/routing` | `404` |
| `/posts/2024-hello-world` | `show_post`, with `year = "2024"` and `slug = "hello-world"` |

- In Rust, write the path as a raw string, `r"/users/:id(\d+)"`, so the backslashes stay as they are. In a regular string, double them: `"/users/:id(\\d+)"`.
- The expression must match the whole value: `:lang(en|fr)` accepts `fr`, not `fra`. There is no need for `^` and `$`.
- It can't contain a `/`, since a parameter never does.
- It is checked against the value as it appears in the URL, before percent-decoding: `%20` is three characters for it.
- Vitesse uses [regex-lite](https://crates.io/crates/regex-lite): the usual syntax (classes, `\d`, `\w`, `|`, repetitions, `(?i)` to ignore case) works, but `\d` and `\w` only cover ASCII, and Unicode classes such as `\p{L}` aren't available. The expressions are compiled once, at startup, and only run for the parameters that have a constraint.
- An invalid expression stops the program at startup: `invalid route: invalid regular expression…`.

A constraint chooses the route, and `req.param_as` still converts the value: here, a `u64` too large to fit still gets a `400`.

> [!NOTE]
> Express 5 removed regular expressions from paths, and actix-web writes them `{id:\d+}`. Vitesse keeps Express 4's syntax, `:id(\d+)`.

### Optional parameters

A `?` after a parameter makes it optional. One route then answers several paths:

```rust
app.get("/archive/:year?/:month?", |req: Request| async move {
    match (req.param("year"), req.param("month")) {
        (Some(year), Some(month)) => format!("archive of {month}/{year}"),
        (Some(year), None) => format!("archive of {year}"),
        _ => "the whole archive".to_owned(),
    }
});
```

| Request | `req.param("year")` | `req.param("month")` |
|---|---|---|
| `/archive` | `None` | `None` |
| `/archive/2024` | `Some("2024")` | `None` |
| `/archive/2024/05` | `Some("2024")` | `Some("05")` |

- Only the last segments of a path can be optional (`/:lang?/about` is rejected), and only a parameter that takes a whole segment (`/files/:name.:ext?` is rejected too).
- A constraint can be optional as well: `r"/users/:id(\d+)?"` matches `/users` and `/users/7`, not `/users/ada`.
- The route counts as one route per possible path: `/archive/:year?` together with a separate `/archive` route for the same method is a [duplicate](#invalid-and-duplicate-routes).

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

To answer `404` instead of `400` for `/users/abc`, or to let another route take it, add a [constraint](#constraints-with-regular-expressions) to the route: `r"/users/:id(\d+)"`.

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

### All parameters at once: `params_as`

`req.params_as::<T>()` deserializes every parameter of the route into a struct, converting numbers and booleans, like axum's `Path<T>`. An optional parameter that is absent fits an `Option`:

```rust
#[derive(serde::Deserialize)]
struct PostPath {
    user: String,
    post: u64,
    draft: Option<bool>,
}

app.get("/users/:user/posts/:post/:draft?", |req: Request| async move {
    let path: PostPath = req.params_as()?; // 400 if `post` isn't a number
    Ok::<_, Error>(format!("post #{} of {}", path.post, path.user))
});
```

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

When several routes could match, Vitesse compares them segment by segment, from the most specific kind of segment to the least:

1. a static segment (`/users/new`);
2. a segment with a constraint or with text around its parameters (`/users/:id(\d+)`, `/img/:id.png`);
3. a plain parameter (`/users/:name`);
4. a wildcard (`/users/*rest`).

The order in which you declare the routes doesn't matter, whereas in Express the first registered route wins. So a route with a constraint and a plain parameter can share the same position:

```rust
app.get("/users/new", |_| async { "new-user form" });
app.get(r"/users/:id(\d+)", |_| async { "one user, by id" });
app.get("/users/:name", |_| async { "one user, by name" });
app.get("/users/*rest", |_| async { "anything else under /users" });
```

| Request | Route used |
|---|---|
| `/users/new` | `/users/new` (static beats everything) |
| `/users/42` | `/users/:id(\d+)` (the constraint matches) |
| `/users/ada` | `/users/:name` (the constraint doesn't match) |
| `/users/42/settings` | `/users/*rest` |
| `/users` | `/users/*rest` (empty wildcard) |

If a branch leads nowhere, the router backs up and tries the next option. With `/a/:x/c` and `/a/b/d`, a request to `/a/b/c` matches `/a/:x/c` with `x = "b"`.

Declaration order only matters between two segments of the second kind at the same position, say `/:id(\d+)` and `/:slug([a-z-]+)`: they are tried in the order you declared them.

### Trailing slashes, case and query string

- A trailing slash is ignored: `/users/` matches `/users`. Express does the same by default.
- Paths are **case-sensitive** by default: `/Users` doesn't match `/users`. Express ignores case by default; to do the same, call `app.case_sensitive(false)`.
- The query string plays no part in matching: `/search?q=rust` matches `/search`.

```rust
let mut app = App::new();
app.case_sensitive(false);
app.get("/users/:id", show_user); // also answers /Users/Ada and /USERS/Ada
```

With `app.case_sensitive(false)`, the static segments and the text around parameters (`.png` in `/img/:id.png`) match whatever their case, in mount prefixes too. Parameters keep the case of the request (`req.param("id")` is `"Ada"`), and constraints are left alone: add `(?i)` to the expression if it should ignore case too. Only ASCII letters are concerned.

### Invalid and duplicate routes

Vitesse catches mistakes in route definitions when the app starts, not when a request comes in. `app.get(...)` panics with an explicit message if:

- the path doesn't start with `/`;
- it has an empty segment (`/a//b`);
- a parameter name is invalid, or two parameters follow each other with no text between them (`/:a:b`);
- a constraint is not a valid regular expression, or its parenthesis isn't closed;
- an optional parameter isn't at the end of the path, or doesn't take a whole segment;
- a wildcard isn't the last segment;
- the same method is registered twice on the same path.

Two patterns that differ only in their parameter names are the same route, so `/users/:id` and `/users/:name` collide:

```text
thread 'main' panicked at src/main.rs:12:9:
duplicate route: GET /users/:name is already defined
```

`/users/:id(\d+)` and `/users/:name` don't collide, since only one of them has a constraint.

## The matched route: `req.route()`

`req.route()` returns the pattern of the route that matched, mount prefix included, like Express's `req.route.path` (or axum's `MatchedPath`):

```rust
app.get("/users/:id", |req: Request| async move {
    format!("route: {}", req.route().unwrap_or("?")) // "route: /users/:id"
});
```

| Request | `req.path()` | `req.route()` |
|---|---|---|
| `GET /users/42` | `/users/42` | `Some("/users/:id")` |
| `GET /api/v1/items/a/b`, router mounted at `/api/:version` with a route `/items/*path` | `/api/v1/items/a/b` | `Some("/api/:version/items/*path")` |
| `GET /archive/2024`, route `/archive/:year?/:month?` | `/archive/2024` | `Some("/archive/:year?/:month?")` |
| `DELETE /users/42`, with no `DELETE` route (`405`) | `/users/42` | `None` |
| `GET /nope` (`404`) | `/nope` | `None` |

The pattern is the one you declared, constraints and `?` included: `r"/users/:id(\d+)"` gives `/users/:id(\d+)`.

It's what you want in logs and metrics: `/users/:id` makes one line in a dashboard, where the raw path would make one per user. Global middleware runs before routing, so there `req.route()` looks the route up on demand. Call it before `next.run(req)`, which takes the request; the result is a `&'static str`, so you can keep it afterwards:

```rust
use std::time::Instant;

app.middleware(|req: Request, next: Next| async move {
    let route = req.route().unwrap_or("unmatched");
    let start = Instant::now();
    let res = next.run(req).await;
    println!("{route} {} {:?}", res.status_code(), start.elapsed());
    res
});
```

In router middleware, route middleware and handlers, the route is already known and `req.route()` costs nothing.

## Named routes and URL generation

Writing `/users/42` by hand in every link and redirect works until the route changes. Give the route a name instead, with `.name(...)` right after it, and let Vitesse build its paths, like actix-web's `url_for`:

```rust
async fn show_user(req: Request) -> String {
    format!("user {}", req.param("id").unwrap_or("?"))
}

async fn create_user(req: Request) -> vitesse::Result<Redirect> {
    let id = 42; // the id of the new user
    let link = req.path_for("user", &[&id])?; // "/users/42"
    Ok(Redirect::see_other(link))
}

app.get("/users/:id", show_user).name("user");
app.post("/users", create_user);
```

- `req.path_for(name, &[values])` returns the path of the named route, with its parameters filled in **in order**. The values can be anything that implements `Display`: `&[&42]`, `&[&"LAX", &"SFO"]`, `&[]` for a route without parameters.
- `req.url_for(name, &[values])` returns the absolute URL, `https://example.com/users/42`: the same path, preceded by `req.protocol()` and `req.host()`. Behind a proxy, declare it with [`app.trust_proxy`](production.md#getting-the-clients-ip-address) so they reflect the client's view.

| Route | Call | Result |
|---|---|---|
| `/users/:id` | `path_for("user", &[&42])` | `/users/42` |
| `/users/:id` | `path_for("user", &[&"ada lovelace"])` | `/users/ada%20lovelace` |
| `/flights/:from-:to` | `path_for("flight", &[&"LAX", &"SFO"])` | `/flights/LAX-SFO` |
| `/files/*path` | `path_for("file", &[&"docs/report 2024.pdf"])` | `/files/docs/report%202024.pdf` |
| `/archive/:year?/:month?` | `path_for("archive", &[&2024])` | `/archive/2024` |
| `/users/:id` | `url_for("user", &[&42])` | `https://example.com/users/42` |

- Values are percent-encoded, so a space becomes `%20` and a `/` becomes `%2F`. Only a wildcard keeps its `/`, since it stands for several segments.
- Optional parameters at the end can be left out, and so can a wildcard: `path_for("archive", &[])` gives `/archive`.
- Constraints are checked: `path_for` refuses `"ada"` for `r"/users/:id(\d+)"`.
- The path is built when you call it, so it can go anywhere: a `Location` header, a link in `Html(...)`, a JSON field.

Names are checked at two moments:

- **At startup**, `name` panics if no route comes right before it (a `mount`, for instance), or if the name is already taken: names are unique across the whole application.
- **When you call it**, `path_for` and `url_for` return a `500` error if the name is unknown, if a value is missing or extra, if a value doesn't satisfy its constraint, or (`url_for` only) if the request has no host. The client only sees `{"error":"Internal Server Error"}`; the cause (`cannot build the URL of 'user': missing a value for ':id'`) is printed on standard error, like other server errors.

Routes declared in a [router](routers.md#named-routes-in-routers) can be named too. The name belongs to the whole application, and the path includes the mount prefix: its parameters come first in the list of values.

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
- A [router](routers.md#router-fallback) can have its own fallback for the paths under its prefix, with `router.fallback(handler)`: JSON `404`s under `/api`, for example, and an HTML page everywhere else.
- To change the format of *every* error response at once (404, 405, 400, 500 and so on), use `app.on_error` instead. See [Error handling](errors.md).

## Parameter callbacks and model binding

`app.param(name, middleware)` runs a middleware before the handler of every route that has the parameter `name`, after routing: it's Express's `app.param()`. Load or check what the parameter designates once, for all the routes that use it:

```rust
app.param("id", |req: Request, next: Next| async move {
    match req.param_as::<u32>("id") {
        Ok(id) if id > 0 => next.run(req).await,
        _ => Error::bad_request("invalid id").into_response(),
    }
});
app.get("/users/:id", show_user);   // checked
app.get("/posts/:id", show_post);   // checked
app.get("/about", about);           // untouched
```

`app.bind_param(name, loader)` is Laravel's route model binding, built on it: the loader turns the value of the parameter into a value of your own, which the handler reads with `req.get::<T>()`. When the loader returns an error, it becomes the response and the handler isn't called:

```rust
#[derive(Clone)]
struct User {
    id: u32,
    name: String,
}

app.bind_param("user", |req: &Request, id: &str| {
    let db = req.state::<Db>(); // `&'static`: the future can keep it
    let id = id.parse::<u32>();
    async move {
        let id = id.map_err(|_| Error::bad_request("invalid user id"))?;
        db.find_user(id).await.ok_or_else(|| Error::not_found("no such user"))
    }
});

app.get("/users/:user", |req: Request| async move {
    let user = req.get::<User>().expect("bound by bind_param");
    format!("#{} {}", user.id, user.name)
});
app.delete("/users/:user", delete_user); // the same loader, for free
```

- The callbacks apply to the routes added before or after them, in the order of the parameters in the path; routes without the parameter pay nothing.
- Like in Express, they belong to the application or the router they are declared on: `router.param(…)` covers the routes of that router, and the application's callbacks don't apply to the routes of a mounted router or of a [group](routers.md#route-groups-appgroup): declare them in the group, with `group.param(…)`.
- They run after the global and router middleware (authentication first, then loading), and before the route's own middleware (`.with`).

## Organising routes

As your app grows, group related routes in a `Router` and mount it under a prefix, as you would with `express.Router()`:

```rust
let mut api = Router::new();
api.get("/users", list_users).post("/users", create_user);

app.mount("/api", api); // GET /api/users, POST /api/users
```

`app.group(prefix, |group| { … })` does the same without naming the router, and plugins (`app.register`) package routes for reuse. Routers can have their own middleware and can be nested, see [Routers](routers.md). For middleware on a single route (`handler.with(auth)`), see [Middleware](middleware.md).
