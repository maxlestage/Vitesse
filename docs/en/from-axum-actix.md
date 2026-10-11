# Coming from axum or actix-web

This page is for developers who know [axum](https://crates.io/crates/axum) or [actix-web](https://crates.io/crates/actix-web): for each feature of these two frameworks, it gives the Vitesse equivalent, then lists what Vitesse does differently and what it does not do. If you come from Node.js instead, read [Coming from Express](from-express.md).

## The mental model

axum and actix-web describe what a handler needs in its signature, with **extractors** (`Path`, `Query`, `Json`, `State`…). Vitesse follows Express: a handler receives **one `Request`** and asks it for what it needs, with methods that return a `Result` you can propagate with `?`. Middleware receives the request and `next`, like in Express, instead of being a tower `Layer` or an actix `Transform`.

The same route in the three frameworks:

```rust
// axum
async fn show(Path(id): Path<u32>, State(db): State<Db>) -> Result<Json<User>, AppError> {
    Ok(Json(db.find(id).await?))
}
let app = Router::new().route("/users/{id}", get(show)).with_state(db);
axum::serve(TcpListener::bind("0.0.0.0:3000").await?, app).await?;
```

```rust
// actix-web
#[get("/users/{id}")]
async fn show(id: web::Path<u32>, db: web::Data<Db>) -> Result<web::Json<User>, AppError> {
    Ok(web::Json(db.find(id.into_inner()).await?))
}
HttpServer::new(move || App::new().app_data(web::Data::new(db.clone())).service(show))
    .bind(("0.0.0.0", 3000))?
    .run()
    .await
```

```rust
// Vitesse
async fn show(req: Request) -> vitesse::Result<Json<User>> {
    let id: u32 = req.param_as("id")?;
    let db = req.state::<Db>();
    Ok(Json(db.find(id).await?))
}

let mut app = App::new();
app.state(db);
app.get("/users/:id", show);
app.run(3000)
```

The rest of this page goes feature by feature.

## Application and server

| axum | actix-web | Vitesse |
|---|---|---|
| `Router::new()` | `App::new()` | `App::new()` |
| `axum::serve(listener, app)` | `HttpServer::new(…).bind(…)?.run()` | `app.run(3000)`, or `app.listen(3000).await` in your runtime |
| `.with_graceful_shutdown(signal)` | built in (`SIGTERM`, `Ctrl+C`) | built in; `server.with_graceful_shutdown(signal)` for your own signal |
| (tokio runtime) | `.workers(n)` | `app.workers(n)`, one thread per core by default |
| hyper settings | `.keep_alive(…)` | `app.keep_alive(…)` |
| hyper settings | `.client_request_timeout(…)` | `app.header_timeout(…)` |
| (tower `ConcurrencyLimit`) | `.max_connections(n)` | `app.max_connections(n)` |
| — | `.shutdown_timeout(…)` | `app.shutdown_timeout(…)` |
| `axum-server` with rustls | `.bind_rustls_0_23(…)` | `app.tls(Tls::from_pem_files(…)?)` (`tls` feature) |
| HTTP/2 through hyper | HTTP/2 with TLS | `http2` feature: ALPN over TLS, and h2c "prior knowledge" in clear text |
| — | — | HTTP/3 over QUIC: `app.http3(…)` (`http3` feature) |
| `axum::serve(UnixListener, app)` | `.bind_uds(path)` | `app.listen_unix(path)`, `app.bind_unix(path)` |
| `ConnectInfo<SocketAddr>` | `req.peer_addr()` | `req.remote_addr()` |

See [Server configuration](server.md) and [HTTPS and HTTP/2](https.md).

## Routing

| axum | actix-web | Vitesse |
|---|---|---|
| `.route("/u/{id}", get(h).post(h2))` | `.route("/u/{id}", web::get().to(h))`, `#[get("/u/{id}")]` | `app.get("/u/:id", h).post("/u/:id", h2)` |
| `/{*rest}` | `/{tail:.*}` | `/*rest` |
| — | `/{id:\d+}` | `/:id(\d+)` |
| `/{flight}`, split by hand | `/{from}-{to}` | `/:from-:to` |
| two routes | two resources | `/archive/:year?` (optional parameter) |
| — | `.name("user")`, then `req.url_for("user", ["42"])` | `.name("user")`, then `req.path_for("user", &[&42])` or `req.url_for(…)` |
| `.nest("/api", router)` | `web::scope("/api")` | `app.mount("/api", router)` |
| `.merge(other)` | `.configure(f)` | `app.mount("/", router)` |
| `.fallback(h)` | `.default_service(h)` | `app.fallback(h)` |
| fallback of a nested router | `scope.default_service(h)` | `router.fallback(h)` |
| `MatchedPath` | `req.match_pattern()` | `req.route()` |
| automatic `405` | automatic `405` | automatic `405`, `HEAD` and `OPTIONS` |

Route parameters are written `:id` (as in Express), not `{id}`, and a constraint follows the name in parentheses: `r"/users/:id(\d+)"`. Whatever the order of declaration (actix-web tries its routes in order), a static segment wins over a constrained one, which wins over a plain parameter: `/users/:id(\d+)` and `/users/:name` can coexist. `path_for` returns the path (`/users/42`) and `url_for` the absolute URL; both percent-encode the values and return a `500` error if the name or a value is wrong. See [Routing](routing.md#named-routes-and-url-generation) and [Routers](routers.md).

## Reading requests

Each extractor becomes a method of `Request`:

| axum | actix-web | Vitesse |
|---|---|---|
| `Path<T>` | `web::Path<T>` | `req.param("id")`, `req.param_as::<u32>("id")?` |
| `Query<T>` | `web::Query<T>` | `req.query("page")`, `req.query_as::<T>()?` |
| `Json<T>` | `web::Json<T>` | `req.json::<T>().await?` |
| `Form<T>` | `web::Form<T>` | `req.form::<T>().await?` |
| `String`, `Bytes` | `String`, `Bytes` | `req.text().await?`, `req.bytes().await?` |
| `Body` (stream) | `web::Payload` | `req.take_body()` |
| `Multipart` | `actix-multipart` | `req.multipart()?` (`multipart` feature) |
| `HeaderMap`, `TypedHeader` | `req.headers()` | `req.header("x-api-key")`, `req.headers()` |
| `CookieJar` (axum-extra) | `req.cookie("name")` | `req.cookie("name")` |
| `State<S>` | `web::Data<T>` | `app.state(value)`, then `req.state::<T>()` |
| `Extension<T>` | `web::ReqData<T>` | `req.set(value)` in a middleware, `req.get::<T>()` |
| `DefaultBodyLimit` | `PayloadConfig`, `JsonConfig::limit` | `app.body_limit(n)`, `middleware::body_limit(n)` per route |
| `axum-client-ip` | `connection_info().realip_remote_addr()` | `app.trust_proxy(…)`, then `req.ip()` |
| — | `connection_info().scheme()`, `.host()` | `req.protocol()`, `req.secure()`, `req.host()` |
| — | `Accept` header, `preference()` | `req.accepts(&["html", "json"])`, `req.accepts_language(…)` |
| `OriginalUri`, `Uri` | `req.uri()` | `req.uri()`, `req.path()` |

Since the methods take `&self`, you can read a parameter, then the JSON body, then a header, in any order. See [Reading requests](requests.md).

## Sending responses

| axum | actix-web | Vitesse |
|---|---|---|
| `impl IntoResponse` | `impl Responder` | `impl IntoResponse` |
| `Json(value)` | `HttpResponse::Ok().json(value)` | `Json(value)`, `res::json(value)` |
| `Html(text)` | `.content_type("text/html")` | `Html(text)` |
| `(StatusCode::CREATED, body)` | `HttpResponse::Created().body(…)` | `(201, body)`, `res::status(201).json(…)` |
| `Redirect::to(…)` | `web::Redirect::to(…)` | `Redirect::to(…)` |
| `Body::from_stream(…)` | `.streaming(stream)` | `Body::from_stream(…)` |
| `Sse::new(stream)` | a `text/event-stream` stream | `Sse::new(stream)`, or `sse::channel(n)` |
| `ServeFile`, `ServeDir` (tower-http) | `NamedFile`, `Files` (actix-files) | `res::file(path).await`, `ServeDir`, `app.static_dir(…)` |
| `AppendHeaders`, `[(name, value)]` | `.insert_header(…)` | `.header(name, value)` |
| `WebSocketUpgrade` | `actix-ws` | `app.ws(path, …)`, `ws::Upgrade`, over HTTP/1.1 and HTTP/2 |
| askama, minijinja or tera, by hand | askama or tera, by hand | `app.views("templates")`, then `req.render("page.html", json!({…}))` (`templates` feature) |

See [Sending responses](responses.md), [HTML templates](templates.md) and [WebSocket](websocket.md).

## Cookies and sessions

| axum | actix-web | Vitesse (`session` feature) |
|---|---|---|
| `SignedCookieJar` (axum-extra) and a `Key` | `CookieJar::signed` (`secure-cookies` feature) | `app.cookie_secret(…)`, then `req.sign_cookie(cookie)` and `req.signed_cookie("name")` |
| tower-sessions | actix-session | `middleware::session(store)`, then `req.session()` |
| tower-sessions' stores | `CookieSessionStore`, `RedisSessionStore` | `session::MemoryStore`, or your own `SessionStore` |

```rust
use vitesse::session::MemoryStore;

app.cookie_secret(std::env::var("COOKIE_SECRET").expect("COOKIE_SECRET is not set"));
app.middleware(middleware::session(MemoryStore::new()));
app.get("/visits", |req: Request| async move {
    let session = req.session();
    let visits = session.get::<u32>("visits").unwrap_or(0) + 1;
    session.insert("visits", visits)?;
    Ok::<_, Error>(format!("visit #{visits}"))
});
```

The session is saved, and its cookie sent, only when a handler changes it. `session.regenerate()` after a login and `session.destroy()` at logout work like actix-session's `renew` and `purge`. See [Signed cookies and sessions](sessions.md); tower-sessions also works, [through the `tower` feature](#sessions-with-tower-sessions).

## Errors

| axum | actix-web | Vitesse |
|---|---|---|
| your error type + `impl IntoResponse` | `impl ResponseError` | `vitesse::Error` with `?`, or `impl From<YourError> for vitesse::Error` |
| `HandleErrorLayer` | `ErrorHandlers` middleware | `app.on_error(…)` |
| `CatchPanicLayer` | — | built in: a panic becomes a `500` |

Any `std::error::Error` converts into a `500` whose details are only logged; constructors such as `Error::not_found("…")` choose the status and the message sent to the client. See [Error handling](errors.md).

## Middleware

A Vitesse middleware is an `async` closure or function that receives the request and `next`:

```rust
app.middleware(|req: Request, next: Next| async move {
    let started = std::time::Instant::now();
    let route = req.route().unwrap_or("-");
    let res = next.run(req).await;
    println!("{route} {} in {:?}", res.status_code(), started.elapsed());
    res
});
```

It is the equivalent of axum's `middleware::from_fn` and actix-web's `wrap_fn`. The built-in middlewares cover what usually comes from tower-http or actix's ecosystem:

| axum (tower, tower-http) | actix-web | Vitesse |
|---|---|---|
| `middleware::from_fn(f)` | `.wrap_fn(f)` | `app.middleware(f)` |
| `.route_layer(…)`, per-route `.layer(…)` | `.wrap(…)` on a resource | `handler.with(mw)`, `router.middleware(mw)` |
| `TraceLayer` | `Logger` | `middleware::logger()` |
| `CorsLayer` | `actix-cors` | `middleware::cors()` |
| `CompressionLayer` | `Compress` | `middleware::compress()` (`compress` feature) |
| `RequestDecompressionLayer` | automatic decoding | `middleware::decompress()` (`compress` feature) |
| `TimeoutLayer` | — | `middleware::timeout(duration)` |
| `RequestBodyLimitLayer` | `PayloadConfig` | `middleware::body_limit(n)` |
| `SetRequestIdLayer`, `PropagateRequestIdLayer` | — | `middleware::request_id()` |
| `ValidateRequestHeaderLayer`, `TypedHeader<Authorization<…>>` | `actix-web-httpauth` | `middleware::basic_auth(…)`, `middleware::bearer_auth(…)` |
| `ConcurrencyLimitLayer` + `LoadShedLayer` | — | `middleware::concurrency_limit(n).max_wait(…)` |
| `tower-governor` | `actix-governor` | `middleware::rate_limit(max, period)` |
| `SetResponseHeaderLayer` | `DefaultHeaders` | `middleware::helmet()`, or `res.header(…)` in a middleware |
| `NormalizePathLayer` | `NormalizePath` | built in: `/users/` matches `/users` |
| any tower `Layer` | — | `middleware::layer(layer)` (`tower` feature) |

See [Middleware](middleware.md).

## Bringing tower and axum code along

With the `tower` feature, the two ecosystems meet:

- **tower layers in Vitesse**: `middleware::layer(…)` runs any tower `Layer` (tower-http, tower-governor, your own) as a Vitesse middleware, globally, on a router or on a route. The layer can rewrite the method, URI, headers and body, and Vitesse errors still reach `app.on_error`.
- **Vitesse in axum or hyper**: `app.into_service()` turns a Vitesse application into a tower `Service`. Nest it in an axum router with `Router::nest_service("/legacy", service)`, serve it with hyper, or test it with `tower::ServiceExt::oneshot`.

```rust
use tower_http::cors::CorsLayer;
use vitesse::prelude::*;

let mut api = Router::new();
api.middleware(middleware::layer(CorsLayer::permissive()));
```

### Sessions with tower-sessions

Vitesse has [sessions of its own](#cookies-and-sessions), but [tower-sessions](https://crates.io/crates/tower-sessions), the usual choice with axum, works too, through `middleware::layer`: handy to keep its ready-made stores, or code shared with an axum application. The layer puts a `Session` in the request's extensions and sets the cookie on the response.

```rust
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};
use vitesse::prelude::*;

let mut app = App::new();
app.middleware(middleware::layer(SessionManagerLayer::new(MemoryStore::default())));
app.get("/visits", |req: Request| async move {
    let session = req.get::<Session>().unwrap().clone();
    let visits: u32 = session.get("visits").await?.unwrap_or(0) + 1;
    session.insert("visits", visits).await?;
    Ok::<_, Error>(format!("visit #{visits}"))
});
```

The session cookie is `Secure` by default, so browsers only send it back over HTTPS: serve the app with the `tls` feature or behind a TLS proxy, or call `.with_secure(false)` during development. Use one of its persistent stores (Redis, PostgreSQL, SQLite…) in production: `MemoryStore` forgets everything on restart and is not shared between instances.

## Testing

| axum | actix-web | Vitesse |
|---|---|---|
| `app.oneshot(request)` (tower `ServiceExt`) | `test::init_service` + `test::call_service` | `TestClient::new(app).get("/").await` |
| `axum-test` | `actix-test` | `app.bind("127.0.0.1:0")` for a real port |

The test client builds requests with `.header()`, `.json()`, `.form()` and `.body()`, and reads responses with `.status()`, `.text()` and `.json()`. With the `tower` feature, `app.into_service().oneshot(request)` works too. See [Testing](testing.md).

## What Vitesse does differently

- **No extractors, by design.** A handler takes one `Request`; data comes from its methods, typed with `param_as`, `query_as`, `json` and `form`, and errors go up with `?`. There is nothing to learn about extractor ordering (the body last in axum) or `FromRequest` implementations.
- **The application is frozen when the server starts.** Routes, middleware and state are fixed: nothing is reference-counted per request. State is shared by reference (`req.state::<T>()` returns a `&'static T`), so mutable state uses atomics, locks or a connection pool, as with axum's `State` or actix's `web::Data`.
- **Its own HTTP/1.1 engine**, rather than hyper (axum) or actix-http: it is what puts Vitesse ahead in the [benchmark](performance.md). HTTP/2 uses the h2 crate, the one hyper uses.
- **One thread per core** by default on Linux, like actix-web's workers, while axum usually runs on tokio's multi-threaded runtime.

## What Vitesse does not do (yet)

- **Guards** (actix-web's `guard::Host`, `guard::Header`): route on the host or a header in a middleware instead.
- **WebSocket over HTTP/3**: browsers don't rely on it, and open their WebSockets over HTTP/2 or HTTP/1.1, which [Vitesse serves](websocket.md#websocket-over-http2).
- **HTTP/2 server push**, which browsers have removed, and the **`Upgrade: h2c`** mechanism, which RFC 9113 removed from HTTP/2: h2c with prior knowledge works.
- **0-RTT over HTTP/3**, on purpose: those early requests can be replayed.
- **Other async runtimes** than tokio, as with axum.
- **A stable 1.x API**: Vitesse is at version 0.1.

The [FAQ](faq.md) keeps the list of limitations up to date.
