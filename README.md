# Vitesse ⚡

**Le confort d'Express.js, la vitesse de Rust.**

Vitesse est un framework web minimaliste qui reprend l'API d'Express
(`app.get`, `req.params`, `res.status(201).json(...)`, `app.use`, `Router`,
`express.static`…) en Rust natif, sur [hyper](https://hyper.rs) et
[tokio](https://tokio.rs).

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    app.run(3000)
}
```

## Benchmark

Requêtes par seconde (plus c'est haut, mieux c'est) :

| Scénario | Express 5 | Express 5 (cluster ×2) | axum 0.8 | **Vitesse** |
|---|---:|---:|---:|---:|
| `GET /` (texte) | 7 948 | 16 661 | 186 506 | **203 540** (×25,6) |
| `GET /json` | 8 101 | 15 525 | 191 580 | **199 881** (×24,7) |
| `GET /users/:id` (paramètre + JSON) | 7 798 | 15 233 | 195 821 | **203 960** (×26,2) |
| `POST /echo` (parse + renvoie du JSON) | 6 239 | 11 848 | 147 854 | **189 994** (×30,5) |
| Latence p99 | 27 à 39 ms | 18 à 26 ms | ~4,7 ms | **~4,7 ms** |

Le multiplicateur est calculé par rapport à Express seul. À nombre de cœurs
égal (Express en cluster sur 2 processus), Vitesse reste **12 à 16 fois plus
rapide**, et au niveau d'[axum](https://github.com/tokio-rs/axum), la référence
en Rust.

<sub>VM 4 vCPU : serveur épinglé sur 2 cœurs, générateur de charge
[oha](https://github.com/hatoo/oha) sur les 2 autres, 128 connexions keep-alive,
10 s par scénario. Node 22.22 / Express 5.3.0, Rust 1.97. Les serveurs Rust
saturent le générateur de charge bien avant leurs propres cœurs : leurs chiffres
sont un minimum. Pour reproduire : `bench/run.sh`.</sub>

## Installation

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/vitesse" }
serde = { version = "1", features = ["derive"] } # pour vos structures JSON
```

Pas besoin de `#[tokio::main]` : `app.run(port)` crée le runtime, utilise tous
les cœurs et s'arrête proprement sur `Ctrl+C` / `SIGTERM`. Si vous avez déjà
un runtime tokio, utilisez `app.listen(port).await`.

## D'Express à Vitesse

| Express | Vitesse |
|---|---|
| `const app = express()` | `let mut app = App::new();` |
| `app.get('/u/:id', (req, res) => …)` | `app.get("/u/:id", \|req: Request\| async move { … })` |
| `app.post` / `put` / `patch` / `delete` / `all` | idem |
| `app.use(fn)` | `app.middleware(fn)` |
| `app.use('/api', router)` | `app.mount("/api", router)` |
| `express.Router()` | `Router::new()` |
| `express.static('public')` | `app.static_dir("/static", "public")` ou `app.middleware(ServeDir::new("public"))` |
| `express.json()` + `req.body` | `req.json::<T>().await?` |
| `express.urlencoded()` | `req.form::<T>().await?` |
| `next()` | `next.run(req).await` |
| `req.params.id` | `req.param("id")` ou `req.param_as::<u64>("id")?` |
| `req.query.q` | `req.query("q")` ou `req.query_as::<T>()?` |
| `req.get('host')` | `req.header("host")` |
| `req.cookies.session` | `req.cookie("session")` |
| `req.ip` | `req.ip()` |
| `app.locals` | `app.state(valeur)` + `req.state::<T>()` |
| `res.locals.user = …` | `req.set(user)` + `req.get::<User>()` |
| `res.send('texte')` | renvoyer `"texte"` |
| `res.json(obj)` | renvoyer `Json(obj)` ou `json!({...})` |
| `res.status(201).json(obj)` | `res::status(201).json(obj)` ou `(201, Json(obj))` |
| `res.sendStatus(404)` | renvoyer `StatusCode::NOT_FOUND` |
| `res.redirect('/login')` | `Redirect::to("/login")` |
| `res.cookie('a', 'b')` | `Response::new().cookie(Cookie::new("a", "b"))` |
| `res.sendFile(path)` | `res::file(path).await` |
| `res.download(path)` | `res::download(path, "nom.pdf").await` |
| `(err, req, res, next) => …` | `app.on_error(\|err\| …)` |
| `app.listen(3000)` | `app.run(3000)` |

## Guide

### Routes et paramètres

```rust
use vitesse::prelude::*;

let mut app = App::new();

// Une closure…
app.get("/hello/:name", |req: Request| async move {
    format!("Salut {} !", req.param("name").unwrap())
});

// …ou une fonction.
async fn show_user(req: Request) -> vitesse::Result<Json<serde_json::Value>> {
    let id: u64 = req.param_as("id")?; // 400 automatique si ce n'est pas un nombre
    Ok(Json(json!({ "id": id })))
}
app.get("/users/:id", show_user);

// Joker : tout le reste du chemin.
app.get("/files/*path", |req: Request| async move {
    format!("fichier demandé : {}", req.param("path").unwrap())
});
```

- Priorité : route statique > paramètre > joker (`/users/new` passe avant `/users/:id`).
- `/users/` et `/users` sont équivalents ; les paramètres sont décodés (`%C3%A9` → `é`).
- `HEAD` utilise la route `GET`, `OPTIONS` répond `204` avec l'en-tête `Allow`, une
  mauvaise méthode donne `405`, une route inconnue `404 {"error":"Cannot GET /x"}`.
- Une route invalide ou en double fait paniquer au démarrage, avec la ligne fautive.

### Query string et corps

```rust
#[derive(serde::Deserialize)]
struct Search { q: String, page: Option<u32> }

#[derive(serde::Deserialize)]
struct NewUser { name: String }

app.get("/search", |req: Request| async move {
    let s: Search = req.query_as()?;            // 400 si invalide
    let tri = req.query("sort");                // Option<Cow<str>>
    Ok::<_, Error>(format!("{} p.{} tri={tri:?}", s.q, s.page.unwrap_or(1)))
});

app.post("/users", |req: Request| async move {
    let user: NewUser = req.json().await?;      // 400 si JSON invalide, 413 si trop gros
    Ok::<_, Error>((201, Json(json!({ "name": user.name }))))
});
```

Le corps n'est lu que si vous le demandez (`json`, `form`, `text`, `bytes`), dans la
limite de `app.body_limit(octets)` (1 Mio par défaut). Pour le streaming (upload,
proxy), utilisez `req.take_body()`.

### Réponses

Un handler peut renvoyer tout ce qui implémente `IntoResponse` :

| Type renvoyé | Réponse |
|---|---|
| `&'static str`, `String` | `200`, `text/plain` |
| `Json(valeur)`, `json!({...})` | `200`, `application/json` |
| `Html("<h1>…</h1>")` | `200`, `text/html` |
| `(201, x)`, `(StatusCode::CREATED, x)` | `x` avec ce statut |
| `StatusCode::NO_CONTENT` | ce statut |
| `Option<T>` | `T`, ou `404` si `None` |
| `Result<T, E>` | `T`, ou l'erreur |
| `Redirect::to("/")` | `302` + `Location` |
| `Response` | ce que vous avez construit |

Et pour tout contrôler, le builder façon Express :

```rust
app.get("/custom", |_| async {
    res::status(202)
        .header("x-powered-by", "Vitesse")
        .cookie(Cookie::new("vu", "1").http_only(true))
        .json(json!({ "ok": true }))
});
```

### Erreurs

`vitesse::Error` porte un statut HTTP et devient `{"error": "message"}`.
N'importe quelle erreur Rust se convertit avec `?` en `500` (sans exposer le
détail au client, qui est seulement journalisé). Les paniques sont rattrapées
et deviennent aussi des `500` : le serveur ne tombe jamais.

