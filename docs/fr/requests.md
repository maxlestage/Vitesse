# Lire la requête

Chaque handler reçoit une `Request`. Vous y lisez la méthode, le chemin, les paramètres de route, la query string, les en-têtes, les cookies, le corps et l'adresse du client. Cette page passe tout cela en revue, avec l'équivalent Express quand il existe.

## L'objet `Request`

Un handler reçoit la requête par valeur. Presque toutes les méthodes prennent `&self`, y compris la lecture du corps : vous pouvez donc garder un paramètre sous la main pendant que vous attendez le corps.

```rust
app.put("/users/:id", |req: Request| async move {
    let id: u64 = req.param_as("id")?;
    let agent = req.header("user-agent").unwrap_or("inconnu");
    let body: vitesse::serde_json::Value = req.json().await?;
    Ok::<_, Error>(format!("utilisateur {id} modifié par {agent} : {body}"))
});
```

Les quelques méthodes qui modifient la requête (`set`, `set_body`, `set_uri`, `headers_mut`, `extensions_mut`) demandent une `mut req`. Vous les utiliserez surtout dans des [middlewares](middleware.md).

## Méthode, chemin et URL

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

- `req.method()` renvoie une `&Method`. Comparez-la avec `*req.method() == Method::POST`.
- `req.path()` renvoie le chemin sans la query string, tel que reçu (non décodé), comme `req.path` dans Express.
- `req.uri()` renvoie l'`http::Uri` complète, chemin et query string, comme `req.originalUrl`.
- `req.hostname()` renvoie l'en-tête `Host` sans le port : `example.com` pour `example.com:8080`.
- `req.version()` renvoie la version HTTP (`vitesse::http::Version`).

### Réécrire l'URL

`req.set_uri(uri)` remplace le chemin et la query string. Les middlewares globaux s'exécutent avant le routage, donc une réécriture faite à cet endroit change la route qui répond :

```rust
app.middleware(|mut req: Request, next: Next| async move {
    if req.path() == "/old-page" {
        req.set_uri(vitesse::http::Uri::from_static("/new-page"));
    }
    next.run(req).await
});
```

Le navigateur ne voit pas cette réécriture. Pour l'envoyer vers la nouvelle adresse, renvoyez plutôt un `Redirect::to(...)` (voir [Envoyer la réponse](responses.md)).

## Paramètres de route

Ils sont détaillés dans [Routage](routing.md). En bref :

```rust
app.get("/users/:id/posts/:post", |req: Request| async move {
    let user: u64 = req.param_as("id")?;              // typé, 400 si invalide
    let post = req.param("post").unwrap_or_default(); // Option<&str>
    let all: Vec<String> = req.params().map(|(k, v)| format!("{k}={v}")).collect();
    Ok::<_, Error>(format!("{user} {post} {}", all.join("&")))
});
```

## Query string

```rust
app.get("/search", |req: Request| async move {
    let q = req.query("q").unwrap_or_default();
    let page: u32 = req.query("page").and_then(|p| p.parse().ok()).unwrap_or(1);
    format!("recherche de {q:?}, page {page}")
});
```

`GET /search?q=caf%C3%A9+cr%C3%A8me&page=2` répond `recherche de "café crème", page 2`.

- `req.query(nom)` renvoie la valeur décodée du premier paramètre portant ce nom, sous forme d'`Option<Cow<str>>`. Un `+` devient une espace. Un `Cow<str>` s'utilise comme un `&str` (et `.into_owned()` vous donne une `String`). Il n'alloue de mémoire que si la valeur contenait des caractères encodés.
- `req.query_pairs()` parcourt toutes les paires `(nom, valeur)`, noms répétés compris. C'est ainsi qu'on lit `?tag=a&tag=b` :

```rust
app.get("/tags", |req: Request| async move {
    let tags: Vec<String> = req
        .query_pairs()
        .filter(|(key, _)| key == "tag")
        .map(|(_, value)| value.into_owned())
        .collect();
    format!("tags : {}", tags.join(", "))
});
```

