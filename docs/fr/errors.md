# Gestion des erreurs

Avec Vitesse, les erreurs sont des valeurs comme les autres : un handler renvoie `Err(...)`, le plus souvent grâce à l'opérateur `?`, et l'erreur devient une réponse HTTP avec le bon statut. Cette page présente le type `Error`, les conversions automatiques, les pages d'erreur personnalisées, les `404` et les paniques.

## Le type `Error`

`vitesse::Error` porte trois informations :

- un **statut** HTTP ;
- un **message**, envoyé au client en JSON : `{"error": "message"}` ;
- éventuellement une **source** : l'erreur d'origine, écrite dans les journaux du serveur mais jamais envoyée au client.

| Constructeur | Statut |
|---|---|
| `Error::bad_request(msg)` | `400 Bad Request` |
| `Error::unauthorized(msg)` | `401 Unauthorized` |
| `Error::forbidden(msg)` | `403 Forbidden` |
| `Error::not_found(msg)` | `404 Not Found` |
| `Error::conflict(msg)` | `409 Conflict` |
| `Error::payload_too_large()` | `413 Payload Too Large` |
| `Error::unprocessable(msg)` | `422 Unprocessable Entity` |
| `Error::internal(msg)` | `500 Internal Server Error` |
| `Error::new(statut, msg)` | n'importe quel statut : `Error::new(418, "je suis une théière")` |
| `Error::from_status(statut)` | n'importe quel statut, avec sa raison standard comme message (`Service Unavailable`) |

`.with_source(err)` attache la cause d'origine. Sur une erreur existante, `status()`, `message()` et `source()` lisent ses composantes, et `Error` implémente `Display` (`500 Internal Server Error (cause)`) pour vos journaux.

## Renvoyer une erreur depuis un handler

Faites renvoyer à votre handler un `vitesse::Result<T>`, alias de `Result<T, vitesse::Error>`, et utilisez `?` :

```rust
use vitesse::prelude::*;

// `find_item` est votre propre fonction, qui renvoie une `Option<Item>`.
async fn show_item(req: Request) -> vitesse::Result<Json<Item>> {
    let id: u64 = req.param_as("id")?; // 400 si `id` n'est pas un nombre
    let item = find_item(id).ok_or_else(|| Error::not_found(format!("article {id} introuvable")))?;
    Ok(Json(item))
}
```

```http
GET /items/42 HTTP/1.1

HTTP/1.1 404 Not Found
content-type: application/json

{"error":"article 42 introuvable"}
```

Dans une closure, Rust ne peut pas deviner le type d'erreur : annotez le `Ok` final.

```rust
app.get("/double/:n", |req: Request| async move {
    let n: i64 = req.param_as("n")?;
    Ok::<_, Error>(format!("{}", n * 2))
});
```

Une `Error` peut aussi être renvoyée directement (`async { Error::new(418, "je suis une théière") }`), et renvoyer `None` depuis un handler qui renvoie une `Option` produit un `404 {"error":"Not Found"}`.

Par rapport à Express, il n'y a ni `next(err)` ni exception à rattraper : l'erreur voyage dans la valeur de retour, et le compilateur s'assure qu'elle est traitée.

## `?` avec d'autres types d'erreur

N'importe quelle erreur standard (`std::error::Error + Send + Sync + 'static` : erreurs d'E/S, de parsing, `serde_json`, pilotes de base de données…) se convertit automatiquement avec `?`. Elle devient un `500 {"error":"Internal Server Error"}` : le détail n'est **pas** envoyé au client (il pourrait divulguer des informations sensibles), mais écrit sur la sortie d'erreur du serveur :

```rust
app.get("/parse", |_| async {
    let n: u32 = "abc".parse()?; // ParseIntError -> 500
    Ok::<_, Error>(n.to_string())
});
```

```text
[vitesse] error 500: invalid digit found in string
```

Quand le client mérite un meilleur message, convertissez l'erreur vous-même avec `map_err` :

```rust
app.get("/config", |_| async {
    let text = std::fs::read_to_string("config.toml")
        // 500 avec un message clair ; l'erreur d'E/S est gardée comme source et journalisée.
        .map_err(|e| Error::internal("configuration indisponible").with_source(e))?;
    let port: u16 = text.trim().parse()
        // 400 : c'est la faute du client... dans cet exemple.
        .map_err(|_| Error::bad_request("numéro de port attendu"))?;
    Ok::<_, Error>(format!("port {port}"))
});
```

> [!NOTE]
> `Box<dyn std::error::Error + Send + Sync>` et `anyhow::Error` n'implémentent pas eux-mêmes `std::error::Error` : `?` ne peut donc pas les convertir. `with_source` les accepte en revanche : `.map_err(|e| Error::internal("...").with_source(e))?`.

### Les erreurs produites par Vitesse

| Situation | Réponse |
|---|---|
| `req.param_as` échoue | `400` : `invalid parameter 'id': 'abc'` ou `missing parameter 'id'` |
| `req.query_as` échoue | `400` : `invalid query string: ...` |
| `req.json` / `req.form` sur un corps invalide | `400` : `invalid JSON: ...` / `invalid form data: ...` |
| `req.text` sur un corps qui n'est pas de l'UTF-8 | `400` : `the body is not valid UTF-8` |
| Corps plus gros que `app.body_limit` | `413 Payload Too Large` |
| Aucune route ne correspond | `404`, `{"error":"Cannot GET /chemin"}` |
| Le chemin existe, mais pas pour cette méthode | `405 Method Not Allowed`, avec un en-tête `Allow` |
| Un handler panique | `500 Internal Server Error` |
| `middleware::timeout` expire | `503`, `request timed out` |

## Vos propres types d'erreur

Pour utiliser `?` avec vos erreurs métier, faites-les correspondre à une `vitesse::Error`. Deux approches, selon que votre type implémente `std::error::Error` ou non.

**Votre type n'implémente pas `std::error::Error`** : implémentez `From<VotreErreur> for vitesse::Error`, et `?` fait la conversion :

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
            ShopError::OutOfStock(id) => Error::conflict(format!("article {id} en rupture de stock")),
            ShopError::InvalidQuantity => Error::unprocessable("quantité invalide"),
            ShopError::Database(e) => Error::from_status(500).with_source(e),
        }
    }
}

// `reserve` est votre logique métier : fn reserve(id: u64) -> Result<(), ShopError>
async fn order(req: Request) -> vitesse::Result<&'static str> {
    let id: u64 = req.param_as("id")?;
    reserve(id)?; // ShopError -> vitesse::Error
    Ok("réservé")
}
```

