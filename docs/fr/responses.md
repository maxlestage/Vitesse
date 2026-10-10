# Répondre

Dans Vitesse, un handler n'écrit pas dans un objet `res` : il **renvoie** sa réponse. Tout ce qui implémente le trait `IntoResponse` peut être renvoyé : une chaîne, `Json(...)`, un tuple `(statut, corps)`, un `Result`, ou une `Response` complète construite avec les helpers `res`, calqués sur Express.

## Renvoyer sa réponse

Avec Express, vous appelez une méthode de `res`. Avec Vitesse, la valeur renvoyée par le handler *est* la réponse :

```js
app.get('/', (req, res) => res.send('Hello World!'));
app.post('/users', (req, res) => res.status(201).json({ id: 1 }));
```

```rust
use vitesse::prelude::*;

let mut app = App::new();
app.get("/", |_| async { "Hello World!" });
app.post("/users", |_| async { (201, Json(json!({ "id": 1 }))) });
```

Le compilateur vérifie que chaque branche du code produit une réponse : impossible d'oublier de répondre, et l'erreur « headers already sent » d'Express n'existe pas.

## Ce qu'un handler peut renvoyer

| Type renvoyé | Statut | `Content-Type` |
|---|---|---|
| `&'static str`, `String`, `Cow<'static, str>` | `200` | `text/plain; charset=utf-8` |
| `Json(valeur)` (tout type `Serialize`) | `200` | `application/json` |
| `serde_json::Value` (la macro `json!`) | `200` | `application/json` |
| `Html(corps)` | `200` | `text/html; charset=utf-8` |
| `Bytes`, `Vec<u8>`, `&'static [u8]` | `200` | `application/octet-stream` |
| `()` | `200` | aucun (corps vide) |
| `StatusCode` | ce statut | `text/plain`, avec la raison en corps (`Not Found`) |
| `(statut, T)` | `statut` | celui de `T` |
| `Option<T>` | `T`, ou `404` si `None` | |
| `Result<T, E>` | `T` ou `E` (tous deux implémentent `IntoResponse`) | |
| `Error` | le statut de l'erreur | `application/json` : `{"error": "..."}` |
| `Redirect` | `301`, `302`, `303` ou `307` | aucun, en-tête `Location` |
| `Body` | `200` | aucun (voir [Streaming](#streaming)) |
| `Response` | ce que vous avez construit | |
| `http::Response<Body>` | inchangée | |

```rust
use serde::Serialize;
use vitesse::prelude::*;

#[derive(Serialize)]
struct User {
    id: u32,
    name: String,
}

let mut app = App::new();
app.get("/text", |_| async { "Bonjour !" });
app.get("/sum", |_| async { format!("1 + 1 = {}", 1 + 1) });
app.get("/user", |_| async { Json(User { id: 1, name: "Ada".into() }) });
app.get("/stats", |_| async { json!({ "users": 42, "online": true }) });
app.get("/page", |_| async { Html("<h1>Bienvenue</h1>") });
app.get("/ping", |_| async { StatusCode::NO_CONTENT });
```

> [!NOTE]
> La macro `json!` et `serde_json` sont réexportées par Vitesse (`vitesse::json`, `vitesse::serde_json`) : inutile d'ajouter `serde_json` à votre `Cargo.toml`. Pour dériver `Serialize` sur vos propres types, ajoutez `serde = { version = "1", features = ["derive"] }`.

### Codes de statut

Un tuple `(statut, corps)` change le statut de n'importe quelle réponse. Le statut peut être un nombre ou une constante `StatusCode` :

```rust
app.post("/users", |_| async { (201, Json(json!({ "id": 2 }))) });
app.post("/jobs", |_| async { (StatusCode::ACCEPTED, "en file d'attente") });
```

Renvoyer un `StatusCode` seul envoie le statut avec sa raison en texte, comme `res.sendStatus(404)` en Express. Un code invalide (comme `1000`) devient un `500`.

### `Option` et `Result`

`None` devient un `404 {"error":"Not Found"}`. Un `Result` envoie soit la valeur, soit l'erreur : la plupart des handlers renvoient un `vitesse::Result<T>` et utilisent `?`, comme l'explique [Gestion des erreurs](errors.md).

```rust
// `find_user` est votre propre fonction, qui renvoie une `Option<User>`.
async fn show_user(req: Request) -> vitesse::Result<Json<User>> {
    let id: u32 = req.param_as("id")?; // 400 si ce n'est pas un nombre
    let user = find_user(id).ok_or_else(|| Error::not_found("utilisateur introuvable"))?;
    Ok(Json(user))
}
```

## Le builder `res`

Quand vous avez besoin de tout contrôler (statut, en-têtes, cookies et corps à la fois), construisez une `Response`. Le module `res` fournit des points d'entrée à la Express, et chacun renvoie une `Response` que vous pouvez continuer à enchaîner :

```rust
app.get("/custom", |_| async {
    res::status(202)
        .header("x-powered-by", "Vitesse")
        .cookie(Cookie::new("vu", "1").http_only(true))
        .json(json!({ "ok": true }))
});
```

| Express | Vitesse |
|---|---|
| `res.status(201)` | `res::status(201)` ou `.status(201)` |
| `res.send(corps)` | `res::send(corps)` ou `.send(corps)` |
| `res.json(obj)` | `res::json(obj)` ou `.json(obj)` |
| `res.type('text/csv')` | `.content_type("text/csv")` |
| `res.set(nom, valeur)` | `.header(nom, valeur)` |
| `res.append(nom, valeur)` | `.append_header(nom, valeur)` |
| `res.cookie(...)` / `res.clearCookie(nom)` | `.cookie(Cookie::new(...))` / `.clear_cookie(nom)` |
| `res.attachment(nom)` | `.attachment(nom)` |
| `res.redirect(url)` | `res::redirect(url)` |
| `res.sendStatus(404)` | `res::send_status(404)` |
| `res.sendFile(chemin)` | `res::file(chemin).await` |
| `res.download(chemin, nom)` | `res::download(chemin, nom).await` |

Il existe aussi `res::text(...)` / `.text(...)` et `res::html(...)` / `.html(...)`, et `Response::new()` vous donne un `200` vide comme point de départ. Chaque méthode prend la réponse et la renvoie : l'ordre est libre. Retenez simplement que `.text()`, `.html()` et `.json()` définissent le `Content-Type` (en remplaçant un `.content_type(...)` précédent), alors que `.send()` ne définit que le corps.

> [!WARNING]
> Contrairement à `res.send()` en Express, `send` ne devine jamais le `Content-Type` : `res::send("bonjour")` part sans. Utilisez `text`, `html` ou `json`, ou ajoutez `.content_type(...)`.

### En-têtes

`.header(nom, valeur)` définit un en-tête (en remplaçant la valeur précédente) et `.append_header(nom, valeur)` ajoute une valeur de plus. Les noms peuvent être des chaînes ou les constantes de `vitesse::header` ; les valeurs, des `&str`, des `String` ou des entiers. Un nom ou une valeur invalide (un retour à la ligne, par exemple) est ignoré silencieusement.

```rust
use vitesse::header;

app.get("/report", |_| async {
    Response::new()
        .header(header::CACHE_CONTROL, "no-store")
        .header("x-total-count", 42)
        .append_header("vary", "accept")
        .append_header("vary", "accept-language")
        .text("...")
});
```

Pour modifier une réponse existante (typiquement dans un [middleware](middleware.md)), utilisez les versions « en place » : `set_status`, `set_header`, `headers_mut()`, et pour la lire, `status_code()` et `get_header(nom)`.

### Type de contenu

`text`, `html` et `json` couvrent les cas courants. Pour le reste, définissez-le vous-même :

```rust
app.get("/export.csv", |_| async {
    res::send("id,nom\n1,Ada\n").content_type("text/csv; charset=utf-8")
});
```

### Cookies

```rust
use std::time::Duration;
use vitesse::SameSite;

app.post("/login", |_| async {
    let session = Cookie::new("session", "abc123")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::from_secs(7 * 24 * 3600));
    Response::new().cookie(session).json(json!({ "ok": true }))
});

app.post("/logout", |_| async { res::redirect("/").clear_cookie("session") });
```

La route de connexion envoie `Set-Cookie: session=abc123; Path=/; Max-Age=604800; HttpOnly; Secure; SameSite=Lax`.

| Méthode de `Cookie` | Attribut |
|---|---|
| `Cookie::new(nom, valeur)` | `nom=valeur; Path=/` |
| `.path("/admin")` | `Path` |
| `.domain("example.com")` | `Domain` |
| `.max_age(Duration)` | `Max-Age` (en secondes) |
| `.secure(true)` | `Secure` |
| `.http_only(true)` | `HttpOnly` |
| `.same_site(SameSite::Strict / Lax / None)` | `SameSite` |

Chaque appel à `.cookie(...)` ajoute son propre en-tête `Set-Cookie` : vous pouvez donc en envoyer plusieurs. `.clear_cookie(nom)` envoie `nom=; Path=/; Max-Age=0` : si le cookie a été créé avec un autre chemin ou domaine, construisez vous-même le cookie de suppression avec les mêmes attributs et `.max_age(Duration::ZERO)`. Pour lire les cookies envoyés par le navigateur, utilisez `req.cookie("session")` (voir [Lire la requête](requests.md)).

> [!IMPORTANT]
> La valeur est écrite telle quelle : elle n'est ni signée, ni chiffrée, ni encodée. Gardez-la compatible avec une URL (un jeton de session aléatoire, par exemple) et stockez les données sensibles côté serveur. Les navigateurs n'acceptent `SameSite::None` qu'avec `.secure(true)`.

### Redirections

| Helper | Statut |
|---|---|
| `Redirect::to(url)` ou `res::redirect(url)` | `302 Found` (le défaut d'Express) |
| `Redirect::permanent(url)` | `301 Moved Permanently` |
| `Redirect::see_other(url)` | `303 See Other` (après un `POST` de formulaire) |
| `Redirect::temporary(url)` | `307 Temporary Redirect` (conserve la méthode et le corps) |

```rust
app.get("/ancienne-page", |_| async { Redirect::permanent("/nouvelle-page") });
app.post("/contact", |_| async { Redirect::see_other("/merci") });
```

Pour un autre statut, posez l'en-tête vous-même : `res::status(308).header("location", "/v2")`. Une URL contenant des caractères invalides (comme un retour à la ligne) produit un `500`.

## Fichiers et téléchargements

```rust
app.get("/cgu", |_| async { res::file("legal/cgu.pdf").await });
app.get("/facture", |_| async {
    res::download("fichiers/facture-42.pdf", "facture.pdf").await
});
```

`res::file` devine le `Content-Type` d'après l'extension, envoie les gros fichiers en flux, et répond `404 {"error":"Not Found"}` si le fichier n'existe pas. Comme Express, il envoie aussi `ETag` et `Last-Modified`, répond `304 Not Modified` quand le navigateur a déjà le fichier, et `206 Partial Content` aux requêtes `Range` des lecteurs vidéo et des gestionnaires de téléchargement (voir [Fichiers statiques](static-files.md#les-requêtes-partielles)). `res::download` ajoute en plus `Content-Disposition: attachment` pour que le navigateur enregistre le fichier sous le nom donné (les noms non ASCII sont gérés). `.attachment("export.csv")` fait la même chose sur n'importe quelle réponse, ce qui est pratique pour un contenu généré. Les chemins relatifs partent du dossier depuis lequel le serveur a été lancé. Pour servir un dossier entier, voir [Fichiers statiques](static-files.md).

## Streaming

`Body::from_stream(flux)` envoie chaque élément d'un flux dès qu'il est produit, en `Transfer-Encoding: chunked`. Le flux produit des `Result<D, E>`, où `D` se convertit en octets (`String`, `&'static str`, `Vec<u8>`, `Bytes`) et `E` est un type d'erreur. Les flux viennent en général de crates comme `tokio-stream` ou `futures-util`. Voici des Server-Sent Events alimentés par un canal :

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
```

```rust
use std::time::Duration;
use tokio_stream::wrappers::ReceiverStream;
use vitesse::prelude::*;

app.get("/events", |_| async {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<String, std::io::Error>>(16);
    tokio::spawn(async move {
        for i in 1..=5 {
            if tx.send(Ok(format!("data: tic {i}\n\n"))).await.is_err() {
                break; // le client est parti
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    Response::new()
        .content_type("text/event-stream")
        .header("cache-control", "no-cache")
        .send(Body::from_stream(ReceiverStream::new(rx)))
});
```

`Body::wrap(corps)` accepte n'importe quel [`http_body::Body`](https://docs.rs/http-body), et `req.take_body()` vous donne le corps de la requête sous forme de `Body`, que vous pouvez renvoyer tel quel :

```rust
app.post("/echo", |req: Request| async move { req.take_body() });
```

> [!TIP]
> Si vous connaissez à l'avance la taille totale d'un flux, indiquez-la avec `.header("content-length", taille)` : la réponse est alors envoyée avec cette longueur au lieu d'un encodage `chunked`.

## `HEAD`, `204` et `304`

Vous n'avez rien à faire pour ces cas : une requête `HEAD` utilise la route `GET` et le moteur envoie les en-têtes (y compris `Content-Length`) sans le corps, et les réponses `204 No Content`, `304 Not Modified` et `1xx` n'ont jamais de corps.

## Vos propres types

Implémentez `IntoResponse` pour renvoyer directement vos propres types depuis les handlers :

```rust
struct Csv(String);

impl IntoResponse for Csv {
    fn into_response(self) -> Response {
        res::send(self.0).content_type("text/csv; charset=utf-8")
    }
}

app.get("/export", |_| async { Csv("id,nom\n1,Ada\n".into()) });
```

La même technique transforme vos propres types d'erreur en réponses (voir [Gestion des erreurs](errors.md)). Enfin, si vous travaillez avec la crate [`http`](https://docs.rs/http), `Response::from_http` et `Response::into_http` convertissent dans les deux sens, et un handler peut renvoyer directement une `http::Response<Body>`.
