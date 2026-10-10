# Error handling

In Vitesse, errors are ordinary values: a handler returns `Err(...)`, usually through the `?` operator, and the error becomes an HTTP response with the right status. This page covers the `Error` type, automatic conversions, custom error pages, `404`s and panics.

## The `Error` type

`vitesse::Error` carries three things:

- an HTTP **status**;
- a **message**, sent to the client as JSON: `{"error": "message"}`;
- optionally, a **source**: the original error, written to the server logs but never sent to the client.

| Constructor | Status |
|---|---|
| `Error::bad_request(msg)` | `400 Bad Request` |
| `Error::unauthorized(msg)` | `401 Unauthorized` |
| `Error::forbidden(msg)` | `403 Forbidden` |
| `Error::not_found(msg)` | `404 Not Found` |
| `Error::conflict(msg)` | `409 Conflict` |
| `Error::payload_too_large()` | `413 Payload Too Large` |
| `Error::unprocessable(msg)` | `422 Unprocessable Entity` |
| `Error::internal(msg)` | `500 Internal Server Error` |
| `Error::new(status, msg)` | any status: `Error::new(418, "I'm a teapot")` |
| `Error::from_status(status)` | any status, with its standard reason as message (`Service Unavailable`) |

`.with_source(err)` attaches the original cause. On an existing error, `status()`, `message()` and `source()` read its parts, and `Error` implements `Display` (`500 Internal Server Error (cause)`) for your logs.

## Returning errors from a handler

Make your handler return `vitesse::Result<T>`, an alias for `Result<T, vitesse::Error>`, and use `?`:

```rust
use vitesse::prelude::*;

// `find_item` is your own function returning an `Option<Item>`.
async fn show_item(req: Request) -> vitesse::Result<Json<Item>> {
    let id: u64 = req.param_as("id")?; // 400 if `id` isn't a number
    let item = find_item(id).ok_or_else(|| Error::not_found(format!("item {id} not found")))?;
    Ok(Json(item))
}
```

```http
GET /items/42 HTTP/1.1

HTTP/1.1 404 Not Found
content-type: application/json

{"error":"item 42 not found"}
```

In a closure, Rust can't guess the error type: annotate the final `Ok`:

```rust
app.get("/double/:n", |req: Request| async move {
    let n: i64 = req.param_as("n")?;
    Ok::<_, Error>(format!("{}", n * 2))
});
```

An `Error` can also be returned directly (`async { Error::new(418, "I'm a teapot") }`), and returning `None` from a handler that returns an `Option` produces a `404 {"error":"Not Found"}`.

Compared with Express, there's no `next(err)` and no exception to catch: the error travels through the return value, and the compiler makes sure it is handled.

## `?` with other error types

Any standard error (`std::error::Error + Send + Sync + 'static`: I/O errors, parsing errors, `serde_json`, database drivers…) converts automatically with `?`. It becomes a `500 {"error":"Internal Server Error"}`: the details are **not** sent to the client (they could leak sensitive information), but logged on the server's standard error output:

```rust
app.get("/parse", |_| async {
    let n: u32 = "abc".parse()?; // ParseIntError -> 500
    Ok::<_, Error>(n.to_string())
});
```

```text
[vitesse] error 500: invalid digit found in string
```

When the client deserves a better message, convert the error yourself with `map_err`:

```rust
app.get("/config", |_| async {
    let text = std::fs::read_to_string("config.toml")
        // 500 with a clear message; the I/O error is kept as the source and logged.
        .map_err(|e| Error::internal("configuration unavailable").with_source(e))?;
    let port: u16 = text.trim().parse()
        // 400: it's the client's fault... in this example.
        .map_err(|_| Error::bad_request("expected a port number"))?;
    Ok::<_, Error>(format!("port {port}"))
});
```

> [!NOTE]
> `Box<dyn std::error::Error + Send + Sync>` and `anyhow::Error` don't implement `std::error::Error` themselves, so `?` can't convert them. `with_source` accepts them, though: `.map_err(|e| Error::internal("...").with_source(e))?`.

### Errors produced by Vitesse

| Situation | Response |
|---|---|
| `req.param_as` fails | `400`: `invalid parameter 'id': 'abc'` or `missing parameter 'id'` |
| `req.query_as` fails | `400`: `invalid query string: ...` |
| `req.json` / `req.form` on an invalid body | `400`: `invalid JSON: ...` / `invalid form data: ...` |
| `req.text` on a body that isn't UTF-8 | `400`: `the body is not valid UTF-8` |
| Body larger than `app.body_limit` | `413 Payload Too Large` |
| No route matches | `404`, `{"error":"Cannot GET /path"}` |
| The path exists, but not for this method | `405 Method Not Allowed`, with an `Allow` header |
| A handler or a middleware panics | `500 Internal Server Error` |
| `middleware::timeout` expires | `503`, `request timed out` |

## Your own error types

To use `?` with your own business errors, map them to a `vitesse::Error`. Two approaches, depending on whether your type implements `std::error::Error`.

**Your type doesn't implement `std::error::Error`**: implement `From<YourError> for vitesse::Error`, and `?` does the conversion:

