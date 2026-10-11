# Middlewares

Un middleware est une fonction qui s'exécute autour de vos handlers : il reçoit la requête et `next`, la suite de la chaîne, et peut agir avant le handler, après lui, ou répondre à sa place. Cette page explique comment les écrire, dans quel ordre ils s'exécutent, présente les middlewares fournis avec Vitesse, et montre comment utiliser les couches tower de l'écosystème d'axum.

## Votre premier middleware

En Express, un middleware reçoit `(req, res, next)` et appelle `next()`. Avec Vitesse, il reçoit `(req, next)`, passe la requête à la suite avec `next.run(req).await`, et **renvoie** la réponse :

```js
app.use((req, res, next) => {
  console.log(`${req.method} ${req.path}`);
  next();
});
```

```rust
use vitesse::prelude::*;

let mut app = App::new();
app.middleware(|req: Request, next: Next| async move {
    println!("{} {}", req.method(), req.path());
    next.run(req).await
});
```

`next.run(req)` prend possession de la requête et rend la `Response` produite par la suite de la chaîne. Comme il consomme `next`, on ne peut l'appeler qu'une fois : le compilateur écarte les bugs du type « `next()` appelé deux fois ».

## Avant et après le handler

Le code placé avant `next.run` s'exécute à l'aller, celui placé après s'exécute au retour, avec la réponse en main :

```rust
use std::time::Instant;

app.middleware(|req: Request, next: Next| async move {
    let start = Instant::now();
    let res = next.run(req).await;
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    res.header("x-response-time", format!("{ms:.3}ms"))
});
```

Pour modifier la réponse, utilisez les méthodes du builder (`.header(...)`, `.status(...)`) ou les méthodes « en place » sur une réponse `mut` (`set_header`, `set_status`, `headers_mut()`), décrites dans [Répondre](responses.md).

## Court-circuiter la chaîne

Pour arrêter la chaîne, renvoyez une réponse sans appeler `next.run` :

```rust
app.middleware(|req: Request, next: Next| async move {
    if req.header("x-api-key") != Some("secret") {
        return Error::unauthorized("clé d'API manquante").into_response();
    }
    next.run(req).await
});
```

Un middleware peut renvoyer tout ce qui implémente `IntoResponse`, y compris un `vitesse::Result<Response>`, ce qui permet d'utiliser `?` :

```rust
async fn require_json(req: Request, next: Next) -> vitesse::Result<Response> {
    if req.method() == Method::POST && !req.is("json") {
        return Err(Error::new(415, "envoyez du JSON"));
    }
    Ok(next.run(req).await)
}

app.middleware(require_json);
```

## Modifier la requête

Prenez la requête en `mut req` pour la modifier avant de la transmettre :