```rust
async fn get_todo(req: Request) -> vitesse::Result<Json<Todo>> {
    let id: u64 = req.param_as("id")?;
    let todo = db.find(id).ok_or_else(|| Error::not_found("tâche introuvable"))?;
    Ok(Json(todo))
}

// Personnaliser toutes les réponses d'erreur (404 compris) :
app.on_error(|err: Error| {
    res::status(err.status()).html(format!("<h1>Oups</h1><p>{}</p>", err.message()))
});
```

### Middlewares

Un middleware reçoit la requête et `next` ; il peut agir avant, après, ou
répondre à la place de la suite :

```rust
// Global (app.use), dans l'ordre d'ajout, y compris pour les 404.
app.middleware(|req: Request, next: Next| async move {
    let debut = std::time::Instant::now();
    let res = next.run(req).await;
    res.header("x-response-time", format!("{:?}", debut.elapsed()))
});

// Sur une seule route :
async fn auth(mut req: Request, next: Next) -> Response {
    match req.header("authorization") {
        Some("Bearer secret") => {
            req.set(User { name: "ada".into() }); // dispo via req.get::<User>()
            next.run(req).await
        }
        _ => Error::unauthorized("connectez-vous").into_response(),
    }
}
app.get("/admin", admin_page.with(auth));
```