**Votre type implémente `std::error::Error`** (à la main ou avec `thiserror`) : le `From` ci-dessus entrerait en conflit avec la conversion automatique (qui transforme toute erreur standard en `500`). Implémentez plutôt `IntoResponse`, et renvoyez un `Result<T, VotreErreur>` :

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
            ApiError::InvalidId => write!(f, "identifiant invalide"),
            ApiError::NotFound(id) => write!(f, "note {id} introuvable"),
            ApiError::Io(e) => write!(f, "erreur d'E/S : {e}"),
        }
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let err = match self {
            ApiError::InvalidId => Error::bad_request("identifiant invalide"),
            ApiError::NotFound(id) => Error::not_found(format!("note {id} introuvable")),
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
> Construisez la réponse à partir d'une `vitesse::Error`, comme ci-dessus, plutôt qu'à la main : elle garde ainsi le format JSON habituel, la source est journalisée, et elle passe par `app.on_error`.

## Des pages d'erreur personnalisées : `app.on_error`

Par défaut, les erreurs sont envoyées sous la forme `{"error": "message"}`. `app.on_error` remplace ce format pour toute l'application, comme le middleware d'erreur `(err, req, res, next)` d'Express :

```rust
app.on_error(|err: Error| {
    if let Some(source) = err.source() {
        eprintln!("{} {} : {source}", err.status().as_u16(), err.message());
    }
    res::status(err.status()).json(json!({
        "error": { "status": err.status().as_u16(), "message": err.message() }
    }))
});
```

La fonction reçoit l'`Error` et renvoie tout ce qui implémente `IntoResponse`. Elle est appelée pour **chaque réponse issue d'une `Error`** : erreurs renvoyées par les handlers et les middlewares, `404` par défaut, `405`, erreurs de lecture du corps, `413`, paniques, délais dépassés, fichiers statiques introuvables. Elle ne s'applique pas aux réponses que vous construisez vous-même, comme `(404, "pas ici")` ou `StatusCode::NOT_FOUND`. Les en-têtes posés par vos middlewares (CORS, par exemple) sont conservés.

> [!WARNING]
> Définissez le statut vous-même, comme avec `res::status(err.status())` : si vous renvoyez simplement `Html(...)`, la page d'erreur part avec un `200 OK`. De plus, une fois `on_error` défini, la source des `500` n'est plus journalisée automatiquement : journalisez-la dans votre gestionnaire, comme ci-dessus.

`on_error` ne reçoit pas la requête. Si votre format en dépend (du HTML pour les navigateurs, du JSON pour le reste), écrivez un middleware global qui inspecte la réponse avec `res.take_error()` :

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

## Une page 404 personnalisée : `app.fallback`

Quand aucune route ne correspond, Vitesse appelle le handler de repli, qui répond par défaut `404 {"error":"Cannot GET /chemin"}`. Remplacez-le avec `app.fallback`, qui prend un handler ordinaire :

```rust
app.fallback(|req: Request| async move {
    (404, Html(format!("<h1>Page introuvable</h1><p>{} n'existe pas.</p>", req.path())))
});
```

Les middlewares globaux s'exécutent aussi pour le handler de repli. Il n'est pas appelé quand le chemin existe avec une autre méthode (c'est un `405`). S'il renvoie une `Error`, la réponse passe par `on_error`. Pour les applications monopages, voir [Fichiers statiques](static-files.md).

## Les paniques

Une panique dans un handler (un `unwrap()` sur `None`, un indice hors limites, `req.state::<T>()` pour un type jamais enregistré…) ne fait pas tomber le serveur : Vitesse la rattrape et répond `500 {"error":"Internal Server Error"}`. Rust affiche le message de la panique sur la sortie d'erreur, les autres requêtes continuent normalement, et le `500` traverse vos middlewares et `on_error` comme n'importe quelle autre erreur.

Préférez malgré tout `?` et des erreurs explicites : une panique est un bug, pas une façon de répondre.

> [!WARNING]
> Les paniques ne peuvent être rattrapées qu'avec la stratégie par défaut (*unwinding*). Avec `panic = "abort"` dans un `[profile]` de votre `Cargo.toml`, une panique arrête tout le processus. Par ailleurs, une panique survenue dans un *middleware* est rattrapée tout en haut de la chaîne : le client reçoit bien un `500`, mais les middlewares extérieurs et `on_error` ne s'exécutent pas pour cette requête.
