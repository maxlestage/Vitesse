# Middlewares

Un middleware est une fonction qui s'exécute autour de vos handlers : il reçoit la requête et `next`, la suite de la chaîne, et peut agir avant le handler, après lui, ou répondre à sa place. Cette page explique comment les écrire, dans quel ordre ils s'exécutent, et présente les middlewares fournis avec Vitesse.

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
- `req.headers_mut()` donne accès aux en-têtes, et `req.set_body(...)` remplace le corps.
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
| `middleware::serve_static(dossier)` | `express.static(dossier)` | fichiers statiques, voir [Fichiers statiques](static-files.md) |

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