- `req.query_string()` renvoie la chaîne brute (`q=caf%C3%A9+cr%C3%A8me&page=2`), ou `None` s'il n'y a pas de query string.

### Query string typée avec `query_as`

Dès qu'il y a plus de deux ou trois paramètres, désérialisez toute la query string dans une structure avec `req.query_as::<T>()`. Utilisez `Option` pour les paramètres facultatifs et `#[serde(default)]` pour les valeurs par défaut :

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
    Ok(format!("{} (page {}, exact : {})", s.q, s.page.unwrap_or(1), s.exact))
}
```

Si un champ obligatoire manque ou qu'une valeur ne se convertit pas, `query_as` renvoie une `400 Bad Request` :

```text
GET /search               → 400 {"error":"invalid query string: missing field `q`"}
GET /search?q=x&page=abc  → 400 {"error":"invalid query string: invalid digit found in string"}
```

> [!NOTE]
> `query_as` ne gère pas les clés répétées : `?q=a&q=b` est refusé avec une `400` (`duplicate field`). Pour ces cas-là, utilisez `req.query_pairs()`.

## En-têtes

```rust
app.get("/whoami", |req: Request| async move {
    let agent = req.header("user-agent").unwrap_or("inconnu");
    format!("Vous utilisez {agent}")
});
```

- `req.header(nom)` renvoie la valeur sous forme d'`Option<&str>`, comme `req.get('User-Agent')` dans Express. Le nom est insensible à la casse. Vous pouvez aussi passer une constante de `vitesse::header`, par exemple `req.header(header::AUTHORIZATION)`.
- `req.header_all(nom)` parcourt toutes les valeurs d'un en-tête répété.
- `req.headers()` renvoie la `HeaderMap` complète, par exemple pour parcourir tous les en-têtes. Elle est construite au premier appel. Quand vous n'avez besoin que de quelques en-têtes, `req.header(...)` coûte moins cher : il lit directement la requête brute.
- `req.headers_mut()` permet à un middleware d'ajouter, de modifier ou de retirer des en-têtes avant que le handler ne les voie.

> [!NOTE]
> Une valeur d'en-tête qui n'est pas en ASCII visible simple (avec des lettres accentuées, par exemple) ne peut pas toujours être renvoyée en `&str`. Dans ce cas, lisez les octets bruts avec `req.headers().get("x-name").map(|v| v.as_bytes())`.

### Type de contenu

- `req.content_type()` est un raccourci pour `req.header("content-type")`.
- `req.is(type)` vérifie le type du corps, comme `req.is()` dans Express, mais renvoie un `bool`. Il accepte un type complet (`"application/json"`), un sous-type (`"json"`, qui reconnaît aussi les suffixes comme `application/ld+json`) ou un joker (`"text/*"`). Les paramètres comme `; charset=utf-8` sont ignorés.

```rust
app.post("/import", |req: Request| async move {
    if !req.is("json") {
        return Err(Error::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, "corps JSON attendu"));
    }
    let items: Vec<vitesse::serde_json::Value> = req.json().await?;
    Ok(format!("{} éléments importés", items.len()))
});
```

## Cookies

`req.cookie(nom)` lit un cookie dans l'en-tête `Cookie`. Avec Express, il faudrait `cookie-parser` pour `req.cookies.nom` ; ici, rien à installer :

```rust
app.get("/", |req: Request| async move {
    match req.cookie("session") {
        Some(id) => format!("Bon retour parmi nous (session {id})"),
        None => "Bonjour, inconnu".to_string(),
    }
});
```

La valeur est renvoyée telle que le navigateur l'a envoyée (sans les guillemets qui l'entourent). Elle n'est ni décodée ni vérifiée par une signature. Pour poser ou supprimer des cookies, voir [Envoyer la réponse](responses.md).

## Corps

Vitesse n'a pas de middleware d'analyse du corps. Vous lisez le corps en appelant une méthode, qui le récupère et l'analyse sur le moment. Toutes ces méthodes sont `async` et renvoient un `vitesse::Result`, donc `?` transforme un corps invalide en erreur HTTP appropriée :

| Méthode | Équivalent Express | Vous obtenez | Erreur |
|---|---|---|---|
| `req.json::<T>().await` | `express.json()` | `T` (désérialisé) | `400` si le JSON est invalide |
| `req.form::<T>().await` | `express.urlencoded()` | `T` (désérialisé) | `400` si le formulaire est invalide |
| `req.text().await` | `express.text()` | `String` | `400` si ce n'est pas de l'UTF-8 |
| `req.bytes().await` | `express.raw()` | `Bytes` | |
| `req.take_body()` | lire `req` comme un flux | `Body` (un flux) | |

Les quatre premières lisent tout le corps en mémoire, dans la limite de la [taille maximale](#taille-maximale-du-corps-et-413).

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
        "{} ({} tags, brouillon : {})",
        post.title,
        post.tags.len(),
        post.draft.unwrap_or(false)
    ))
}
```

