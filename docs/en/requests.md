# Reading requests

Every handler receives a `Request`. From it you can read the method, the path, route parameters, the query string, headers, cookies and the session, the body, uploaded files and the client's address, find out which format the client prefers, and build the URLs of your routes. This page covers each of them, with the Express equivalent where there is one.

## The `Request` object

A handler receives the request by value. Almost every method takes `&self`, body reading included, so you can hold on to a parameter while you wait for the body:

```rust
app.put("/users/:id", |req: Request| async move {
    let id: u64 = req.param_as("id")?;
    let agent = req.header("user-agent").unwrap_or("unknown");
    let body: vitesse::serde_json::Value = req.json().await?;
    Ok::<_, Error>(format!("user {id} updated by {agent}: {body}"))
});
```

The few methods that modify the request (`set`, `set_body`, `set_uri`, `set_method`, `set_body_limit`, `headers_mut`, `extensions_mut`) need a `mut req`. You'll mostly use them in [middleware](middleware.md).

## Method, path and URL

```rust
app.all("/debug", |req: Request| async move {
    format!(
        "{} {} query={:?} version={:?} host={:?}",
        req.method(),       // GET
        req.path(),         // /debug
        req.query_string(), // Some("a=1&b=2")
        req.version(),      // HTTP/1.1
        req.hostname(),     // Some("localhost")
    )
});
```