Middlewares fournis (`vitesse::middleware`) :

| Middleware | Équivalent Express |
|---|---|
| `middleware::logger()` | `morgan('dev')` |
| `middleware::cors()` (configurable) | `cors()` |
| `middleware::helmet()` | `helmet()` |
| `middleware::timeout(durée)` | `connect-timeout` |
| `middleware::serve_static(dossier)` | `express.static()` |

### Routeurs

```rust
let mut api = Router::new();
api.middleware(auth);                 // ne s'applique qu'aux routes de ce routeur
api.get("/users", list_users)
   .post("/users", create_user)
   .get("/users/:id", show_user);

let mut app = App::new();
app.mount("/api/v1", api);            // GET /api/v1/users, …
```

Les routeurs s'imbriquent (`router.mount(...)`), et leurs préfixes peuvent
contenir des paramètres (`/users/:id/posts`).

### État partagé

```rust
use std::sync::atomic::{AtomicU64, Ordering};

app.state(AtomicU64::new(0));          // ou un pool de base de données, une config…
app.get("/visites", |req: Request| async move {
    let n = req.state::<AtomicU64>().fetch_add(1, Ordering::Relaxed);
    format!("visite n°{}", n + 1)
});
```

### Fichiers statiques

```rust
app.static_dir("/assets", "./public");           // GET /assets/*
app.middleware(ServeDir::new("./www"));          // à la racine, puis les routes
app.serve_dir("/docs", ServeDir::new("./site").max_age(Duration::from_secs(3600)));
```

Types MIME, `index.html`, `ETag`/`Last-Modified` (`304`), requêtes partielles
(`Range`, pour la vidéo), streaming des gros fichiers, et protection contre
`../` et les fichiers cachés (`.env`, `.git`).

### Tests sans réseau

```rust
use vitesse::test::TestClient;

#[tokio::test]
async fn hello() {
    let client = TestClient::new(build_app());

    let res = client.get("/hello/Ada").await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "Salut Ada !");

    let res = client.post("/users").json(&json!({ "name": "Ada" })).await;
    assert_eq!(res.status(), 201);
}
```

### Serveur

```rust
app.run(3000);                         // runtime géré, arrêt propre sur Ctrl+C / SIGTERM
app.run("127.0.0.1:8080");
app.listen(3000).await;                // dans votre propre runtime tokio

let server = app.bind("127.0.0.1:0").await?;   // port choisi par l'OS
println!("http://{}", server.local_addr());
server.with_graceful_shutdown(signal).await?;
```

Réglages : `app.workers(n)` (threads, un par cœur par défaut),
`app.body_limit(octets)`, `app.thread_per_core(true)` (voir plus bas).

## Pourquoi c'est rapide

- **Natif et multi-cœur** : le socle HTTP/1.1 est hyper sur tokio, compilé avec
  LTO. Keep-alive et pipelining (réponses regroupées en un seul `write`).
- **Routeur sans regex** : un arbre de segments parcouru sans allocation pour
  les routes statiques ; les paramètres pointent dans le chemin et ne sont
  copiés que s'ils contiennent des `%XX`.
- **Zéro compteur atomique par requête** : l'application est figée au
  démarrage (`&'static`). Handlers, middlewares et état sont lus sans `Arc`,
  donc sans ping-pong de lignes de cache entre cœurs.
- **Une allocation par handler** pour son `Future`, et la récupération des
  paniques n'en ajoute aucune.
- **Rien de superflu** : le corps n'est lu que si le handler le demande, la
  query string est décodée à la demande, sans copie quand c'est possible.
- **Option thread-par-cœur** (`app.thread_per_core(true)`, Linux) : une boucle
  d'événements et un socket `SO_REUSEPORT` par cœur, aucune synchronisation
  entre threads. Un peu moins de CPU par requête à pleine charge, mais sans
  rééquilibrage de la charge entre threads ; d'où un défaut sur le runtime
  multi-thread de tokio.

## Lancer le projet

```sh
cargo run --release --example hello      # Hello World
cargo run --release --example rest_api   # API CRUD complète
cargo test                               # tests unitaires, d'intégration et doctests
bench/run.sh                             # benchmark (nécessite oha et Node.js)
```

## Limites actuelles

Vitesse fait volontairement peu de choses, comme Express. Ne sont pas (encore)
inclus : HTTP/2 et TLS (à placer derrière un reverse proxy comme Nginx ou
Caddy, comme on le fait souvent avec Express), WebSocket, compression,
moteurs de templates, et paramètres partiels dans un segment
(`/vols/:de-:vers`).