- Un JSON invalide, un champ manquant ou un mauvais type donne une `400` qui explique pourquoi (exemple ci-dessous). Un corps vide est lui aussi un JSON invalide.
- Les champs inconnus sont ignorés. Ajoutez `#[serde(deny_unknown_fields)]` à la structure pour les refuser.
- Pour du JSON sans forme fixe, demandez une `vitesse::serde_json::Value`.
- Contrairement à `express.json()`, `req.json()` ne regarde pas le `Content-Type` : il analyse ce qui a été envoyé, quoi qu'il arrive. Pour l'exiger, vérifiez d'abord `req.is("json")` (exemple plus haut).

Par exemple, envoyer `{"tags": []}` à ce handler donne :

```json
{"error":"invalid JSON: missing field `title` at line 1 column 12"}
```

### Formulaires

Les formulaires HTML (`application/x-www-form-urlencoded`) se lisent de la même façon, avec `req.form()` :

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Login {
    email: String,
    password: String,
    // Une case non cochée n'est pas envoyée du tout : `false` par défaut
    #[serde(default)]
    remember: bool,
}

async fn login(req: Request) -> vitesse::Result<Redirect> {
    let form: Login = req.form().await?;
    if form.email.is_empty() || form.password.is_empty() {
        return Err(Error::bad_request("e-mail et mot de passe obligatoires"));
    }
    // … vérifier les identifiants, ouvrir une session…
    Ok(Redirect::see_other("/dashboard"))
}
```

Vitesse n'analyse pas le `multipart/form-data` (formulaires d'envoi de fichiers). Lisez le flux brut avec [`take_body()`](#streaming-avec-take_body) et confiez-le à une crate d'analyse multipart.

### Texte et octets bruts

```rust
app.post("/shout", |req: Request| async move {
    let text = req.text().await?; // String, 400 si ce n'est pas de l'UTF-8 valide
    Ok::<_, Error>(text.to_uppercase())
});

