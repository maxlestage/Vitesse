# Viniendo de axum o actix-web

Esta página es para quienes conocen [axum](https://crates.io/crates/axum) o [actix-web](https://crates.io/crates/actix-web): para cada funcionalidad de estos dos frameworks, da el equivalente en Vitesse y después enumera lo que Vitesse hace de otra manera y lo que no hace. Si en cambio vienes de Node.js, lee [Viniendo de Express](from-express.md).

## El modelo mental

axum y actix-web describen lo que necesita un handler en su firma, con **extractores** (`Path`, `Query`, `Json`, `State`…). Vitesse sigue a Express: un handler recibe **una sola `Request`** y le pide lo que necesita, con métodos que devuelven un `Result` que puedes propagar con `?`. Un middleware recibe la petición y `next`, como en Express, en lugar de ser una `Layer` de tower o un `Transform` de actix.

La misma ruta en los tres frameworks:

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

El resto de la página avanza funcionalidad por funcionalidad.

## Aplicación y servidor

| axum | actix-web | Vitesse |
|---|---|---|
| `Router::new()` | `App::new()` | `App::new()` |
| `axum::serve(listener, app)` | `HttpServer::new(…).bind(…)?.run()` | `app.run(3000)`, o `app.listen(3000).await` dentro de tu runtime |
| `.with_graceful_shutdown(signal)` | integrado (`SIGTERM`, `Ctrl+C`) | integrado; `server.with_graceful_shutdown(signal)` para tu propia señal |
| (runtime de tokio) | `.workers(n)` | `app.workers(n)`, un hilo por núcleo por defecto |
| ajustes de hyper | `.keep_alive(…)` | `app.keep_alive(…)` |
| ajustes de hyper | `.client_request_timeout(…)` | `app.header_timeout(…)` |
| (`ConcurrencyLimit` de tower) | `.max_connections(n)` | `app.max_connections(n)` |
| — | `.shutdown_timeout(…)` | `app.shutdown_timeout(…)` |
| `axum-server` con rustls | `.bind_rustls_0_23(…)` | `app.tls(Tls::from_pem_files(…)?)` (feature `tls`) |
| HTTP/2 a través de hyper | HTTP/2 con TLS | feature `http2`: ALPN sobre TLS, y h2c «prior knowledge» sin cifrar |
| — | — | HTTP/3 sobre QUIC: `app.http3(…)` (feature `http3`) |
| `axum::serve(UnixListener, app)` | `.bind_uds(path)` | `app.listen_unix(path)`, `app.bind_unix(path)` |
| `ConnectInfo<SocketAddr>` | `req.peer_addr()` | `req.remote_addr()` |

Consulta [Configuración del servidor](server.md) y [HTTPS y HTTP/2](https.md).

## Enrutamiento

| axum | actix-web | Vitesse |
|---|---|---|
| `.route("/u/{id}", get(h).post(h2))` | `.route("/u/{id}", web::get().to(h))`, `#[get("/u/{id}")]` | `app.get("/u/:id", h).post("/u/:id", h2)` |
| `/{*rest}` | `/{tail:.*}` | `/*rest` |
| — | `/{id:\d+}` | `/:id(\d+)` |
| `/{flight}`, dividido a mano | `/{from}-{to}` | `/:from-:to` |
| dos rutas | dos recursos | `/archive/:year?` (parámetro opcional) |
| — | `.name("user")`, y luego `req.url_for("user", ["42"])` | `.name("user")`, y luego `req.path_for("user", &[&42])` o `req.url_for(…)` |
| `.nest("/api", router)` | `web::scope("/api")` | `app.mount("/api", router)` |
| `.merge(other)` | `.configure(f)` | `app.mount("/", router)` |
| `.fallback(h)` | `.default_service(h)` | `app.fallback(h)` |
| fallback de un router anidado | `scope.default_service(h)` | `router.fallback(h)` |
| `MatchedPath` | `req.match_pattern()` | `req.route()` |
| `405` automático | `405` automático | `405`, `HEAD` y `OPTIONS` automáticos |

Los parámetros de ruta se escriben `:id` (como en Express), no `{id}`, y una restricción sigue al nombre entre paréntesis: `r"/users/:id(\d+)"`. Sea cual sea el orden de declaración (actix-web prueba sus rutas en orden), un segmento estático gana a uno con restricción, que gana a un parámetro simple: `/users/:id(\d+)` y `/users/:name` pueden convivir. `path_for` devuelve la ruta (`/users/42`) y `url_for` la URL absoluta; los dos codifican los valores y devuelven un error `500` si el nombre o un valor es incorrecto. Consulta [Enrutamiento](routing.md#rutas-con-nombre-y-generación-de-url) y [Routers](routers.md).

## Leer la petición

Cada extractor se convierte en un método de `Request`:

| axum | actix-web | Vitesse |
|---|---|---|
| `Path<T>` | `web::Path<T>` | `req.param("id")`, `req.param_as::<u32>("id")?`, `req.params_as::<T>()?` |
| `Query<T>` | `web::Query<T>` | `req.query("page")`, `req.query_as::<T>()?` |
| `Json<T>` | `web::Json<T>` | `req.json::<T>().await?`, `req.valid_json::<T>().await?` con [validación](validation.md) |
| `Form<T>` | `web::Form<T>` | `req.form::<T>().await?` |
| `String`, `Bytes` | `String`, `Bytes` | `req.text().await?`, `req.bytes().await?` |
| `Body` (flujo) | `web::Payload` | `req.take_body()` |
| `Multipart` | `actix-multipart` | `req.multipart()?` (feature `multipart`) |
| `HeaderMap`, `TypedHeader` | `req.headers()` | `req.header("x-api-key")`, `req.headers()` |
| `CookieJar` (axum-extra) | `req.cookie("name")` | `req.cookie("name")` |
| `State<S>` | `web::Data<T>` | `app.state(value)`, y luego `req.state::<T>()` |
| `Extension<T>` | `web::ReqData<T>` | `req.set(value)` en un middleware, `req.get::<T>()` |
| `DefaultBodyLimit` | `PayloadConfig`, `JsonConfig::limit` | `app.body_limit(n)`, `middleware::body_limit(n)` por ruta |
| `axum-client-ip` | `connection_info().realip_remote_addr()` | `app.trust_proxy(…)`, y luego `req.ip()` |
| — | `connection_info().scheme()`, `.host()` | `req.protocol()`, `req.secure()`, `req.host()` |
| — | cabecera `Accept`, `preference()` | `req.accepts(&["html", "json"])`, `req.accepts_language(…)` |
| `OriginalUri`, `Uri` | `req.uri()` | `req.uri()`, `req.path()` |

Como los métodos reciben `&self`, puedes leer un parámetro, luego el cuerpo JSON y después una cabecera, en el orden que quieras. Consulta [Leer la petición](requests.md).

## Enviar respuestas

| axum | actix-web | Vitesse |
|---|---|---|
| `impl IntoResponse` | `impl Responder` | `impl IntoResponse` |
| `Json(value)` | `HttpResponse::Ok().json(value)` | `Json(value)`, `res::json(value)` |
| `Html(text)` | `.content_type("text/html")` | `Html(text)` |
| `(StatusCode::CREATED, body)` | `HttpResponse::Created().body(…)` | `(201, body)`, `res::status(201).json(…)` |
| `Redirect::to(…)` | `web::Redirect::to(…)` | `Redirect::to(…)` |
| `Body::from_stream(…)` | `.streaming(stream)` | `Body::from_stream(…)` |
| `Sse::new(stream)` | un flujo `text/event-stream` | `Sse::new(stream)`, o `sse::channel(n)` |
| `ServeFile`, `ServeDir` (tower-http) | `NamedFile`, `Files` (actix-files) | `res::file(path).await`, `ServeDir`, `app.static_dir(…)` |
| `AppendHeaders`, `[(name, value)]` | `.insert_header(…)` | `.header(name, value)` |
| `WebSocketUpgrade` | `actix-ws` | `app.ws(path, …)`, `ws::Upgrade`, sobre HTTP/1.1 y HTTP/2 |
| askama, minijinja o tera, a mano | askama o tera, a mano | `app.views("templates")`, y luego `req.render("page.html", json!({…}))` (feature `templates`) |

Consulta [Responder](responses.md), [Plantillas HTML](templates.md) y [WebSocket](websocket.md).

## Cookies y sesiones

| axum | actix-web | Vitesse (feature `session`) |
|---|---|---|
| `SignedCookieJar` (axum-extra) y una `Key` | `CookieJar::signed` (feature `secure-cookies`) | `app.cookie_secret(…)`, y luego `req.sign_cookie(cookie)` y `req.signed_cookie("name")` |
| tower-sessions | actix-session | `middleware::session(store)`, y luego `req.session()` |
| los almacenes de tower-sessions | `CookieSessionStore`, `RedisSessionStore` | `session::MemoryStore`, o tu propio `SessionStore` |

```rust
use vitesse::session::MemoryStore;

app.cookie_secret(std::env::var("COOKIE_SECRET").expect("COOKIE_SECRET no está definida"));
app.middleware(middleware::session(MemoryStore::new()));
app.get("/visits", |req: Request| async move {
    let session = req.session();
    let visits = session.get::<u32>("visits").unwrap_or(0) + 1;
    session.insert("visits", visits)?;
    Ok::<_, Error>(format!("visita n.º {visits}"))
});
```

La sesión se guarda, y su cookie se envía, solo cuando un handler la modifica. `session.regenerate()` tras un inicio de sesión y `session.destroy()` al cerrar la sesión funcionan como `renew` y `purge` de actix-session. Consulta [Cookies firmadas y sesiones](sessions.md); tower-sessions también funciona, [a través de la feature `tower`](#sesiones-con-tower-sessions).

## Errores

| axum | actix-web | Vitesse |
|---|---|---|
| tu tipo de error + `impl IntoResponse` | `impl ResponseError` | `vitesse::Error` con `?`, o `impl From<YourError> for vitesse::Error` |
| `HandleErrorLayer` | middleware `ErrorHandlers` | `app.on_error(…)` |
| `CatchPanicLayer` | — | integrado: un pánico se convierte en un `500` |

Cualquier `std::error::Error` se convierte en un `500` cuyos detalles solo se registran en el log; constructores como `Error::not_found("…")` eligen el estado y el mensaje que se envía al cliente. Consulta [Manejo de errores](errors.md).

## Middlewares

Un middleware de Vitesse es un closure o una función `async` que recibe la petición y `next`:

```rust
app.middleware(|req: Request, next: Next| async move {
    let started = std::time::Instant::now();
    let route = req.route().unwrap_or("-");
    let res = next.run(req).await;
    println!("{route} {} in {:?}", res.status_code(), started.elapsed());
    res
});
```

Es el equivalente de `middleware::from_fn` en axum y de `wrap_fn` en actix-web. Los middlewares incluidos cubren lo que suele venir de tower-http o del ecosistema de actix:

| axum (tower, tower-http) | actix-web | Vitesse |
|---|---|---|
| `middleware::from_fn(f)` | `.wrap_fn(f)` | `app.middleware(f)` |
| `.route_layer(…)`, `.layer(…)` por ruta | `.wrap(…)` en un recurso | `handler.with(mw)`, `router.middleware(mw)` |
| `TraceLayer` | `Logger` | `middleware::logger()` |
| `CorsLayer` | `actix-cors` | `middleware::cors()` |
| `CompressionLayer` | `Compress` | `middleware::compress()` (feature `compress`) |
| `RequestDecompressionLayer` | decodificación automática | `middleware::decompress()` (feature `compress`) |
| `TimeoutLayer` | — | `middleware::timeout(duration)` |
| `RequestBodyLimitLayer` | `PayloadConfig` | `middleware::body_limit(n)` |
| `SetRequestIdLayer`, `PropagateRequestIdLayer` | — | `middleware::request_id()` |
| `ValidateRequestHeaderLayer`, `TypedHeader<Authorization<…>>` | `actix-web-httpauth` | `middleware::basic_auth(…)`, `middleware::bearer_auth(…)` |
| `ConcurrencyLimitLayer` + `LoadShedLayer` | — | `middleware::concurrency_limit(n).max_wait(…)` |
| `tower-governor` | `actix-governor` | `middleware::rate_limit(max, period)` |
| `SetResponseHeaderLayer` | `DefaultHeaders` | `middleware::helmet()`, o `res.header(…)` en un middleware |
| `NormalizePathLayer` | `NormalizePath` | integrado: `/users/` coincide con `/users` |
| cualquier `Layer` de tower | — | `middleware::layer(layer)` (feature `tower`) |

Consulta [Middlewares](middleware.md).

## Reutilizar código de tower y axum

Con la feature `tower`, los dos ecosistemas se encuentran:

- **Capas de tower en Vitesse**: `middleware::layer(…)` ejecuta cualquier `Layer` de tower (tower-http, tower-governor, las tuyas) como un middleware de Vitesse, de forma global, en un router o en una ruta. La capa puede reescribir el método, la URI, las cabeceras y el cuerpo, y los errores de Vitesse siguen llegando a `app.on_error`.
- **Vitesse dentro de axum o hyper**: `app.into_service()` convierte una aplicación Vitesse en un `Service` de tower. Anídala en un router de axum con `Router::nest_service("/legacy", service)`, sírvela con hyper o pruébala con `tower::ServiceExt::oneshot`.

```rust
use tower_http::cors::CorsLayer;
use vitesse::prelude::*;

let mut api = Router::new();
api.middleware(middleware::layer(CorsLayer::permissive()));
```

### Sesiones con tower-sessions

Vitesse tiene [sus propias sesiones](#cookies-y-sesiones), pero [tower-sessions](https://crates.io/crates/tower-sessions), la opción habitual con axum, también funciona, a través de `middleware::layer`: algo práctico para conservar sus almacenes ya hechos, o código compartido con una aplicación axum. La capa pone una `Session` en las extensiones de la petición y añade la cookie a la respuesta.

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

La cookie de sesión es `Secure` por defecto, así que los navegadores solo la devuelven por HTTPS: sirve la aplicación con la feature `tls` o detrás de un proxy TLS, o llama a `.with_secure(false)` durante el desarrollo. En producción, usa uno de sus almacenes persistentes (Redis, PostgreSQL, SQLite…): `MemoryStore` lo olvida todo al reiniciar y no se comparte entre instancias.

## Pruebas

| axum | actix-web | Vitesse |
|---|---|---|
| `app.oneshot(request)` (`ServiceExt` de tower) | `test::init_service` + `test::call_service` | `TestClient::new(app).get("/").await` |
| `axum-test` | `actix-test` | `app.bind("127.0.0.1:0")` para un puerto real |

El cliente de pruebas construye las peticiones con `.header()`, `.json()`, `.form()` y `.body()`, y lee las respuestas con `.status()`, `.text()` y `.json()`. Con la feature `tower`, `app.into_service().oneshot(request)` también funciona. Consulta [Pruebas](testing.md).

## Lo que Vitesse hace de otra manera

- **Sin extractores, a propósito.** Un handler recibe una sola `Request`; los datos salen de sus métodos, con tipos gracias a `param_as`, `query_as`, `json` y `form`, y los errores suben con `?`. No hay nada que aprender sobre el orden de los extractores (en axum, el cuerpo va el último) ni sobre implementaciones de `FromRequest`.
- **La aplicación se congela cuando arranca el servidor.** Las rutas, los middlewares y el estado quedan fijos: no hay ningún contador de referencias por petición. El estado se comparte por referencia (`req.state::<T>()` devuelve un `&'static T`), así que el estado mutable usa atómicos, bloqueos o un pool de conexiones, como con `State` en axum o `web::Data` en actix.
- **Su propio motor HTTP/1.1**, en lugar de hyper (axum) o actix-http: es lo que pone a Vitesse por delante en el [benchmark](performance.md). HTTP/2 usa el crate h2, el mismo que usa hyper.
- **Un hilo por núcleo** por defecto en Linux, como los workers de actix-web, mientras que axum suele ejecutarse en el runtime multihilo de tokio.

## Lo que Vitesse (todavía) no hace

- **Guards** (`guard::Host`, `guard::Header` en actix-web): enruta según el host o una cabecera en un middleware.
- **WebSocket sobre HTTP/3**: los navegadores no dependen de él, y abren sus WebSockets sobre HTTP/2 o HTTP/1.1, que [Vitesse sirve](websocket.md#websocket-sobre-http2).
- **El server push de HTTP/2**, que los navegadores han eliminado, y el mecanismo **`Upgrade: h2c`**, que la RFC 9113 eliminó de HTTP/2: h2c con prior knowledge funciona.
- **0-RTT sobre HTTP/3**, a propósito: esas peticiones anticipadas se pueden reenviar.
- **Otros runtimes asíncronos** que no sean tokio, igual que axum.
- **Una API 1.x estable**: Vitesse está en la versión 0.1.

Las [preguntas frecuentes](faq.md) mantienen al día la lista de límites.
