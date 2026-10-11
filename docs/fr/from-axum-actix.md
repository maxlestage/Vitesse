# Venir d'axum ou d'actix-web

Cette page s'adresse aux développeurs qui connaissent [axum](https://crates.io/crates/axum) ou [actix-web](https://crates.io/crates/actix-web) : pour chaque fonctionnalité de ces deux frameworks, elle donne l'équivalent dans Vitesse, puis liste ce que Vitesse fait autrement et ce qu'il ne fait pas. Si vous venez plutôt de Node.js, lisez [Venir d'Express](from-express.md).

## Le modèle mental

axum et actix-web décrivent ce dont un handler a besoin dans sa signature, avec des **extracteurs** (`Path`, `Query`, `Json`, `State`…). Vitesse suit Express : un handler reçoit **une seule `Request`** et lui demande ce dont il a besoin, avec des méthodes qui renvoient un `Result` que vous pouvez propager avec `?`. Un middleware reçoit la requête et `next`, comme en Express, au lieu d'être une `Layer` tower ou un `Transform` actix.

La même route dans les trois frameworks :

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

La suite de la page passe en revue chaque fonctionnalité.

## Application et serveur

| axum | actix-web | Vitesse |
|---|---|---|
| `Router::new()` | `App::new()` | `App::new()` |
| `axum::serve(listener, app)` | `HttpServer::new(…).bind(…)?.run()` | `app.run(3000)`, ou `app.listen(3000).await` dans votre runtime |
| `.with_graceful_shutdown(signal)` | intégré (`SIGTERM`, `Ctrl+C`) | intégré ; `server.with_graceful_shutdown(signal)` pour votre propre signal |
| (runtime tokio) | `.workers(n)` | `app.workers(n)`, un thread par cœur par défaut |
| options de hyper | `.keep_alive(…)` | `app.keep_alive(…)` |
| options de hyper | `.client_request_timeout(…)` | `app.header_timeout(…)` |
| (`ConcurrencyLimit` de tower) | `.max_connections(n)` | `app.max_connections(n)` |
| — | `.shutdown_timeout(…)` | `app.shutdown_timeout(…)` |
| `axum-server` avec rustls | `.bind_rustls_0_23(…)` | `app.tls(Tls::from_pem_files(…)?)` (feature `tls`) |
| HTTP/2 via hyper | HTTP/2 avec TLS | feature `http2` : ALPN sur TLS, et h2c « prior knowledge » en clair |
| — | — | HTTP/3 sur QUIC : `app.http3(…)` (feature `http3`) |
| `axum::serve(UnixListener, app)` | `.bind_uds(path)` | `app.listen_unix(path)`, `app.bind_unix(path)` |
| `ConnectInfo<SocketAddr>` | `req.peer_addr()` | `req.remote_addr()` |

Voir [Configuration du serveur](server.md) et [HTTPS et HTTP/2](https.md).

## Routage

| axum | actix-web | Vitesse |
|---|---|---|
| `.route("/u/{id}", get(h).post(h2))` | `.route("/u/{id}", web::get().to(h))`, `#[get("/u/{id}")]` | `app.get("/u/:id", h).post("/u/:id", h2)` |
| `/{*rest}` | `/{tail:.*}` | `/*rest` |
| `.nest("/api", router)` | `web::scope("/api")` | `app.mount("/api", router)` |
| `.merge(other)` | `.configure(f)` | `app.mount("/", router)` |
| `.fallback(h)` | `.default_service(h)` | `app.fallback(h)` |
| fallback d'un routeur imbriqué | `scope.default_service(h)` | `router.fallback(h)` |
| `MatchedPath` | `req.match_pattern()` | `req.route()` |
| `405` automatique | `405` automatique | `405`, `HEAD` et `OPTIONS` automatiques |

Les paramètres de route s'écrivent `:id` (comme en Express), et non `{id}`. Voir [Routage](routing.md) et [Routeurs et sous-applications](routers.md).

## Lire la requête

Chaque extracteur devient une méthode de `Request` :

| axum | actix-web | Vitesse |
|---|---|---|
| `Path<T>` | `web::Path<T>` | `req.param("id")`, `req.param_as::<u32>("id")?` |
| `Query<T>` | `web::Query<T>` | `req.query("page")`, `req.query_as::<T>()?` |
| `Json<T>` | `web::Json<T>` | `req.json::<T>().await?` |
| `Form<T>` | `web::Form<T>` | `req.form::<T>().await?` |
| `String`, `Bytes` | `String`, `Bytes` | `req.text().await?`, `req.bytes().await?` |
| `Body` (flux) | `web::Payload` | `req.take_body()` |
| `Multipart` | `actix-multipart` | `req.multipart()?` (feature `multipart`) |
| `HeaderMap`, `TypedHeader` | `req.headers()` | `req.header("x-api-key")`, `req.headers()` |
| `CookieJar` (axum-extra) | `req.cookie("name")` | `req.cookie("name")` |
| `State<S>` | `web::Data<T>` | `app.state(value)`, puis `req.state::<T>()` |
| `Extension<T>` | `web::ReqData<T>` | `req.set(value)` dans un middleware, `req.get::<T>()` |
| `DefaultBodyLimit` | `PayloadConfig`, `JsonConfig::limit` | `app.body_limit(n)`, `middleware::body_limit(n)` par route |
| `axum-client-ip` | `connection_info().realip_remote_addr()` | `app.trust_proxy(…)`, puis `req.ip()` |
| — | `connection_info().scheme()`, `.host()` | `req.protocol()`, `req.secure()`, `req.host()` |
| — | en-tête `Accept`, `preference()` | `req.accepts(&["html", "json"])`, `req.accepts_language(…)` |
| `OriginalUri`, `Uri` | `req.uri()` | `req.uri()`, `req.path()` |