app.post("/size", |req: Request| async move {
    let bytes = req.bytes().await?; // vitesse::Bytes
    Ok::<_, Error>(format!("{} octets reçus", bytes.len()))
});
```

Le corps n'est lu qu'une fois, puis gardé en mémoire : vous pouvez donc appeler ces méthodes plusieurs fois. Par exemple, un webhook peut vérifier une signature sur les octets bruts, puis analyser le même corps en JSON :

```rust
app.post("/webhook", |req: Request| async move {
    let raw = req.bytes().await?;          // vérifier une signature sur les octets bruts…
    let event: vitesse::serde_json::Value = req.json().await?; // …puis les analyser
    Ok::<_, Error>(StatusCode::NO_CONTENT)
});
```

### Taille maximale du corps et 413

`json`, `form`, `text` et `bytes` lisent tout le corps en mémoire, sa taille est donc limitée : **1 Mio** par défaut (`vitesse::DEFAULT_BODY_LIMIT`). Un corps plus gros reçoit une `413 Payload Too Large`. Si le client a annoncé la taille avec `Content-Length`, la requête est refusée sans même lire le corps. Pour changer la limite :

```rust
let mut app = App::new();
app.body_limit(10 * 1024 * 1024); // 10 Mio
```

- La limite s'applique à toute l'application, comme `express.json({ limit: '10mb' })`.
- Elle n'entre en jeu que lorsque vous lisez le corps. Un handler qui ne le lit jamais ne produit jamais de `413`.
- Si une seule route a besoin de plus (un upload, par exemple), gardez une limite globale basse et lisez le corps de cette route en flux avec `take_body()`, en appliquant votre propre limite.

### Streaming avec `take_body`

Pour les gros uploads, ou pour relayer un corps ailleurs, `req.take_body()` vous donne le corps brut sous forme de flux (`vitesse::Body`), sans le charger en mémoire. Les données arrivent au rythme où le client les envoie.

> [!WARNING]
> `take_body()` contourne `app.body_limit` : c'est à vous de limiter la taille. Ensuite, `json()`, `text()` et les autres méthodes de lecture du corps ne peuvent plus le lire : elles échouent avec une `500`.

`Body` implémente le trait standard `http_body::Body`. Le plus simple pour le lire morceau par morceau est la méthode `frame()` de [http-body-util](https://docs.rs/http-body-util) (`cargo add http-body-util`). Cet exemple enregistre un upload dans un fichier, avec un plafond de 100 Mio :

```rust
use http_body_util::BodyExt; // pour .frame()
use vitesse::prelude::*;
use vitesse::tokio::{fs::File, io::AsyncWriteExt};

const MAX_UPLOAD: usize = 100 * 1024 * 1024; // 100 Mio

async fn upload(req: Request) -> vitesse::Result<String> {
    let mut body = req.take_body();
    let mut file = File::create("upload.bin").await?;
    let mut total = 0;

    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|e| Error::bad_request("envoi interrompu").with_source(e))?;
        if let Ok(chunk) = frame.into_data() {
            total += chunk.len();
            if total > MAX_UPLOAD {
                return Err(Error::payload_too_large());
            }
            file.write_all(&chunk).await?;
        }
    }
    file.flush().await?;
    Ok(format!("{total} octets enregistrés"))
}
```

`?` transforme les erreurs de fichier (`std::io::Error`) en `500`. En revanche, une erreur de lecture du flux est une `BoxError` générique que `?` ne sait pas convertir tout seul, d'où le `map_err` de l'exemple.

Une réponse peut aussi être un `Body` : renvoyer le flux tel quel donne un écho en streaming.

```rust
app.post("/echo", |req: Request| async move { req.take_body() });
```

### Remplacer le corps

`req.set_body(corps)` remplace le corps avant que le handler ne le lise. Un middleware peut s'en servir pour décompresser ou déchiffrer le corps. Il accepte tout ce qui se convertit en `Body` : `String`, `Vec<u8>`, `Bytes`, `&'static str`, etc. Le nouveau corps est soumis à `body_limit` comme l'original.

## Adresse du client

```rust
app.get("/ip", |req: Request| async move {
    match req.ip() {
        Some(ip) => format!("Votre IP : {ip}"),
        None => "inconnue".to_string(),
    }
});
```

