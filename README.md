# Vitesse ⚡

**Le confort d'Express.js, la vitesse de Rust.**

Vitesse est un framework web minimaliste qui reprend l'API d'Express
(`app.get`, `req.params`, `res.status(201).json(...)`, `app.use`, `Router`,
`express.static`…) en Rust natif, avec son propre moteur HTTP/1.1 sur
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

Requêtes par seconde, et entre parenthèses le temps CPU consommé par le serveur
pour chaque requête (plus c'est bas, mieux c'est) :

| Scénario | Express 5 | Drogon 1.9 (C++) | axum 0.8 | actix-web 4 | **Vitesse** |
|---|---:|---:|---:|---:|---:|
| `GET /` (texte) | 5 921 (180 µs) | 203 244 (9,7 µs) | 168 165 (11,7 µs) | 274 195 (7,1 µs) | **291 415 (6,1 µs)** |
| `GET /json` | 5 765 (184 µs) | 120 489 (16,5 µs) | 165 988 (11,9 µs) | 248 987 (7,9 µs) | **313 438 (5,9 µs)** |
| `GET /json` envoyé par un navigateur (12 en-têtes) | 5 627 (188 µs) | 92 041 (21,7 µs) | 131 710 (15,0 µs) | 179 505 (10,9 µs) | **290 478 (6,8 µs)** |
| `GET /users/:id` (paramètre + JSON) | 5 627 (187 µs) | 98 486 (20,1 µs) | 151 303 (13,1 µs) | 209 173 (9,4 µs) | **304 323 (6,2 µs)** |
| `POST /echo` (lit et renvoie du JSON) | 4 538 (235 µs) | 69 876 (28,4 µs) | 105 570 (18,8 µs) | 170 129 (11,7 µs) | **253 941 (7,6 µs)** |
| `GET /` pipeliné ×16 | 8 352 (128 µs) | 724 059 (2,7 µs) | 213 678 (9,3 µs) | 1 272 510 (1,6 µs) | **2 781 541 (0,64 µs)** |

- **Contre actix-web**, le framework Rust réputé le plus rapide : jusqu'à
  **+62 %** de débit avec une vraie requête de navigateur, +45 à +49 % avec des
  paramètres ou un corps JSON, **2,2 fois plus** en pipeline, et 15 à 59 % de
  CPU en moins par requête.
- **Contre axum** : de 1,7 à 2,4 fois plus de requêtes par seconde, deux fois
  moins de CPU par requête, et 13 fois plus en pipeline.
- **Contre Drogon (C++)** : de 1,4 à 3,8 fois plus rapide.
- **Contre Express** : environ 50 fois plus rapide (et toujours 25 fois plus
  face à Express en cluster sur les mêmes 2 cœurs).

**Pourquoi l'écart avec actix est plus faible sur `GET /` ?** Sur la requête
la plus simple, tous les serveurs rapides butent sur le même plancher : environ
4,7 µs de travail du noyau par requête (lecture, écriture et, sur la boucle
locale, le traitement de la réception côté client, imputé à l'envoi du
serveur). Vitesse n'ajoute que ~1,4 µs par-dessus, actix ~2,4 µs et axum ~7 µs.
Dès que la requête ressemble à une vraie requête (en-têtes de navigateur,
paramètres, corps JSON, pipelining), c'est le code du framework qui fait la
différence, et l'écart se creuse. En production, à travers un vrai réseau,
la part du noyau côté serveur est plus faible : l'avantage de Vitesse n'en est
que plus visible.