Comme ces méthodes prennent `&self`, vous pouvez lire un paramètre, puis le corps JSON, puis un en-tête, dans l'ordre que vous voulez. Voir [Lire la requête](requests.md).

## Répondre

| axum | actix-web | Vitesse |
|---|---|---|
| `impl IntoResponse` | `impl Responder` | `impl IntoResponse` |
| `Json(value)` | `HttpResponse::Ok().json(value)` | `Json(value)`, `res::json(value)` |
| `Html(text)` | `.content_type("text/html")` | `Html(text)` |
| `(StatusCode::CREATED, body)` | `HttpResponse::Created().body(…)` | `(201, body)`, `res::status(201).json(…)` |
| `Redirect::to(…)` | `web::Redirect::to(…)` | `Redirect::to(…)` |
| `Body::from_stream(…)` | `.streaming(stream)` | `Body::from_stream(…)` |
| `Sse::new(stream)` | un flux `text/event-stream` | `Sse::new(stream)`, ou `sse::channel(n)` |
| `ServeFile`, `ServeDir` (tower-http) | `NamedFile`, `Files` (actix-files) | `res::file(path).await`, `ServeDir`, `app.static_dir(…)` |
| `AppendHeaders`, `[(name, value)]` | `.insert_header(…)` | `.header(name, value)` |
| `WebSocketUpgrade` | `actix-ws` | `app.ws(path, …)`, `ws::Upgrade` |

Voir [Répondre](responses.md) et [WebSocket](websocket.md).

## Erreurs

| axum | actix-web | Vitesse |
|---|---|---|
| votre type d'erreur + `impl IntoResponse` | `impl ResponseError` | `vitesse::Error` avec `?`, ou `impl From<YourError> for vitesse::Error` |
| `HandleErrorLayer` | middleware `ErrorHandlers` | `app.on_error(…)` |
| `CatchPanicLayer` | — | intégré : une panique devient une `500` |

Toute `std::error::Error` se convertit en une `500` dont le détail est seulement journalisé ; les constructeurs comme `Error::not_found("…")` choisissent le statut et le message envoyé au client. Voir [Gestion des erreurs](errors.md).

## Middlewares

Un middleware Vitesse est une closure ou une fonction `async` qui reçoit la requête et `next` :

```rust
app.middleware(|req: Request, next: Next| async move {
    let started = std::time::Instant::now();
    let route = req.route().unwrap_or("-");
    let res = next.run(req).await;
    println!("{route} {} in {:?}", res.status_code(), started.elapsed());
    res
});
```

C'est l'équivalent de `middleware::from_fn` dans axum et de `wrap_fn` dans actix-web. Les middlewares intégrés couvrent ce qui vient d'habitude de tower-http ou de l'écosystème d'actix :

| axum (tower, tower-http) | actix-web | Vitesse |
|---|---|---|
| `middleware::from_fn(f)` | `.wrap_fn(f)` | `app.middleware(f)` |
| `.route_layer(…)`, `.layer(…)` par route | `.wrap(…)` sur une ressource | `handler.with(mw)`, `router.middleware(mw)` |
| `TraceLayer` | `Logger` | `middleware::logger()` |
| `CorsLayer` | `actix-cors` | `middleware::cors()` |
| `CompressionLayer` | `Compress` | `middleware::compress()` (feature `compress`) |
| `RequestDecompressionLayer` | décodage automatique | `middleware::decompress()` (feature `compress`) |
| `TimeoutLayer` | — | `middleware::timeout(duration)` |
| `RequestBodyLimitLayer` | `PayloadConfig` | `middleware::body_limit(n)` |
| `SetRequestIdLayer`, `PropagateRequestIdLayer` | — | `middleware::request_id()` |
| `ValidateRequestHeaderLayer`, `TypedHeader<Authorization<…>>` | `actix-web-httpauth` | `middleware::basic_auth(…)`, `middleware::bearer_auth(…)` |
| `ConcurrencyLimitLayer` + `LoadShedLayer` | — | `middleware::concurrency_limit(n).max_wait(…)` |
| `tower-governor` | `actix-governor` | `middleware::rate_limit(max, period)` |
| `SetResponseHeaderLayer` | `DefaultHeaders` | `middleware::helmet()`, ou `res.header(…)` dans un middleware |
| `NormalizePathLayer` | `NormalizePath` | intégré : `/users/` correspond à `/users` |
| n'importe quelle `Layer` tower | — | `middleware::layer(layer)` (feature `tower`) |