- `req.ip()` renvoie l'adresse IP du client connecté, sous forme d'`Option<IpAddr>`, comme `req.ip`. `req.remote_addr()` renvoie l'IP et le port, sous forme d'`Option<SocketAddr>`.
- Derrière un reverse proxy ou un répartiteur de charge (Nginx, Heroku, le load balancer d'un cloud, etc.), cette adresse est celle du proxy. Le client d'origine figure en général dans l'en-tête `X-Forwarded-For`. Vitesse n'a pas d'équivalent du réglage `trust proxy` d'Express, lisez donc l'en-tête vous-même (voir aussi [Mise en production](production.md)) :

```rust
fn client_ip(req: &Request) -> Option<String> {
    req.header("x-forwarded-for")
        .and_then(|list| list.split(',').next())
        .map(|ip| ip.trim().to_string())
        .or_else(|| req.ip().map(|ip| ip.to_string()))
}
```

> [!WARNING]
> Un client peut envoyer `X-Forwarded-For` lui-même. Ne vous y fiez que si votre proxy le renseigne, et ne prenez jamais de décision de sécurité sur cette base.

## État et données par requête

- `req.state::<T>()` renvoie l'état global enregistré avec `app.state(valeur)`, l'équivalent de `app.locals`. Il panique (et répond donc `500`) si aucun état de ce type n'existe. `req.try_state::<T>()` renvoie une `Option` à la place. Voir [État partagé](state.md).
- `req.set(valeur)` attache une valeur à cette requête et `req.get::<T>()` la relit, l'équivalent de `res.locals` ou `req.user`. Le type doit implémenter `Clone`. C'est ainsi qu'un middleware transmet l'utilisateur connecté au handler (voir [Middlewares](middleware.md)).
- `req.extensions()` et `req.extensions_mut()` donnent un accès direct au stockage `http::Extensions` sous-jacent.

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
        Some(user) => format!("Bonjour, {}", user.name),
        None => "Non connecté".to_string(),
    }
});
```

## Référence des méthodes

| Méthode | Express | Renvoie |
|---|---|---|
| `method()` | `req.method` | `&Method` |
| `path()` | `req.path` | `&str` |
| `uri()` | `req.originalUrl` | `&Uri` |
| `version()` | `req.httpVersion` | `Version` |
| `hostname()` | `req.hostname` | `Option<&str>` |
| `set_uri(uri)` | affecter `req.url` | |
| `param(nom)` | `req.params.nom` | `Option<&str>` |
| `param_as::<T>(nom)` | | `Result<T, Error>` (`400`) |
| `params()` | `req.params` | itérateur de `(&str, &str)` |
| `query(nom)` | `req.query.nom` | `Option<Cow<str>>` |
| `query_as::<T>()` | | `Result<T, Error>` (`400`) |
| `query_pairs()` | | itérateur de `(Cow<str>, Cow<str>)` |
| `query_string()` | | `Option<&str>` |
| `header(nom)` | `req.get(nom)` | `Option<&str>` |
| `header_all(nom)` | | itérateur de `&str` |
| `headers()` / `headers_mut()` | `req.headers` | `&HeaderMap` / `&mut HeaderMap` |
| `content_type()` | `req.get('content-type')` | `Option<&str>` |
| `is(type)` | `req.is(type)` | `bool` |
| `cookie(nom)` | `req.cookies.nom` | `Option<&str>` |
| `json::<T>().await` | `req.body` + `express.json()` | `Result<T, Error>` |
| `form::<T>().await` | `req.body` + `express.urlencoded()` | `Result<T, Error>` |
| `text().await` | `req.body` + `express.text()` | `Result<String, Error>` |
| `bytes().await` | `req.body` + `express.raw()` | `Result<Bytes, Error>` |
| `take_body()` | `req` (flux) | `Body` |
| `set_body(corps)` | | |
| `ip()` | `req.ip` | `Option<IpAddr>` |
| `remote_addr()` | `req.socket.remoteAddress` | `Option<SocketAddr>` |
| `state::<T>()` / `try_state::<T>()` | `req.app.locals` | `&T` / `Option<&T>` |
| `get::<T>()` / `set(valeur)` | `res.locals` | `Option<&T>` / `Option<T>` (la valeur précédente) |
| `extensions()` / `extensions_mut()` | | `&Extensions` / `&mut Extensions` |