<sub>VM 4 vCPU : serveur épinglé sur 2 cœurs, [wrk](https://github.com/wg/wrk)
sur les 2 autres, 128 connexions keep-alive, 10 s par scénario, même machine
et même session pour tous. Node 22.22 / Express 5.3.0, Drogon 1.9.13 (GCC 13,
`-O3`), axum 0.8, actix-web 4.15, Rust 1.97, allocateur système partout. Sans
pipeline, les serveurs les plus rapides saturent wrk : le temps CPU par
requête, mesuré côté serveur, est alors le juge le plus fiable. Les mesures
varient de quelques pourcents d'une exécution à l'autre. Pour reproduire :
`bench/run.sh` (code des serveurs dans `bench/`).</sub>

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
`app.body_limit(octets)`, `app.thread_per_core(false)` (voir plus bas).

## Pourquoi c'est rapide

La quasi-totalité du temps d'une requête simple est passée dans le noyau
(lecture et écriture du socket) : un serveur rapide est un serveur qui ajoute
le moins possible autour. Vitesse fait exactement **un `read` et un `write`
par requête**, et un seul de chaque pour tout un lot de requêtes pipelinées.

- **Un moteur HTTP/1.1 maison** (`src/http1.rs`) :
  - la tête de la requête est analysée par [httparse](https://github.com/seanmonstar/httparse)
    (SIMD) ; les en-têtes ne sont notés que par leur position. La `HeaderMap` et
    l'`Uri` ne sont construites que si un handler les demande : une requête de
    navigateur et ses douze en-têtes coûtent presque autant qu'une requête nue ;
  - les réponses sont sérialisées directement dans un tampon d'écriture
    réutilisé : lignes de statut et types de contenu courants pré-calculés,
    en-tête `Date` en cache par thread, `HeaderMap` créée seulement si on ajoute
    d'autres en-têtes ;
  - un handler qui répond sans attendre suit un chemin entièrement synchrone :
    pas de `Future` intermédiaire ni de copie de grosses structures ;
  - un seul minuteur par connexion (et non par requête) gère l'inactivité.
- **Presque aucune allocation** : les requêtes (et leurs tampons) sont
  recyclées par thread, les tables d'en-têtes des réponses aussi, et le routeur
  écrit les paramètres dans des tampons réutilisés. Il ne reste que le `Future`
  du handler (et le tampon d'un corps JSON).
- **Un thread par cœur** (Linux) : chaque cœur a sa propre boucle d'événements
  et son propre socket `SO_REUSEPORT`, le noyau répartit les connexions et une
  requête ne change jamais de thread. `app.thread_per_core(false)` revient au
  runtime multi-thread de tokio (utile si des handlers font de longs calculs
  bloquants).
- **Un routeur sans regex** : un arbre de segments parcouru sans allocation
  pour les routes statiques ; les paramètres pointent dans le chemin.
- **Zéro compteur atomique partagé par requête** : l'application est figée au
  démarrage (`&'static`), handlers, middlewares et état sont lus sans `Arc`.
- **Peu de copies** : la requête traverse middlewares et handlers en ne
  déplaçant qu'un pointeur, la réponse ne pèse que 72 octets, et le corps
  n'est lu que si le handler le demande.

Le moteur reste robuste : rejet des requêtes ambiguës (`Content-Length` +
`Transfer-Encoding`), limites de taille des en-têtes (431) et du corps (413),
délais d'inactivité, `Expect: 100-continue`, corps `chunked` dans les deux
sens, fermeture différée pour ne pas perdre de réponse, et arrêt propre qui
laisse finir les requêtes en cours.

## Lancer le projet

```sh
cargo run --release --example hello      # Hello World
cargo run --release --example rest_api   # API CRUD complète
cargo test                               # tests unitaires, d'intégration et doctests
bench/run.sh                             # benchmark (wrk, Node.js, et Drogon si installé)
```

### Le site de présentation

Le dossier [`site/`](site) contient le site du projet, écrit en Rust avec
[Yew](https://yew.rs) et compilé en WebAssembly avec
[Trunk](https://trunkrs.dev). Le workflow `Site` le publie sur GitHub Pages à
chaque modification sur `master`.

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
cd site && trunk serve --open            # http://127.0.0.1:8080, rechargement à chaud
```

## Limites actuelles

Vitesse fait volontairement peu de choses, comme Express. Ne sont pas (encore)
inclus : HTTP/2 et TLS (à placer derrière un reverse proxy comme Nginx ou
Caddy, comme on le fait souvent avec Express), WebSocket, compression,
moteurs de templates, et paramètres partiels dans un segment
(`/vols/:de-:vers`).