Voir [Middlewares](middleware.md).

## Réutiliser votre code tower et axum

Avec la feature `tower`, les deux écosystèmes se rejoignent :

- **Les couches tower dans Vitesse** : `middleware::layer(…)` exécute n'importe quelle `Layer` tower (tower-http, tower-governor, les vôtres) comme un middleware Vitesse, pour toute l'application, sur un routeur ou sur une route. La couche peut réécrire la méthode, l'URI, les en-têtes et le corps, et les erreurs de Vitesse arrivent toujours jusqu'à `app.on_error`.
- **Vitesse dans axum ou hyper** : `app.into_service()` transforme une application Vitesse en `Service` tower. Imbriquez-la dans un routeur axum avec `Router::nest_service("/legacy", service)`, servez-la avec hyper, ou testez-la avec `tower::ServiceExt::oneshot`.

```rust
use tower_http::cors::CorsLayer;
use vitesse::prelude::*;

let mut api = Router::new();
api.middleware(middleware::layer(CorsLayer::permissive()));
```

### Sessions avec tower-sessions

Vitesse n'a pas de stockage de sessions à lui, mais [tower-sessions](https://crates.io/crates/tower-sessions), le choix habituel avec axum, fonctionne grâce à `middleware::layer` : la couche place une `Session` dans les extensions de la requête et pose le cookie sur la réponse.

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

Le cookie de session est `Secure` par défaut : les navigateurs ne le renvoient donc qu'en HTTPS. Servez l'application avec la feature `tls` ou derrière un proxy TLS, ou appelez `.with_secure(false)` pendant le développement. En production, utilisez l'un de ses stockages persistants (Redis, PostgreSQL, SQLite…) : `MemoryStore` oublie tout au redémarrage et n'est pas partagé entre les instances.

## Tests

| axum | actix-web | Vitesse |
|---|---|---|
| `app.oneshot(request)` (`ServiceExt` de tower) | `test::init_service` + `test::call_service` | `TestClient::new(app).get("/").await` |
| `axum-test` | `actix-test` | `app.bind("127.0.0.1:0")` pour un vrai port |

Le client de test construit les requêtes avec `.header()`, `.json()`, `.form()` et `.body()`, et lit les réponses avec `.status()`, `.text()` et `.json()`. Avec la feature `tower`, `app.into_service().oneshot(request)` fonctionne aussi. Voir [Tests](testing.md).

## Ce que Vitesse fait autrement

- **Pas d'extracteurs, par choix.** Un handler prend une seule `Request` ; les données viennent de ses méthodes, typées avec `param_as`, `query_as`, `json` et `form`, et les erreurs remontent avec `?`. Rien à apprendre sur l'ordre des extracteurs (le corps en dernier dans axum) ni sur les implémentations de `FromRequest`.
- **L'application est figée au démarrage du serveur.** Les routes, les middlewares et l'état ne bougent plus : aucun compteur de références n'est manipulé à chaque requête. L'état est partagé par référence (`req.state::<T>()` renvoie une `&'static T`), donc un état modifiable passe par des atomiques, des verrous ou un pool de connexions, comme avec le `State` d'axum ou le `web::Data` d'actix.
- **Son propre moteur HTTP/1.1**, plutôt que hyper (axum) ou actix-http : c'est lui qui place Vitesse en tête du [benchmark](performance.md). HTTP/2 utilise le crate h2, celui qu'utilise hyper.
- **Un thread par cœur** par défaut sous Linux, comme les workers d'actix-web, alors qu'axum tourne généralement sur le runtime multi-thread de tokio.

## Ce que Vitesse ne fait pas (encore)

- **Les routes nommées et la génération d'URL** (le `url_for` d'actix-web).
- **Les expressions régulières et les guards dans les routes** (`{id:\d+}` et `guard::Host` dans actix-web) : vérifiez le paramètre dans le handler (`req.param_as::<u32>("id")?` répond déjà `400`) ou aiguillez selon l'hôte dans un middleware.
- **Les sessions et les cookies signés intégrés** : utilisez [tower-sessions](#sessions-avec-tower-sessions) avec la feature `tower`, comme vous le feriez avec axum.
- **WebSocket sur HTTP/2 ou HTTP/3**, et le server push HTTP/2 : WebSocket fonctionne en HTTP/1.1, que tous les navigateurs utilisent pour cela.
- **D'autres runtimes asynchrones** que tokio, comme avec axum.
- **Une API 1.x stable** : Vitesse en est à la version 0.1.

La [FAQ](faq.md) tient à jour la liste des limites.