- `req.set(valeur)` attache une donnée que les handlers lisent avec `req.get::<T>()` (l'équivalent de `res.locals` ou de `req.user` ; le type doit implémenter `Clone`). Voir [État partagé](state.md).
- `req.headers_mut()` donne accès aux en-têtes, `req.set_body(...)` remplace le corps, et `req.set_body_limit(...)` change la taille maximale du corps que le handler peut lire (voir [Taille maximale du corps](requests.md#taille-maximale-du-corps-et-413)).
- `req.set_method(...)` change la méthode, pour [surcharger la méthode](requests.md#surcharger-la-méthode).
- `req.set_uri(...)` réécrit l'URL. Les middlewares globaux s'exécutent *avant* le routage : c'est donc le nouveau chemin qui est routé.

```rust
app.middleware(|mut req: Request, next: Next| async move {
    if req.path().starts_with("/old/") {
        let new = req.uri().to_string().replacen("/old/", "/new/", 1);
        if let Ok(uri) = new.parse() {
            req.set_uri(uri);
        }
    }
    next.run(req).await
});
```

Un middleware peut aussi lire le corps (`req.bytes().await`, `req.json().await`…), par exemple pour vérifier la signature d'un webhook : le corps est mis en cache, et le handler peut le relire.

## Ordre d'exécution

Les middlewares globaux s'exécutent dans l'ordre où ils ont été ajoutés, comme les couches d'un oignon :

```rust
app.middleware(a);
app.middleware(b);
app.get("/", handler);
// a (aller) → b (aller) → handler → b (retour) → a (retour)
```

La chaîne complète d'une requête est : **middlewares globaux → middlewares du [routeur](routers.md) → middlewares de la route (`.with`) → handler**. Les middlewares d'un routeur s'exécutent aussi, après les middlewares globaux, pour les requêtes sous son préfixe auxquelles aucune route ne répond : le `404`, le `405` et l'`OPTIONS` automatique (voir [Routeurs et sous-applications](routers.md#les-requêtes-auxquelles-aucune-route-ne-répond)).

> [!IMPORTANT]
> Contrairement à `app.use()` en Express, la position de `app.middleware(...)` par rapport à vos routes n'a pas d'importance : un middleware global enveloppe **toutes** les requêtes, y compris les routes déclarées avant lui, les `404` et les `405`. Le gestionnaire [`app.on_error`](errors.md) est toujours la couche la plus externe.

## Un middleware pour certaines routes seulement

Il y a trois façons de limiter un middleware à certaines routes :

- **Une route** : `.with(middleware)` (du trait `HandlerExt`, inclus dans le prélude). Les appels s'enchaînent et s'exécutent de gauche à droite. Entourez une closure de parenthèses avant d'appeler `.with` :

```rust
#[derive(Clone)]
struct CurrentUser {
    name: String,
}

async fn auth(mut req: Request, next: Next) -> Response {
    match req.header("authorization") {
        Some("Bearer secret") => {
            req.set(CurrentUser { name: "ada".into() });
            next.run(req).await
        }
        _ => Error::unauthorized("connectez-vous").into_response(),
    }
}

async fn dashboard(req: Request) -> String {
    let user = req.get::<CurrentUser>().unwrap();
    format!("Bonjour {}", user.name)
}

app.get("/dashboard", dashboard.with(auth));
app.get("/health", (|_| async { "ok" }).with(auth));
```

- **Un groupe de routes** : placez-les dans un `Router` et appelez `router.middleware(...)`. Comme `router.use()` en Express, il couvre tout le préfixe du routeur, `404` compris (voir [Routeurs et sous-applications](routers.md)).
- **Un préfixe de chemin** : testez le chemin dans un middleware global :

```rust
app.middleware(|req: Request, next: Next| async move {
    if req.path().starts_with("/admin") && req.header("x-admin") != Some("1") {
        return Error::forbidden("réservé aux administrateurs").into_response();
    }
    next.run(req).await
});
```

## Middlewares fournis

Le module `vitesse::middleware` (accessible en `middleware::` avec le prélude) fournit les grands classiques :

| Middleware | Équivalent Express | Rôle |
|---|---|---|
| `middleware::logger()` | `morgan('dev')` | journalise chaque requête |
| `middleware::cors()` | `cors()` | en-têtes CORS et requêtes préliminaires |
| `middleware::helmet()` | `helmet()` | en-têtes de sécurité |
| `middleware::timeout(durée)` | `connect-timeout` | `503` quand une requête est trop longue |
| `middleware::body_limit(octets)` | `express.json({ limit })` | taille maximale du corps pour une route ou un routeur |
| `middleware::request_id()` | `express-request-id` | un identifiant pour chaque requête, dans `X-Request-Id` |
| `middleware::basic_auth(identifiants)` | `express-basic-auth` | authentification HTTP Basic (nom d'utilisateur et mot de passe) |
| `middleware::bearer_auth(jeton)` | `passport-http-bearer` | jetons `Authorization: Bearer` |
| `middleware::concurrency_limit(max)` | | plafonne le nombre de requêtes traitées en même temps |
| `middleware::rate_limit(max, période)` | `express-rate-limit` | requêtes par client et par période, `429` au-delà |
| `middleware::serve_static(dossier)` | `express.static(dossier)` | fichiers statiques, voir [Fichiers statiques](static-files.md) |
| `middleware::compress()` | `compression()` | réponses en Brotli ou gzip (fonctionnalité `compress`) |
| `middleware::decompress()` | | corps de requête compressés (fonctionnalité `compress`) |
| `middleware::layer(couche)` | | n'importe quelle couche tower (fonctionnalité `tower`), voir [Couches tower](#couches-tower) |

### `logger`

Affiche une ligne par requête sur la sortie standard : méthode, URL, statut et durée. Le statut est coloré quand la sortie est un terminal.

```rust
app.middleware(middleware::logger());
```

```text
GET /users/42 200 0.084 ms
```

Ajoutez-le en premier pour qu'il mesure toute la chaîne.

### `cors`

Sans option, `middleware::cors()` se comporte comme `cors()` en Express : toutes les origines sont autorisées (`Access-Control-Allow-Origin: *`) pour les méthodes `GET, HEAD, PUT, PATCH, POST, DELETE`. Chaque option s'enchaîne :

```rust
use std::time::Duration;

app.middleware(
    middleware::cors()
        .allow_origin("https://app.example.com")
        .allow_origin("https://admin.example.com")
        .allow_methods([Method::GET, Method::POST])
        .allow_headers("content-type, authorization")
        .expose_headers("x-total-count")
        .allow_credentials(true)
        .max_age(Duration::from_secs(600)),
);
```

| Option | Effet |
|---|---|
| `allow_origin(origine)` | n'autorise que les origines listées (appelable plusieurs fois ; `"*"` garde toutes les origines) |
| `allow_methods(méthodes)` | méthodes envoyées dans `Access-Control-Allow-Methods` |
| `allow_headers(liste)` | `Access-Control-Allow-Headers` (par défaut, renvoie les en-têtes demandés par le navigateur) |
| `expose_headers(liste)` | en-têtes de réponse que le JavaScript a le droit de lire |
| `allow_credentials(true)` | ajoute `Access-Control-Allow-Credentials: true` (cookies et authentification), à combiner avec `allow_origin` |
| `max_age(durée)` | durée pendant laquelle le navigateur peut garder en cache la réponse préliminaire |

Quand l'origine de la requête fait partie de la liste, Vitesse la renvoie dans `Access-Control-Allow-Origin` et ajoute `Vary: Origin`. Sans aucun `allow_origin`, la réponse est toujours `*`, même avec `allow_credentials(true)`, exactement comme Express.

> [!IMPORTANT]
> Les navigateurs refusent les requêtes avec identifiants (cookies, `Authorization`) quand la réponse est `*`. Pour utiliser des cookies ou une authentification entre origines, listez explicitement vos origines de confiance avec `.allow_origin(...)`, une fois par origine. Vitesse ne renvoie volontairement jamais une origine quelconque : n'importe quel site pourrait alors lire les réponses d'un utilisateur connecté.

Les requêtes sans en-tête `Origin` passent sans être modifiées. Les requêtes préliminaires (`OPTIONS` avec `Access-Control-Request-Method`) reçoivent directement un `204`, sans atteindre vos routes. Ajoutez `cors()` à l'application ou à un [routeur](routers.md), qui couvre aussi les requêtes préliminaires de ses routes ; sur une seule route (`.with`), il ne voit jamais la requête préliminaire, à laquelle l'`OPTIONS` automatique répond sans en-têtes CORS. Quand une origine n'est pas autorisée, Vitesse n'ajoute aucun en-tête CORS et le navigateur bloque la réponse : CORS protège les navigateurs des utilisateurs, il ne remplace pas une authentification.

> [!TIP]
> Ajoutez `cors()` avant votre middleware d'authentification : ainsi, les réponses d'erreur (un `401`, par exemple) portent aussi les en-têtes CORS, et le JavaScript de votre front-end peut les lire.

### `helmet`

Ajoute les en-têtes de sécurité habituels, sans écraser ceux que votre handler a déjà posés :

```text
x-content-type-options: nosniff
x-frame-options: SAMEORIGIN
x-dns-prefetch-control: off
x-download-options: noopen
x-permitted-cross-domain-policies: none
x-xss-protection: 0
referrer-policy: no-referrer
cross-origin-opener-policy: same-origin
strict-transport-security: max-age=31536000; includeSubDomains
```

### `timeout`

Interrompt les requêtes qui dépassent la durée indiquée et répond `503 {"error":"request timed out"}` :

```rust
app.middleware(middleware::timeout(Duration::from_secs(10)));
```

Le `Future` du handler est abandonné à son prochain `.await`. Un code qui bloque le thread sans jamais attendre (un long calcul, `std::thread::sleep`) ne peut pas être interrompu.

### `body_limit`

Change la taille maximale du corps que les handlers peuvent lire, pour les routes qu'il couvre : une route avec `.with`, ou tout un routeur. Le reste de l'application garde `app.body_limit` (1 Mio par défaut) :

```rust
app.post("/upload", upload.with(middleware::body_limit(100 * 1024 * 1024))); // 100 Mio
```

Au-delà de la limite, la lecture du corps échoue avec `413 Payload Too Large`. Tous les détails sont dans [Taille maximale du corps et 413](requests.md#taille-maximale-du-corps-et-413).

### `request_id`

Donne un identifiant à chaque requête, pour la suivre dans vos journaux et d'un service à l'autre :

```rust
use vitesse::middleware::RequestId;

app.middleware(middleware::request_id());
app.get("/", |req: Request| async move {
    let id = req.get::<RequestId>().unwrap();
    println!("[{id}] page d'accueil");
    "bonjour"
});
```

- Un `X-Request-Id` reçu avec la requête (posé par votre proxy, ou par le service qui vous appelle) est conservé s'il a l'air valide : de 1 à 200 caractères ASCII visibles. Sinon, Vitesse génère un UUID v4 (`9b2f3c1e-4a7d-4e1b-…`).
- L'identifiant est placé dans l'en-tête de la requête (transmettez-le quand vous appelez d'autres services), dans `req.get::<RequestId>()` (`RequestId(pub String)`, qui s'affiche avec `{}`) et dans l'en-tête de la réponse, `404` et erreurs compris.
- `.header("x-correlation-id")` utilise un autre en-tête. `.trust_incoming(false)` génère toujours un nouvel identifiant : c'est utile quand les clients joignent l'application directement et que vous ne voulez pas de leurs identifiants dans vos journaux.

Ajoutez-le avant les middlewares qui journalisent, pour qu'ils puissent lire l'identifiant. Il identifie les requêtes, ce n'est pas un secret : ne l'utilisez pas comme jeton.

### `basic_auth` et `bearer_auth`

L'authentification HTTP, pour un back-office ou une API interne. `basic_auth` demande un nom d'utilisateur et un mot de passe :

```rust
use vitesse::middleware::BasicUser;

let password = std::env::var("ADMIN_PASSWORD").expect("ADMIN_PASSWORD n'est pas défini");

let mut admin = Router::new();
admin.middleware(middleware::basic_auth(("admin".to_string(), password)).realm("Back-office"));
admin.get("/", |req: Request| async move {
    let user = req.get::<BasicUser>().unwrap();
    format!("Bonjour {}", user.0)
});
app.mount("/admin", admin);
```

Sans identifiants valides, la réponse est une `401` avec `WWW-Authenticate: Basic realm="Back-office", charset="UTF-8"`, et le navigateur affiche sa fenêtre de connexion. Une fois l'utilisateur connecté, son nom se trouve dans `req.get::<BasicUser>()`. Les identifiants peuvent être :

| Identifiants | Exemple |
|---|---|
| une paire | `("admin", "secret")`, avec des `&'static str` ou des `String` |
| plusieurs paires | `[("admin", "s3cret"), ("ada", "lovelace")]`, ou un `Vec` de paires |
| une closure | `\|user: &str, password: &str\| user == "ada" && check(password)` |

`bearer_auth` vérifie un en-tête `Authorization: Bearer <jeton>`, pour les API :

```rust
let token = std::env::var("API_TOKEN").expect("API_TOKEN n'est pas défini");

let mut api = Router::new();
api.middleware(middleware::bearer_auth(token));
api.get("/stats", |_| async { Json(json!({ "users": 42 })) });
app.mount("/api", api);
```

Il accepte un jeton (`&'static str` ou `String`), un tableau ou un `Vec` de jetons, ou une closure `|token: &str| -> bool`. Sans jeton, la réponse est une `401` avec `WWW-Authenticate: Bearer` ; avec un mauvais jeton, `WWW-Authenticate: Bearer error="invalid_token"`.

- Les mots de passe et les jetons sont comparés en temps constant, pour que le temps de réponse ne les trahisse pas. `middleware::constant_time_eq(a, b)` fait la même chose dans votre propre code.
- Les closures sont synchrones : pour chercher les utilisateurs dans une base de données, écrivez votre propre middleware (voir [Court-circuiter la chaîne](#court-circuiter-la-chaîne)).
- Sur un routeur, l'authentification couvre aussi les `404` sous son préfixe : un visiteur sans identifiants n'apprend pas quelles pages existent.

> [!IMPORTANT]
> L'authentification Basic envoie le mot de passe avec chaque requête, simplement encodé en base64, et un jeton peut être réutilisé par quiconque le voit : ne les utilisez qu'en HTTPS ([HTTPS et HTTP/2](https.md), ou un proxy qui termine TLS).

### `concurrency_limit`

Plafonne le nombre de requêtes traitées en même temps par ce qu'il couvre, pour protéger une route coûteuse ou une base de données fragile :

```rust
use std::time::Duration;

// Au plus 4 rapports à la fois ; les autres attendent jusqu'à 2 s, puis reçoivent une 503.
app.get(
    "/report",
    report.with(middleware::concurrency_limit(4).max_wait(Duration::from_secs(2))),
);
```

- Au plus `max` requêtes sont à l'intérieur en même temps ; les suivantes attendent leur tour.
- `.max_wait(durée)` plafonne cette attente : au-delà, la réponse est `503 Service Unavailable` avec `Retry-After: 1`. Avec `Duration::ZERO`, les requêtes sont refusées dès que la limite est atteinte, ce qui garde le serveur réactif pendant un pic.
- Chaque middleware compte de son côté : avec `app.middleware(...)`, la limite couvre toute l'application ; avec `.with(...)`, cette route seulement.

Cela limite les requêtes en cours, d'où qu'elles viennent. `app.max_connections` (voir [Configuration du serveur](server.md#délais-et-connexions)) limite plutôt les connexions ouvertes, inactives comprises.

### `rate_limit`

Limite chaque client à `max` requêtes par `période`, comme `express-rate-limit` :

```rust
use std::time::Duration;

app.middleware(middleware::rate_limit(100, Duration::from_secs(60))); // 100 requêtes par minute
app.post("/login", login.with(middleware::rate_limit(5, Duration::from_secs(60)))); // freine les tentatives pour deviner un mot de passe
```

- Un client peut envoyer une rafale de `max` requêtes, puis une toutes les `période / max` (l'algorithme GCRA) : avec `rate_limit(60, Duration::from_secs(60))`, 60 d'un coup, puis une par seconde.
- Au-delà, la réponse est `429 Too Many Requests` avec `Retry-After` (en secondes). Chaque réponse qui passe par le middleware porte `X-RateLimit-Limit` et `X-RateLimit-Remaining`.
- Par défaut, un client est une adresse IP, `req.ip()`. Derrière un reverse proxy, réglez [`app.trust_proxy`](requests.md#derrière-un-proxy--trust_proxy), sinon tous les visiteurs partagent le compteur du proxy.
- `.key(...)` identifie les clients autrement, par clé d'API ou par identifiant d'utilisateur, par exemple. Quand la closure renvoie `None`, la requête n'est pas limitée :

```rust
let mut api = Router::new();
api.middleware(
    middleware::rate_limit(10, Duration::from_secs(1))
        .key(|req: &Request| req.header("x-api-key").map(str::to_owned)),
);
```

Les compteurs vivent dans la mémoire du processus et sont nettoyés automatiquement. Avec plusieurs instances de l'application (plusieurs serveurs ou conteneurs), chacune compte de son côté.

### `compress`

Compresse les réponses en Brotli, ou en gzip quand le navigateur n'accepte pas Brotli, comme `compression()` d'Express. Il demande la fonctionnalité Cargo `compress` :

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["compress"] }
```

```rust
app.middleware(middleware::compress());
```

Seuls les corps qui en valent la peine sont compressés : HTML, CSS, JavaScript, JSON, XML, SVG, WebAssembly et les autres types textuels, d'au moins 1 Kio, de taille connue (pas en flux : les [Server-Sent Events](responses.md#server-sent-events) passent donc tels quels, eux aussi) et pas déjà encodés. Les images, les vidéos et les archives passent tels quels. La réponse reçoit `Content-Encoding` et `Vary: Accept-Encoding`, et la compression tourne sur un thread bloquant : les workers continuent de répondre aux autres requêtes. Ajoutez-le avant les autres middlewares, pour que chaque réponse passe par lui.

### `decompress`

Dans l'autre sens : certains clients (applications mobiles, autres services, expéditeurs de journaux) compressent les corps qu'ils envoient. `decompress()` les décode avant que vos handlers ne les lisent avec `req.json()`, `req.text()` et les autres. Il est fourni par la même fonctionnalité `compress` :

```rust
app.middleware(middleware::decompress());
```

- Les corps envoyés avec `Content-Encoding: gzip` (ou `x-gzip`), `br` ou `deflate` sont décompressés, puis `Content-Encoding` est retiré : le handler voit le corps d'origine. Les requêtes sans `Content-Encoding` passent sans être modifiées.
- Un autre encodage reçoit une `415 Unsupported Media Type`, et des données corrompues une `400`.
- Le corps compressé et le corps décompressé sont tous deux soumis à la [limite de taille](requests.md#taille-maximale-du-corps-et-413) (`413` au-delà) : une petite « bombe zip » ne peut donc pas remplir la mémoire.
- Le corps est lu en entier, puis décompressé sur un thread bloquant : les workers continuent de répondre aux autres requêtes.

## Écrire un middleware réutilisable

La forme la plus simple est une `async fn`, comme `auth` plus haut. Pour un middleware configurable, écrivez une fonction qui renvoie `impl Middleware`. Le trait `Middleware` n'est pas dans le prélude : importez-le depuis `vitesse`.

```rust
use vitesse::Middleware;

fn api_key(expected: &'static str) -> impl Middleware {
    move |req: Request, next: Next| async move {
        if req.header("x-api-key") != Some(expected) {
            return Error::unauthorized("clé d'API invalide").into_response();
        }
        next.run(req).await
    }
}

app.middleware(api_key("ma-cle-secrete"));
```

Si la configuration n'est pas `Copy` (une `String`, un `Vec`…), clonez-la dans le `Future` de chaque requête : `move |req: Request, next: Next| { let value = value.clone(); async move { /* ... */ } }`.

Pour plus de contrôle, implémentez le trait sur une structure. Le middleware vit aussi longtemps que l'application (`&'static self`) : son `Future` peut donc emprunter ses champs sans `Arc` ni clone.

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use vitesse::{BoxFuture, Middleware};

struct RequestCounter {
    total: AtomicU64,
}

impl Middleware for RequestCounter {
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response> {
        Box::pin(async move {
            let n = self.total.fetch_add(1, Ordering::Relaxed) + 1;
            next.run(req).await.header("x-request-number", n)
        })
    }
}

app.middleware(RequestCounter { total: AtomicU64::new(0) });
```

`next.run(req)` renvoie déjà un `BoxFuture<Response>` : quand vous n'avez pas besoin de toucher à la réponse, renvoyez-le directement, sans `Box::pin`.

> [!NOTE]
> Une panique dans un middleware écrit sous forme de closure ou d'`async fn` devient un `500` qui repasse par les middlewares extérieurs et par `app.on_error`, exactement comme une panique dans un handler. Un `impl Middleware` écrit à la main n'a pas ce traitement : ses paniques sont rattrapées plus haut dans la chaîne (voir [Les paniques](errors.md#les-paniques)).

## Couches tower

axum, hyper et tonic partagent un écosystème de middlewares : [tower](https://docs.rs/tower) et [tower-http](https://docs.rs/tower-http). Avec la fonctionnalité `tower`, `middleware::layer(...)` exécute n'importe quelle `Layer` tower comme un middleware Vitesse :

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tower"] }
tower-http = { version = "0.6", features = ["cors", "set-header", "trace"] }
```

```rust
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use vitesse::http::{HeaderName, HeaderValue};
use vitesse::prelude::*;

let mut app = App::new();
app.middleware(middleware::layer(TraceLayer::new_for_http()));
app.middleware(middleware::layer(SetResponseHeaderLayer::overriding(
    HeaderName::from_static("x-powered-by"),
    HeaderValue::from_static("Vitesse"),
)));

let mut api = Router::new();
api.middleware(middleware::layer(CorsLayer::permissive()));
api.get("/users", |_| async { Json(vec!["ada", "grace"]) });
app.mount("/api", api);
```

Une couche se place partout où se place un middleware : `app.middleware(...)`, `router.middleware(...)` et `handler.with(...)`, dans le même [ordre](#ordre-dexécution) que les autres. `TraceLayer` passe par la crate [tracing](https://docs.rs/tracing) : ses lignes n'apparaissent qu'une fois un subscriber installé, `tracing_subscriber::fmt::init()` par exemple.

Toute couche dont le service accepte une `http::Request<vitesse::Body>` et renvoie une `http::Response` fonctionne : la plupart de tower-http (`CorsLayer`, `SetResponseHeaderLayer`, `ValidateRequestHeaderLayer`, `RequestBodyLimitLayer`, `TraceLayer`…), les couches de tower lui-même (`ConcurrencyLimitLayer`, `MapRequestLayer`…) et des crates comme tower-governor.

Comment les deux mondes se rejoignent :

- **La requête.** La couche voit une `http::Request` construite à partir de la requête Vitesse. Tout ce qu'elle modifie (méthode, URI, en-têtes, extensions, corps) est reporté sur la requête que reçoit la suite de la chaîne, routage compris si la couche est globale.
- **La réponse.** Elle revient sous forme de `Response` Vitesse, que les middlewares autour de la couche peuvent encore modifier.
- **Les erreurs.** Une erreur renvoyée par le service tower (le `Timeout` de tower en renvoie une, par exemple) devient une `500`, journalisée sans être montrée au client. Les erreurs produites par Vitesse (une `404`, le `Err` d'un handler) traversent la couche et arrivent toujours jusqu'à [`app.on_error`](errors.md).
- **Un seul service pour toutes les requêtes.** Le service est construit une fois, au démarrage, puis cloné pour chaque requête, comme dans axum. L'état que partagent ses clones est partagé par toutes les requêtes : un `ConcurrencyLimitLayer` limite vraiment toute la route.

> [!TIP]
> Chaque couche convertit la requête en `http::Request`, puis dans l'autre sens, ce qui coûte un peu. Pour ce que Vitesse fournit déjà (CORS, délais, compression, limites, authentification), les middlewares intégrés sont le choix le plus rapide ; les couches sont là pour tout le reste.

Dans l'autre sens, `app.into_service()` transforme toute une application Vitesse en `Service` tower : servez-la avec hyper, imbriquez-la dans un routeur axum (`Router::nest_service`), ou testez-la avec `tower::ServiceExt` (voir [Tester comme un service tower](testing.md#tester-comme-un-service-tower)).