- `req.method()` returns a `&Method`. Compare it with `*req.method() == Method::POST`.
- `req.path()` returns the path without the query string, exactly as received (not decoded), like `req.path` in Express.
- `req.uri()` returns the full `http::Uri`, path and query string, like `req.originalUrl`.
- `req.hostname()` returns the requested host without the port: `example.com` for `example.com:8080`. `req.host()` keeps the port (see [Host and protocol](#host-and-protocol)).
- `req.version()` returns the HTTP version (`vitesse::http::Version`).

### Rewriting the URL

`req.set_uri(uri)` replaces the path and query string. Global middleware runs before routing, so a rewrite there changes which route answers:

```rust
app.middleware(|mut req: Request, next: Next| async move {
    if req.path() == "/old-page" {
        req.set_uri(vitesse::http::Uri::from_static("/new-page"));
    }
    next.run(req).await
});
```

The browser doesn't see this rewrite. To send it to the new address instead, return a `Redirect::to(...)` (see [Sending responses](responses.md)).

### Method override

`req.set_method(method)` replaces the method. Like `set_uri`, it changes which route answers when you call it in a global middleware. That's how you write the equivalent of Express's `method-override` package: HTML forms can only send `GET` and `POST`, so the form passes the method it wants in the query string:

```html
<form method="post" action="/posts/42?_method=DELETE">
  <button>Delete</button>
</form>
```

```rust
app.middleware(|mut req: Request, next: Next| async move {
    if *req.method() == Method::POST {
        let wanted = req
            .query("_method")
            .and_then(|m| m.to_ascii_uppercase().parse::<Method>().ok())
            .filter(|m| [Method::PUT, Method::PATCH, Method::DELETE].contains(m));
        if let Some(method) = wanted {
            req.set_method(method);
        }
    }
    next.run(req).await
});

app.delete("/posts/:id", |_| async { "post deleted" }); // also reached by the form
```

Only `POST` requests are rewritten, and only into `PUT`, `PATCH` or `DELETE`, as `method-override` does by default: a link (`GET`) can't delete anything.

## Route parameters

These are covered in detail in [Routing](routing.md#path-parameters). In short:

```rust
app.get("/users/:id/posts/:post", |req: Request| async move {
    let user: u64 = req.param_as("id")?;              // typed, 400 if invalid
    let post = req.param("post").unwrap_or_default(); // Option<&str>
    let all: Vec<String> = req.params().map(|(k, v)| format!("{k}={v}")).collect();
    Ok::<_, Error>(format!("{user} {post} {}", all.join("&")))
});
```

`req.params_as::<T>()?` reads them all into a struct at once (see [Typed parameters](routing.md#all-parameters-at-once-params_as)).

### The matched route

`req.route()` returns the pattern of the route that answers the request, mount prefix included: `Some("/users/:id/posts/:post")` for `/users/42/posts/7`. It's Express's `req.route.path`, and it's what you want in logs and metrics, where the raw path would give one line per user. See [Routing](routing.md#the-matched-route-reqroute).

### Building URLs: `path_for` and `url_for`

Give a route a name with `.name(...)`, and any handler can build its URL instead of writing it by hand (actix-web's `url_for`; Express has no equivalent):

```rust
app.get("/users/:id", show_user).name("user");

app.post("/users", |req: Request| async move {
    let id = 42; // the id of the user you just created
    let location = req.url_for("user", &[&id])?; // "https://example.com/users/42"
    Ok::<_, Error>(res::status(201).header("location", location).json(json!({ "id": id })))
});
```

- `req.path_for(name, &[...])` returns the path, `"/users/42"`: the values fill the route's parameters in order, and are percent-encoded (`"a b"` gives `a%20b`). Pass `&[]` for a route without parameters, and `&[&"LAX", &"SFO"]` for several.
- `req.url_for(name, &[...])` adds the protocol and the host of the current request (`req.protocol()` and `req.host()`, so it follows [`trust_proxy`](#behind-a-proxy-trust_proxy)). For links that leave the site, in an email for instance, prefer `path_for` after a base URL from your configuration (see [Host and protocol](#host-and-protocol)).
- Both return an `Error` for an unknown name, a missing or extra value, or a value that doesn't satisfy the parameter's constraint (and `url_for` for a request without a host). These are programming mistakes, not client errors: the client gets a plain `500 {"error":"Internal Server Error"}`, and the cause is printed on standard error.

Naming routes, optional parameters and constraints are covered in [Routing](routing.md#named-routes-and-url-generation).

## Query string

```rust
app.get("/search", |req: Request| async move {
    let q = req.query("q").unwrap_or_default();
    let page: u32 = req.query("page").and_then(|p| p.parse().ok()).unwrap_or(1);
    format!("searching {q:?}, page {page}")
});
```

`GET /search?q=caf%C3%A9+cr%C3%A8me&page=2` answers `searching "café crème", page 2`.

- `req.query(name)` returns the decoded value of the first parameter with that name, as an `Option<Cow<str>>`. A `+` becomes a space. A `Cow<str>` works like a `&str` (and `.into_owned()` gives you a `String`). It only allocates memory when the value contained encoded characters.
- `req.query_pairs()` iterates over every `(name, value)` pair, repeated names included. That's how you read `?tag=a&tag=b`:

```rust
app.get("/tags", |req: Request| async move {
    let tags: Vec<String> = req
        .query_pairs()
        .filter(|(key, _)| key == "tag")
        .map(|(_, value)| value.into_owned())
        .collect();
    format!("tags: {}", tags.join(", "))
});
```

- `req.query_string()` returns the raw string (`q=caf%C3%A9+cr%C3%A8me&page=2`), or `None` when there's no query string.

### Typed query strings with `query_as`

Once you have more than a couple of parameters, deserialise the whole query string into a struct with `req.query_as::<T>()`. Use `Option` for optional parameters and `#[serde(default)]` for default values:

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Search {
    q: String,
    page: Option<u32>,
    #[serde(default)]
    exact: bool,
}

async fn search(req: Request) -> vitesse::Result<String> {
    let s: Search = req.query_as()?;
    Ok(format!("{} (page {}, exact: {})", s.q, s.page.unwrap_or(1), s.exact))
}
```

If a required field is missing or a value doesn't convert, `query_as` returns a `400 Bad Request`:

```text
GET /search               → 400 {"error":"invalid query string: missing field `q`"}
GET /search?q=x&page=abc  → 400 {"error":"invalid query string: invalid digit found in string"}
```

> [!NOTE]
> `query_as` doesn't handle repeated keys: `?q=a&q=b` is rejected with a `400` (`duplicate field`). Use `req.query_pairs()` for those.

## Headers

```rust
app.get("/whoami", |req: Request| async move {
    let agent = req.header("user-agent").unwrap_or("unknown");
    format!("You are using {agent}")
});
```

- `req.header(name)` returns the value as an `Option<&str>`, like `req.get('User-Agent')` in Express. The name is case-insensitive. You can also pass a constant from `vitesse::header`, such as `req.header(header::AUTHORIZATION)`.
- `req.header_all(name)` iterates over every value of a repeated header.
- `req.headers()` returns the full `HeaderMap`, for example to loop over every header. It's built the first time you call it, so when you only need a few headers, `req.header(...)` is cheaper: it reads the raw request directly.
- `req.headers_mut()` lets a middleware add, change or remove headers before the handler sees them.

> [!NOTE]
> A header value that isn't plain visible ASCII (accented letters, for example) can't always be returned as a `&str`. In that case, read the raw bytes with `req.headers().get("x-name").map(|v| v.as_bytes())`.

### Content type

- `req.content_type()` is a shortcut for `req.header("content-type")`.
- `req.is(type)` checks the body's media type, like Express's `req.is()`, but returns a `bool`. It accepts a full type (`"application/json"`), a subtype (`"json"`, which also matches suffixes such as `application/ld+json`) or a wildcard (`"text/*"`). Parameters like `; charset=utf-8` are ignored.

```rust
app.post("/import", |req: Request| async move {
    if !req.is("json") {
        return Err(Error::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, "expected a JSON body"));
    }
    let items: Vec<vitesse::serde_json::Value> = req.json().await?;
    Ok(format!("{} items imported", items.len()))
});
```

## Content negotiation

Some routes can answer in several formats: HTML for a browser, JSON for a script. `req.accepts(&[...])` reads the `Accept` header and returns the format the client prefers among the ones you offer, like Express's `req.accepts()`:

```rust
async fn show_user(req: Request) -> Response {
    match req.accepts(&["html", "json"]) {
        Some("html") => res::html("<h1>Ada Lovelace</h1>"),
        Some(_) => res::json(json!({ "name": "Ada Lovelace" })),
        None => res::send_status(406), // Not Acceptable
    }
}
```

| `Accept` sent by the client | `req.accepts(&["html", "json"])` |
|---|---|
| none | `Some("html")`: your first offer |
| `text/html,application/xhtml+xml,…,*/*;q=0.8` (a browser) | `Some("html")` |
| `application/json` | `Some("json")` |
| `*/*` | `Some("html")` |
| `image/png` | `None` |

- Offers are file extensions (`"html"`, `"json"`, `"csv"`, `"txt"`…) or MIME types (`"application/json"`, `"image/*"`). You get back the offer as you wrote it, so you can `match` on it.
- The highest quality (`q=`) wins, then the most specific match (`text/html` before `text/*` before `*/*`). A type sent with `q=0` is refused.
- Without an `Accept` header, the client takes anything: you get your first offer.
- `None` means that nothing you offer is acceptable: answer `406 Not Acceptable`, or fall back to a default format.

> [!TIP]
> When the response depends on `Accept`, add `.header("vary", "accept")` to it, so that caches (the browser's, a CDN) don't serve the JSON version to a browser.

`req.accepts_language(&[...])` does the same with `Accept-Language`, like `req.acceptsLanguages()`. A language matches its regional variants and the other way round: `"fr"` matches `fr-FR`, and `"en-GB"` matches `en`. Without the header you get the first language, and `None` when no language matches, so provide a default:

```rust
app.get("/", |req: Request| async move {
    let lang = req.accepts_language(&["en", "fr", "es"]).unwrap_or("en");
    Redirect::to(format!("/{lang}/"))
});
```

A French browser (`fr-FR,fr;q=0.9,en;q=0.8`) is sent to `/fr/`, a German one (`de`) to `/en/`.

## Cookies

`req.cookie(name)` reads a cookie from the `Cookie` header. In Express you'd need `cookie-parser` for `req.cookies.name`; here nothing needs to be installed:

```rust
app.get("/", |req: Request| async move {
    match req.cookie("session") {
        Some(id) => format!("Welcome back (session {id})"),
        None => "Hello, stranger".to_string(),
    }
});
```

The value comes back as the browser sent it (surrounding quotes removed), without URL decoding. To set or delete cookies, see [Sending responses](responses.md#cookies).

### Signed cookies and the session

With the `session` feature, two more things come from cookies. `req.signed_cookie(name)` reads a cookie signed with the app's secret (`app.cookie_secret(...)`), like Express's `req.signedCookies.name`: you get the value, or `None` if the cookie is missing or the visitor changed it. And with the `session` middleware, `req.session()` returns the visitor's session, like `req.session` with express-session:

```rust
app.get("/", |req: Request| async move {
    let theme = req.signed_cookie("theme").unwrap_or_else(|| "light".into());
    let user: Option<String> = req.session().get("user");
    format!("theme: {theme}, user: {user:?}")
});
```

Signing, sessions, logging in and out and session stores are covered in [Signed cookies and sessions](sessions.md).

## Body

Vitesse has no body-parsing middleware. You read the body by calling a method, which collects and parses it on the spot. Every method is `async` and returns a `vitesse::Result`, so `?` turns a bad body into the right HTTP error:

| Method | Express equivalent | Gives you | Error |
|---|---|---|---|
| `req.json::<T>().await` | `express.json()` | `T` (deserialised) | `400` if the JSON is invalid |
| `req.form::<T>().await` | `express.urlencoded()` | `T` (deserialised) | `400` if the form is invalid |
| `req.text().await` | `express.text()` | `String` | `400` if it isn't UTF-8 |
| `req.bytes().await` | `express.raw()` | `Bytes` | |
| `req.multipart()?` | `multer` | the fields of an upload form, one by one (see [File uploads](#file-uploads-multipart)) | `415` if it isn't `multipart/form-data` |
| `req.take_body()` | reading `req` as a stream | `Body` (a stream) | |

The first four read the whole body into memory, up to the [size limit](#body-size-limit-and-413), which also applies to `multipart`.

### JSON

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct NewPost {
    title: String,
    tags: Vec<String>,
    draft: Option<bool>,
}

async fn create_post(req: Request) -> vitesse::Result<String> {
    let post: NewPost = req.json().await?;
    Ok(format!(
        "{} ({} tags, draft: {})",
        post.title,
        post.tags.len(),
        post.draft.unwrap_or(false)
    ))
}
```

- Invalid JSON, a missing field or a wrong type gives a `400` that explains why (example below). An empty body is invalid JSON too.
- Unknown fields are ignored. Add `#[serde(deny_unknown_fields)]` to the struct to reject them.
- For JSON with no fixed shape, ask for a `vitesse::serde_json::Value`.
- Unlike `express.json()`, `req.json()` doesn't look at the `Content-Type`: it parses whatever was sent. If you want to require it, check `req.is("json")` first (example above).

For example, sending `{"tags": []}` to this handler gives:

```json
{"error":"invalid JSON: missing field `title` at line 1 column 12"}
```

To check the content too (a title that isn't empty, an email address that looks like one…), read the body with `req.valid_json()`, which answers `422` with the messages of each invalid field. See [Validation](validation.md).

### Forms

HTML forms (`application/x-www-form-urlencoded`) work the same way, with `req.form()`:

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Login {
    email: String,
    password: String,
    // An unchecked checkbox isn't sent at all: default to `false`
    #[serde(default)]
    remember: bool,
}

async fn login(req: Request) -> vitesse::Result<Redirect> {
    let form: Login = req.form().await?;
    if form.email.is_empty() || form.password.is_empty() {
        return Err(Error::bad_request("email and password are required"));
    }
    // … check the credentials, open a session…
    Ok(Redirect::see_other("/dashboard"))
}
```

Forms that upload files (`enctype="multipart/form-data"`) are read with `req.multipart()`: see [File uploads (multipart)](#file-uploads-multipart).

### Text and raw bytes

```rust
app.post("/shout", |req: Request| async move {
    let text = req.text().await?; // String, 400 if it isn't valid UTF-8
    Ok::<_, Error>(text.to_uppercase())
});

app.post("/size", |req: Request| async move {
    let bytes = req.bytes().await?; // vitesse::Bytes
    Ok::<_, Error>(format!("{} bytes received", bytes.len()))
});
```

The body is read once, then kept in memory, so you can call these methods several times. For example, a webhook can check a signature on the raw bytes and then parse the same body as JSON:

```rust
app.post("/webhook", |req: Request| async move {
    let raw = req.bytes().await?;          // check a signature on the raw bytes…
    let event: vitesse::serde_json::Value = req.json().await?; // …then parse them
    Ok::<_, Error>(StatusCode::NO_CONTENT)
});
```

### Body size limit and 413

`json`, `form`, `text` and `bytes` read the whole body into memory, so its size is limited: **1 MiB** by default (`vitesse::DEFAULT_BODY_LIMIT`). A larger body gets a `413 Payload Too Large`. If the client announced the size with `Content-Length`, the request is turned away without reading the body at all. To change the limit for the whole app:

```rust
let mut app = App::new();
app.body_limit(10 * 1024 * 1024); // 10 MiB
```

Most routes only receive small JSON bodies, though, and only a few need more. Keep the global limit low and raise it where it's needed with `middleware::body_limit(bytes)`, on one route with `.with(...)` or on a whole [router](routers.md). It's the equivalent of `express.json({ limit: '50mb' })` on a single route:

```rust
async fn import(req: Request) -> vitesse::Result<String> {
    let rows: Vec<vitesse::serde_json::Value> = req.json().await?;
    Ok(format!("{} rows imported", rows.len()))
}

let mut app = App::new(); // 1 MiB by default…
app.post("/import", import.with(middleware::body_limit(50 * 1024 * 1024))); // …50 MiB here

let mut admin = Router::new();
admin.middleware(middleware::body_limit(200 * 1024 * 1024)); // 200 MiB under /admin
admin.post("/restore", restore_backup);
app.mount("/admin", admin);
```

The limit belongs to each request: `req.body_limit()` returns the one that applies, and `req.set_body_limit(bytes)` changes it, which is all `middleware::body_limit` does. Your own middleware can use it to choose a limit per user, for example (`CurrentUser` being set by your authentication middleware):

```rust
async fn larger_bodies_for_admins(mut req: Request, next: Next) -> Response {
    if req.get::<CurrentUser>().is_some_and(|user| user.admin) {
        req.set_body_limit(1024 * 1024 * 1024); // 1 GiB
    }
    next.run(req).await
}
```

- The limit only kicks in when you read the body. A handler that never reads it never produces a `413`.
- It also covers [`req.multipart()`](#file-uploads-multipart) (the whole form) and [`middleware::decompress()`](middleware.md#decompress) (the compressed and the decompressed size), but not `take_body()`.
- It works the same over HTTP/1.1, [HTTP/2](https.md) and [HTTP/3](http3.md).
- A body cut short by tower-http's `RequestBodyLimitLayer` (see [Tower layers](middleware.md#tower-layers)) also gives a `413`.

### Streaming with `take_body`

For a large raw upload (not a form), or to relay a body elsewhere, `req.take_body()` hands you the raw body as a stream (`vitesse::Body`) without loading it into memory. Data comes in as the client sends it.

> [!WARNING]
> `take_body()` bypasses the body limit (`app.body_limit`, `middleware::body_limit`), so limiting the size is up to you. Afterwards, `json()`, `text()` and the other body methods can no longer read the body: they fail with a `500`.

`Body` implements the standard `http_body::Body` trait. The easiest way to read it chunk by chunk is the `frame()` method from [http-body-util](https://docs.rs/http-body-util) (`cargo add http-body-util`). This example saves an upload to a file, capped at 100 MiB:

```rust
use http_body_util::BodyExt; // for .frame()
use vitesse::prelude::*;
use vitesse::tokio::{fs::File, io::AsyncWriteExt};

const MAX_UPLOAD: usize = 100 * 1024 * 1024; // 100 MiB

async fn upload(req: Request) -> vitesse::Result<String> {
    let mut body = req.take_body();
    let mut file = File::create("upload.bin").await?;
    let mut total = 0;

    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|e| Error::bad_request("upload interrupted").with_source(e))?;
        if let Ok(chunk) = frame.into_data() {
            total += chunk.len();
            if total > MAX_UPLOAD {
                return Err(Error::payload_too_large());
            }
            file.write_all(&chunk).await?;
        }
    }
    file.flush().await?;
    Ok(format!("{total} bytes saved"))
}
```

`?` turns file errors (`std::io::Error`) into a `500`. A read error from the stream, though, is a generic `BoxError` that `?` can't convert by itself, which is why the example wraps it with `map_err`.

A response can also be a `Body`, so returning the stream as it is makes a streaming echo:

```rust
app.post("/echo", |req: Request| async move { req.take_body() });
```

### Replacing the body

`req.set_body(body)` replaces the body before the handler reads it. A middleware can use it to decrypt or transform the body ([`middleware::decompress()`](middleware.md#decompress) uses it for compressed bodies). It accepts anything that converts into a `Body`: `String`, `Vec<u8>`, `Bytes`, `&'static str` and so on. The new body is subject to the body limit like the original.

## File uploads (multipart)

Forms that send files use `enctype="multipart/form-data"`. In Express you'd add `multer`; in Vitesse, enable the `multipart` feature:

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["multipart"] }
```

```html
<form method="post" action="/upload" enctype="multipart/form-data">
  <input name="title">
  <input type="file" name="photos" multiple>
  <button>Send</button>
</form>
```

`req.multipart()?` opens the form, and `form.next_field().await?` returns its fields one by one, in the order the browser sent them, while the body is still arriving: a file is never held in memory unless you ask for it.

```rust
use vitesse::prelude::*;

async fn upload(req: Request) -> vitesse::Result<String> {
    let mut form = req.multipart()?; // 415 if the body isn't multipart/form-data
    let mut report = Vec::new();
    while let Some(field) = form.next_field().await? {
        let name = field.name().unwrap_or("").to_owned();
        match field.file_name().map(safe_name) {
            // A file: written to disk chunk by chunk, as it arrives.
            Some(file_name) => {
                let size = field.save(format!("uploads/{file_name}")).await?;
                report.push(format!("{name}: {file_name} ({size} bytes)"));
            }
            // A text input.
            None => {
                let value = field.text().await?;
                report.push(format!("{name} = {value}"));
            }
        }
    }
    Ok(report.join("\n"))
}

/// The client's file name, reduced to letters, digits, `-`, `_` and `.`
/// (no `/`, no `..`, no hidden file).
fn safe_name(client_name: &str) -> String {
    let name: String = client_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    match name.trim_start_matches('.') {
        "" => "upload".to_owned(),
        name => name.to_owned(),
    }
}

let mut app = App::new();
app.post("/upload", upload.with(middleware::body_limit(100 * 1024 * 1024))); // 100 MiB
```

| `Field` method | Gives you |
|---|---|
| `field.name()` | The input's `name` (`Option<&str>`) |
| `field.file_name()` | For a file, its name on the client's machine (`Option<&str>`); `None` for a text input |
| `field.content_type()` | The type announced by the browser (`image/png`…), if any |
| `field.headers()` | All the headers of the part |
| `field.chunk().await?` | The next piece of the content (`Option<Bytes>`), `None` at the end |
| `field.bytes().await?` | The whole content, in memory (`Bytes`) |
| `field.text().await?` | The whole content as a `String` (`400` if it isn't UTF-8) |
| `field.save(path).await?` | Writes the content to a file as it arrives (created, or truncated if it exists) and returns its size in bytes (`u64`) |

`bytes`, `text` and `save` consume the field. The fields arrive one after the other on the same stream, so read (or drop) each one before asking for the next. To process a file on the fly (hash it, send it elsewhere), read it chunk by chunk:

```rust
while let Some(mut field) = form.next_field().await? {
    let mut size = 0;
    while let Some(chunk) = field.chunk().await? {
        size += chunk.len(); // hash it, send it to object storage…
    }
    println!("{:?}: {size} bytes", field.name());
}
```

- The whole body is limited by the [body limit](#body-size-limit-and-413), 1 MiB by default: raise it on the upload routes with `middleware::body_limit`, as above. Beyond it, reading fails with a `413`.
- A body that isn't `multipart/form-data` gets a `415 Unsupported Media Type`, and a malformed form a `400`.
- `req.multipart()` takes the body: the other body methods can't read it afterwards.
- If the upload fails halfway (`413`, client gone), `save` removes the partial file and returns the error.
- The repository's `examples/live.rs` has a complete upload page (`cargo run --example live --features multipart`).

> [!WARNING]
> `file_name()` comes from the client: it can be `../../etc/passwd`, contain any character, or be the same as another user's file. Never use it as a path as is. Clean it up as above or, better, store the file under a name you choose (a random id, a database key) and keep the original name as information only. `content_type()` is what the client claims, too: check the content if it matters.

## Host and protocol

```rust
app.get("/where", |req: Request| async move {
    format!(
        "{} {} {:?} {:?}",
        req.protocol(), // "https"
        req.secure(),   // true
        req.host(),     // Some("example.com:8443")
        req.hostname(), // Some("example.com")
    )
});
```

| Method | Express | Returns |
|---|---|---|
| `req.protocol()` | `req.protocol` | `"https"` over TLS or HTTP/3, `"http"` otherwise |
| `req.secure()` | `req.secure` | `true` when `protocol()` is `"https"` |
| `req.host()` | `req.host` (Express 5) | The `Host` header with its port (`example.com:8443`), or the authority of an HTTP/2 or HTTP/3 request |
| `req.hostname()` | `req.hostname` | The same without the port: `example.com` (`::1` for `[::1]:3000`) |

Behind a reverse proxy, the request reaches Vitesse in plain HTTP, and the `Host` may be an internal name. Declare your proxy with [`app.trust_proxy`](#behind-a-proxy-trust_proxy), and these methods read `X-Forwarded-Proto` and `X-Forwarded-Host` instead.

> [!NOTE]
> The host comes from the client. Don't build security-sensitive links with it (the link of a password reset email, for example): use a base URL from your configuration.

## Client address

```rust
app.get("/ip", |req: Request| async move {
    match req.ip() {
        Some(ip) => format!("Your IP: {ip}"),
        None => "unknown".to_string(),
    }
});
```

- `req.ip()` returns the client's IP address as an `Option<IpAddr>`, like `req.ip`. By default, it's the address of the other end of the TCP connection.
- `req.remote_addr()` returns the IP and the port of the TCP connection as an `Option<SocketAddr>`, like `req.socket.remoteAddress`. It never reads proxy headers.
- On a Unix socket (`app.run_unix(...)`, see [Unix domain sockets](server.md#unix-domain-sockets)), the connection has no IP address: both are `None`, unless you set `trust_proxy` (below), in which case `req.ip()` comes from `X-Forwarded-For`.

### Behind a proxy: `trust_proxy`

Behind a reverse proxy or a load balancer (Nginx, Caddy, Heroku, a cloud load balancer…), the other end of the connection is the proxy: `req.ip()` would be the proxy's address for every request, and `req.protocol()` would say `"http"` even when the visitor uses HTTPS. The proxy passes the original values in headers: `X-Forwarded-For` (the client's address, followed by the proxies it went through), `X-Forwarded-Proto` and `X-Forwarded-Host`. As with Express's `app.set('trust proxy', …)`, tell Vitesse which proxies to believe:

```rust
let mut app = App::new();
app.trust_proxy("loopback"); // Nginx or Caddy on the same machine
```

| Value | Trusted proxies | Express |
|---|---|---|
| `false` (the default) | none: the headers are ignored | `false` |
| `"loopback"`, `"10.0.0.0/8, 192.168.1.10"` | the proxies whose address is in these networks | the same strings |
| `1`, `2`… | that number of proxies, the closest to the server, whatever their address | the same numbers |
| `true` | all of them: the client is the left-most address of `X-Forwarded-For` | `true` |

Networks are IP addresses or CIDR ranges, plus three names: `loopback` (`127.0.0.1/8`, `::1`), `linklocal` (`169.254.0.0/16`, `fe80::/10`) and `uniquelocal` (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `fc00::/7`). Combine them with commas: `"loopback, uniquelocal"`. An invalid address makes `trust_proxy` panic at startup. A proxy connected through a Unix socket, which only processes of the same machine can reach, is trusted with any value but `false` and `0`.

When the request comes from a trusted proxy:

- `req.ip()` walks `X-Forwarded-For` from the right (the closest proxy) and stops at the first address that isn't a trusted proxy: that's the client.
- `req.protocol()` and `req.secure()` follow `X-Forwarded-Proto: https`.
- `req.host()` and `req.hostname()` use `X-Forwarded-Host` when the proxy sends it.
- `req.remote_addr()` doesn't change: it remains the proxy's address.

For example, with `app.trust_proxy("loopback, 10.0.0.0/8")`, a request from the local proxy (`127.0.0.1`) carrying `X-Forwarded-For: 203.0.113.9, 10.0.0.2` comes from `203.0.113.9`: `10.0.0.2` is an internal load balancer, trusted, and `203.0.113.9` isn't. Built-in middleware relies on `req.ip()` too: [`middleware::rate_limit`](middleware.md#rate_limit) then counts per visitor rather than per proxy. Proxy configurations (Nginx, Caddy, Heroku) are in [Going to production](production.md).

> [!WARNING]
> Anyone can send `X-Forwarded-*` headers. Only trust the proxies you run, and make sure clients can't reach the application without going through them. Avoid `true` unless your proxy replaces `X-Forwarded-For` instead of appending to it: otherwise the client chooses its own address.

The parsed setting is a `vitesse::TrustProxy` (`"loopback".parse::<TrustProxy>()`), and `trust.trusts(ip, 0)` tells whether an address is trusted, should you need the same rule in your own code.

## State and per-request data

- `req.state::<T>()` returns the global state registered with `app.state(value)`, the equivalent of `app.locals`. It panics (and so answers `500`) if no state of that type exists. `req.try_state::<T>()` returns an `Option` instead. See [Shared state](state.md).
- `req.set(value)` attaches a value to this request and `req.get::<T>()` reads it back, the equivalent of `res.locals` or `req.user`. The type must implement `Clone`. This is how a middleware passes the logged-in user to the handler (see [Middleware](middleware.md)).
- `req.extensions()` and `req.extensions_mut()` give direct access to the underlying `http::Extensions` storage.

```rust
#[derive(Clone)]
struct User {
    name: String,
}

app.middleware(|mut req: Request, next: Next| async move {
    if req.header("authorization") == Some("Bearer secret") {
        req.set(User { name: "ada".into() });
    }
    next.run(req).await
});

app.get("/me", |req: Request| async move {
    match req.get::<User>() {
        Some(user) => format!("Hello, {}", user.name),
        None => "Not logged in".to_string(),
    }
});
```

## Method reference

| Method | Express | Returns |
|---|---|---|
| `method()` | `req.method` | `&Method` |
| `set_method(method)` | `method-override` | |
| `path()` | `req.path` | `&str` |
| `uri()` | `req.originalUrl` | `&Uri` |
| `version()` | `req.httpVersion` | `Version` |
| `host()` | `req.host` | `Option<&str>` |
| `hostname()` | `req.hostname` | `Option<&str>` |
| `protocol()` | `req.protocol` | `&'static str` (`"http"` or `"https"`) |
| `secure()` | `req.secure` | `bool` |
| `set_uri(uri)` | assign `req.url` | |
| `route()` | `req.route.path` | `Option<&'static str>` |
| `path_for(name, &[...])` | | `Result<String, Error>` (`500`): the path of a named route |
| `url_for(name, &[...])` | | `Result<String, Error>` (`500`): the same, with protocol and host |
| `param(name)` | `req.params.name` | `Option<&str>` |
| `param_as::<T>(name)` | | `Result<T, Error>` (`400`) |
| `params()` | `req.params` | iterator of `(&str, &str)` |
| `query(name)` | `req.query.name` | `Option<Cow<str>>` |
| `query_as::<T>()` | | `Result<T, Error>` (`400`) |
| `query_pairs()` | | iterator of `(Cow<str>, Cow<str>)` |
| `query_string()` | | `Option<&str>` |
| `header(name)` | `req.get(name)` | `Option<&str>` |
| `header_all(name)` | | iterator of `&str` |
| `headers()` / `headers_mut()` | `req.headers` | `&HeaderMap` / `&mut HeaderMap` |
| `content_type()` | `req.get('content-type')` | `Option<&str>` |
| `is(type)` | `req.is(type)` | `bool` |
| `accepts(&[...])` | `req.accepts([...])` | `Option<&str>` (`None`: answer `406`) |
| `accepts_language(&[...])` | `req.acceptsLanguages(...)` | `Option<&str>` |
| `cookie(name)` | `req.cookies.name` | `Option<&str>` |
| `signed_cookie(name)` (feature `session`) | `req.signedCookies.name` | `Option<String>` (`None` if missing or changed) |
| `sign_cookie(cookie)` (feature `session`) | `res.cookie(name, value, { signed: true })` | `Cookie`, to send with `.cookie(...)` |
| `session()` (feature `session`) | `req.session` | `&Session` (`500` without the `session` middleware) |
| `json::<T>().await` | `req.body` + `express.json()` | `Result<T, Error>` |
| `form::<T>().await` | `req.body` + `express.urlencoded()` | `Result<T, Error>` |
| `text().await` | `req.body` + `express.text()` | `Result<String, Error>` |
| `bytes().await` | `req.body` + `express.raw()` | `Result<Bytes, Error>` |
| `multipart()` (feature `multipart`) | `multer` | `Result<Multipart, Error>` (`415`) |
| `take_body()` | `req` (stream) | `Body` |
| `set_body(body)` | | |
| `body_limit()` / `set_body_limit(bytes)` | `express.json({ limit })` | `usize` / |
| `ip()` | `req.ip` (follows `trust_proxy`) | `Option<IpAddr>` |
| `remote_addr()` | `req.socket.remoteAddress` | `Option<SocketAddr>` |
| `state::<T>()` / `try_state::<T>()` | `req.app.locals` | `&T` / `Option<&T>` |
| `get::<T>()` / `set(value)` | `res.locals` | `Option<&T>` / `Option<T>` (the previous value) |
| `extensions()` / `extensions_mut()` | | `&Extensions` / `&mut Extensions` |
| `render(template, context)` (feature `templates`) | `res.render(view, locals)` | `Result<Html<String>, Error>`, see [HTML templates](templates.md) |