```rust
#[derive(Debug)]
enum ShopError {
    OutOfStock(u64),
    InvalidQuantity,
    Database(std::io::Error),
}

impl From<ShopError> for Error {
    fn from(err: ShopError) -> Self {
        match err {
            ShopError::OutOfStock(id) => Error::conflict(format!("item {id} is out of stock")),
            ShopError::InvalidQuantity => Error::unprocessable("invalid quantity"),
            ShopError::Database(e) => Error::from_status(500).with_source(e),
        }
    }
}

// `reserve` is your business logic: fn reserve(id: u64) -> Result<(), ShopError>
async fn order(req: Request) -> vitesse::Result<&'static str> {
    let id: u64 = req.param_as("id")?;
    reserve(id)?; // ShopError -> vitesse::Error
    Ok("reserved")
}
```

**Your type implements `std::error::Error`** (by hand or with `thiserror`): the `From` above would conflict with the automatic conversion (which turns every standard error into a `500`). Implement `IntoResponse` instead, and return `Result<T, YourError>`:

```rust
use std::fmt;

#[derive(Debug)]
enum ApiError {
    InvalidId,
    NotFound(u64),
    Io(std::io::Error),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::InvalidId => write!(f, "invalid id"),
            ApiError::NotFound(id) => write!(f, "note {id} not found"),
            ApiError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let err = match self {
            ApiError::InvalidId => Error::bad_request("invalid id"),
            ApiError::NotFound(id) => Error::not_found(format!("note {id} not found")),
            other => Error::from_status(500).with_source(other),
        };
        err.into_response()
    }
}

async fn read_note(req: Request) -> Result<String, ApiError> {
    let id: u64 = req.param("id").and_then(|s| s.parse().ok()).ok_or(ApiError::InvalidId)?;
    match tokio::fs::read_to_string(format!("notes/{id}.txt")).await {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(ApiError::NotFound(id)),
        Err(e) => Err(ApiError::Io(e)),
    }
}
```

> [!TIP]
> Build the response from a `vitesse::Error`, as above, rather than by hand: that way it keeps the usual JSON format, the source is logged, and it goes through `app.on_error`.

## Custom error pages: `app.on_error`

By default, errors are sent as `{"error": "message"}`. `app.on_error` replaces that format for the whole application, like Express's `(err, req, res, next)` error middleware:

```rust
app.on_error(|err: Error| {
    if let Some(source) = err.source() {
        eprintln!("{} {}: {source}", err.status().as_u16(), err.message());
    }
    res::status(err.status()).json(json!({
        "error": { "status": err.status().as_u16(), "message": err.message() }
    }))
});
```

The function receives the `Error` and returns anything that implements `IntoResponse`. It is called for **every response produced from an `Error`**: errors returned by handlers and middleware, the default `404`, `405`s, body parsing errors, `413`s, panics, timeouts, missing static files. It doesn't apply to responses you build yourself, such as `(404, "not here")` or `StatusCode::NOT_FOUND`. Headers set by your middleware (CORS, for example) are kept.

> [!WARNING]
> Set the status yourself, as in `res::status(err.status())`: if you return just `Html(...)`, the error page goes out with a `200 OK`. Also, once `on_error` is set, the source of `500`s is no longer logged automatically: log it in your handler, as above.

`on_error` doesn't receive the request. If your format depends on it (HTML for browsers, JSON for everything else), write a global middleware that inspects the response with `res.take_error()`:

```rust
app.middleware(|req: Request, next: Next| async move {
    let wants_html = req.header("accept").is_some_and(|a| a.contains("text/html"));
    let mut res = next.run(req).await;
    if wants_html {
        if let Some(err) = res.take_error() {
            return res::status(err.status())
                .html(format!("<h1>{}</h1><p>{}</p>", err.status(), err.message()));
        }
    }
    res
});
```

## Custom 404: `app.fallback`

When no route matches, Vitesse calls the fallback handler, which answers `404 {"error":"Cannot GET /path"}` by default. Replace it with `app.fallback`, which takes a regular handler:

```rust
app.fallback(|req: Request| async move {
    (404, Html(format!("<h1>Page not found</h1><p>{} doesn't exist.</p>", req.path())))
});
```

Global middleware also runs for the fallback, and so does the middleware of a [router](routers.md) whose prefix covers the path. The fallback isn't called when the path exists with another method (that's a `405`). If it returns an `Error`, the response goes through `on_error`. For single-page applications, see [Static files](static-files.md).

## Panics

A panic in a handler or in a middleware (an `unwrap()` on `None`, an index out of bounds, `req.state::<T>()` for a type that was never registered…) doesn't bring the server down: Vitesse catches it and answers `500 {"error":"Internal Server Error"}`. Rust prints the panic message on standard error, other requests carry on normally, and the `500` goes back through the middleware around it (CORS headers, for example, are kept) and through `on_error`, like any other error.

Even so, prefer `?` and explicit errors: a panic is a bug, not a way to answer.

> [!WARNING]
> Panics can only be caught with the default unwinding strategy. With `panic = "abort"` in a `[profile]` of your `Cargo.toml`, a panic stops the whole process.

> [!NOTE]
> This applies to middleware written as a closure or an `async fn`, whether it is added with `app.middleware`, `router.middleware` or `.with`. The only exception is a hand-written `impl Middleware for MyType` that panics: the panic is caught further up, at the very top of the chain if no closure or `async fn` middleware surrounds it, and the client then gets a plain `500 {"error":"Internal Server Error"}` that doesn't go through `on_error`.
