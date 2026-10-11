# Lire la requête

Chaque handler reçoit une `Request`. Vous y lisez la méthode, le chemin, les paramètres de route, la query string, les en-têtes, les cookies, le corps, les fichiers envoyés et l'adresse du client, et savoir quel format le client préfère. Cette page passe tout cela en revue, avec l'équivalent Express quand il existe.

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

Les quelques méthodes qui modifient la requête (`set`, `set_body`, `set_uri`, `set_method`, `set_body_limit`, `headers_mut`, `extensions_mut`) demandent une `mut req`. Vous les utiliserez surtout dans des [middlewares](middleware.md).

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
- `req.hostname()` renvoie l'hôte demandé sans le port : `example.com` pour `example.com:8080`. `req.host()` garde le port (voir [Hôte et protocole](#hôte-et-protocole)).
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

### Surcharger la méthode

`req.set_method(méthode)` remplace la méthode. Comme `set_uri`, elle change la route qui répond quand vous l'appelez dans un middleware global. C'est ainsi qu'on écrit l'équivalent du paquet `method-override` d'Express : les formulaires HTML ne savent envoyer que `GET` et `POST`, le formulaire passe donc la méthode voulue dans la query string :

```html
<form method="post" action="/posts/42?_method=DELETE">
  <button>Supprimer</button>
</form>
```

```rust
app.middleware(|mut req: Request, next: Next| async move {
    if *req.method() == Method::POST {
        let wanted = req
            .query("_method")
            .and_then(|m| m.to_ascii_uppercase().parse::<Method>().ok())
            .filter(|m| [Method::PUT, Method::PATCH, Method::DELETE].contains(m));
        if let Some(method) = wanted {
            req.set_method(method);
        }
    }
    next.run(req).await
});

app.delete("/posts/:id", |_| async { "article supprimé" }); // atteinte aussi par le formulaire
```

Seules les requêtes `POST` sont réécrites, et seulement en `PUT`, `PATCH` ou `DELETE`, comme le fait `method-override` par défaut : un lien (`GET`) ne peut rien supprimer.

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

### La route correspondante

`req.route()` renvoie le motif de la route qui répond à la requête, préfixe de montage compris : `Some("/users/:id/posts/:post")` pour `/users/42/posts/7`. C'est le `req.route.path` d'Express, et c'est ce qu'il vous faut dans les journaux et les métriques, où le chemin brut donnerait une ligne par utilisateur. Voir [Routage](routing.md#la-route-correspondante--reqroute).

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

## Négociation de contenu

Certaines routes peuvent répondre dans plusieurs formats : du HTML pour un navigateur, du JSON pour un script. `req.accepts(&[...])` lit l'en-tête `Accept` et renvoie, parmi les formats que vous proposez, celui que le client préfère, comme `req.accepts()` dans Express :

```rust
async fn show_user(req: Request) -> Response {
    match req.accepts(&["html", "json"]) {
        Some("html") => res::html("<h1>Ada Lovelace</h1>"),
        Some(_) => res::json(json!({ "name": "Ada Lovelace" })),
        None => res::send_status(406), // Not Acceptable
    }
}
```

| `Accept` envoyé par le client | `req.accepts(&["html", "json"])` |
|---|---|
| aucun | `Some("html")` : votre première proposition |
| `text/html,application/xhtml+xml,…,*/*;q=0.8` (un navigateur) | `Some("html")` |
| `application/json` | `Some("json")` |
| `*/*` | `Some("html")` |
| `image/png` | `None` |

- Les propositions sont des extensions de fichier (`"html"`, `"json"`, `"csv"`, `"txt"`…) ou des types MIME (`"application/json"`, `"image/*"`). Vous récupérez la proposition telle que vous l'avez écrite : vous pouvez donc faire un `match` dessus.
- La qualité la plus haute (`q=`) l'emporte, puis la correspondance la plus précise (`text/html` avant `text/*`, avant `*/*`). Un type envoyé avec `q=0` est refusé.
- Sans en-tête `Accept`, le client accepte tout : vous obtenez votre première proposition.
- `None` signifie qu'aucune de vos propositions n'est acceptable : répondez `406 Not Acceptable`, ou rabattez-vous sur un format par défaut.

> [!TIP]
> Quand la réponse dépend d'`Accept`, ajoutez-lui `.header("vary", "accept")`, pour que les caches (celui du navigateur, un CDN) ne servent pas la version JSON à un navigateur.

`req.accepts_language(&[...])` fait la même chose avec `Accept-Language`, comme `req.acceptsLanguages()`. Une langue correspond à ses variantes régionales, et inversement : `"fr"` correspond à `fr-FR`, et `"en-GB"` à `en`. Sans l'en-tête, vous obtenez la première langue, et `None` quand aucune langue ne correspond : prévoyez donc une valeur par défaut :

```rust
app.get("/", |req: Request| async move {
    let lang = req.accepts_language(&["en", "fr", "es"]).unwrap_or("en");
    Redirect::to(format!("/{lang}/"))
});
```

Un navigateur français (`fr-FR,fr;q=0.9,en;q=0.8`) est envoyé vers `/fr/`, un navigateur allemand (`de`) vers `/en/`.

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
| `req.multipart()?` | `multer` | les champs d'un formulaire d'envoi de fichiers, un par un (voir [Envois de fichiers](#envois-de-fichiers-multipart)) | `415` si ce n'est pas du `multipart/form-data` |
| `req.take_body()` | lire `req` comme un flux | `Body` (un flux) | |

Les quatre premières lisent tout le corps en mémoire, dans la limite de la [taille maximale](#taille-maximale-du-corps-et-413), qui s'applique aussi à `multipart`.

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

Les formulaires qui envoient des fichiers (`enctype="multipart/form-data"`) se lisent avec `req.multipart()` : voir [Envois de fichiers (multipart)](#envois-de-fichiers-multipart).

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

`json`, `form`, `text` et `bytes` lisent tout le corps en mémoire, sa taille est donc limitée : **1 Mio** par défaut (`vitesse::DEFAULT_BODY_LIMIT`). Un corps plus gros reçoit une `413 Payload Too Large`. Si le client a annoncé la taille avec `Content-Length`, la requête est refusée sans même lire le corps. Pour changer la limite de toute l'application :

```rust
let mut app = App::new();
app.body_limit(10 * 1024 * 1024); // 10 Mio
```

La plupart des routes ne reçoivent pourtant que de petits corps JSON, et seules quelques-unes ont besoin de plus. Gardez une limite globale basse et augmentez-la là où c'est nécessaire avec `middleware::body_limit(octets)`, sur une route avec `.with(...)` ou sur tout un [routeur](routers.md). C'est l'équivalent de `express.json({ limit: '50mb' })` sur une seule route :

```rust
async fn import(req: Request) -> vitesse::Result<String> {
    let rows: Vec<vitesse::serde_json::Value> = req.json().await?;
    Ok(format!("{} lignes importées", rows.len()))
}

let mut app = App::new(); // 1 Mio par défaut…
app.post("/import", import.with(middleware::body_limit(50 * 1024 * 1024))); // …50 Mio ici

let mut admin = Router::new();
admin.middleware(middleware::body_limit(200 * 1024 * 1024)); // 200 Mio sous /admin
admin.post("/restore", restore_backup);
app.mount("/admin", admin);
```

La limite appartient à chaque requête : `req.body_limit()` renvoie celle qui s'applique, et `req.set_body_limit(octets)` la modifie, ce qui est tout ce que fait `middleware::body_limit`. Votre propre middleware peut s'en servir pour choisir une limite selon l'utilisateur, par exemple (`CurrentUser` étant posé par votre middleware d'authentification) :

```rust
async fn larger_bodies_for_admins(mut req: Request, next: Next) -> Response {
    if req.get::<CurrentUser>().is_some_and(|user| user.admin) {
        req.set_body_limit(1024 * 1024 * 1024); // 1 Gio
    }
    next.run(req).await
}
```

- La limite n'entre en jeu que lorsque vous lisez le corps. Un handler qui ne le lit jamais ne produit jamais de `413`.
- Elle couvre aussi [`req.multipart()`](#envois-de-fichiers-multipart) (le formulaire entier) et [`middleware::decompress()`](middleware.md#decompress) (la taille compressée et la taille décompressée), mais pas `take_body()`.
- En [HTTP/3](http3.md), les corps sont lus avant le routage, dans la limite de `app.body_limit` : si des clients HTTP/3 envoient de gros fichiers, augmentez aussi la limite globale.
- Un corps interrompu par le `RequestBodyLimitLayer` de tower-http (voir [Couches tower](middleware.md#couches-tower)) donne aussi une `413`.

### Streaming avec `take_body`

Pour un gros envoi brut (pas un formulaire), ou pour relayer un corps ailleurs, `req.take_body()` vous donne le corps brut sous forme de flux (`vitesse::Body`), sans le charger en mémoire. Les données arrivent au rythme où le client les envoie.

> [!WARNING]
> `take_body()` contourne la limite de taille (`app.body_limit`, `middleware::body_limit`) : c'est à vous de limiter la taille. Ensuite, `json()`, `text()` et les autres méthodes de lecture du corps ne peuvent plus le lire : elles échouent avec une `500`.

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

`req.set_body(corps)` remplace le corps avant que le handler ne le lise. Un middleware peut s'en servir pour déchiffrer ou transformer le corps ([`middleware::decompress()`](middleware.md#decompress) s'en sert pour les corps compressés). Il accepte tout ce qui se convertit en `Body` : `String`, `Vec<u8>`, `Bytes`, `&'static str`, etc. Le nouveau corps est soumis à la limite de taille comme l'original.

## Envois de fichiers (multipart)

Les formulaires qui envoient des fichiers utilisent `enctype="multipart/form-data"`. Avec Express, vous ajouteriez `multer` ; avec Vitesse, activez la feature `multipart` :

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["multipart"] }
```

```html
<form method="post" action="/upload" enctype="multipart/form-data">
  <input name="title">
  <input type="file" name="photos" multiple>
  <button>Envoyer</button>
</form>
```

`req.multipart()?` ouvre le formulaire, et `form.next_field().await?` renvoie ses champs un par un, dans l'ordre où le navigateur les a envoyés, pendant que le corps arrive encore : un fichier n'est jamais gardé en mémoire, sauf si vous le demandez.

```rust
use vitesse::prelude::*;

async fn upload(req: Request) -> vitesse::Result<String> {
    let mut form = req.multipart()?; // 415 si le corps n'est pas du multipart/form-data
    let mut report = Vec::new();
    while let Some(field) = form.next_field().await? {
        let name = field.name().unwrap_or("").to_owned();
        match field.file_name().map(safe_name) {
            // Un fichier : écrit sur le disque morceau par morceau, au fil de son arrivée.
            Some(file_name) => {
                let size = field.save(format!("uploads/{file_name}")).await?;
                report.push(format!("{name} : {file_name} ({size} octets)"));
            }
            // Un champ texte.
            None => {
                let value = field.text().await?;
                report.push(format!("{name} = {value}"));
            }
        }
    }
    Ok(report.join("\n"))
}

/// Le nom de fichier du client, réduit aux lettres, aux chiffres, à `-`, `_` et `.`
/// (pas de `/`, pas de `..`, pas de fichier caché).
fn safe_name(client_name: &str) -> String {
    let name: String = client_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    match name.trim_start_matches('.') {
        "" => "upload".to_owned(),
        name => name.to_owned(),
    }
}

let mut app = App::new();
app.post("/upload", upload.with(middleware::body_limit(100 * 1024 * 1024))); // 100 Mio
```

| Méthode de `Field` | Vous donne |
|---|---|
| `field.name()` | Le `name` du champ (`Option<&str>`) |
| `field.file_name()` | Pour un fichier, son nom sur la machine du client (`Option<&str>`) ; `None` pour un champ texte |
| `field.content_type()` | Le type annoncé par le navigateur (`image/png`…), s'il y en a un |
| `field.headers()` | Tous les en-têtes de la partie |
| `field.chunk().await?` | Le morceau suivant du contenu (`Option<Bytes>`), `None` à la fin |
| `field.bytes().await?` | Tout le contenu, en mémoire (`Bytes`) |
| `field.text().await?` | Tout le contenu sous forme de `String` (`400` si ce n'est pas de l'UTF-8) |
| `field.save(chemin).await?` | Écrit le contenu dans un fichier au fil de son arrivée (créé, ou tronqué s'il existe) et renvoie sa taille en octets (`u64`) |

`bytes`, `text` et `save` consomment le champ. Les champs arrivent l'un après l'autre sur le même flux : lisez donc (ou abandonnez) chacun avant de demander le suivant. Pour traiter un fichier à la volée (calculer son empreinte, l'envoyer ailleurs), lisez-le morceau par morceau :

```rust
while let Some(mut field) = form.next_field().await? {
    let mut size = 0;
    while let Some(chunk) = field.chunk().await? {
        size += chunk.len(); // calculer une empreinte, l'envoyer vers un stockage objet…
    }
    println!("{:?} : {size} octets", field.name());
}
```

- Le corps entier est soumis à la [limite de taille](#taille-maximale-du-corps-et-413), 1 Mio par défaut : augmentez-la sur les routes d'envoi avec `middleware::body_limit`, comme ci-dessus. Au-delà, la lecture échoue avec une `413`.
- Un corps qui n'est pas du `multipart/form-data` reçoit une `415 Unsupported Media Type`, et un formulaire malformé une `400`.
- `req.multipart()` prend le corps : les autres méthodes de lecture du corps ne peuvent plus le lire ensuite.
- Si l'envoi échoue en cours de route (`413`, client parti), `save` supprime le fichier partiel et renvoie l'erreur.
- L'exemple `examples/live.rs` du dépôt contient une page d'envoi complète (`cargo run --example live --features multipart`).

> [!WARNING]
> `file_name()` vient du client : il peut valoir `../../etc/passwd`, contenir n'importe quel caractère, ou être identique au fichier d'un autre utilisateur. Ne l'utilisez jamais tel quel comme chemin. Nettoyez-le comme ci-dessus ou, mieux, enregistrez le fichier sous un nom que vous choisissez (un identifiant aléatoire, une clé de base de données) et gardez le nom d'origine à titre d'information seulement. `content_type()` est lui aussi ce que le client prétend : vérifiez le contenu si cela compte.

## Hôte et protocole

```rust
app.get("/where", |req: Request| async move {
    format!(
        "{} {} {:?} {:?}",
        req.protocol(), // "https"
        req.secure(),   // true
        req.host(),     // Some("example.com:8443")
        req.hostname(), // Some("example.com")
    )
});
```

| Méthode | Express | Renvoie |
|---|---|---|
| `req.protocol()` | `req.protocol` | `"https"` sur TLS ou HTTP/3, `"http"` sinon |
| `req.secure()` | `req.secure` | `true` quand `protocol()` vaut `"https"` |
| `req.host()` | `req.host` (Express 5) | L'en-tête `Host` avec son port (`example.com:8443`), ou l'autorité d'une requête HTTP/2 ou HTTP/3 |
| `req.hostname()` | `req.hostname` | La même chose sans le port : `example.com` (`::1` pour `[::1]:3000`) |

Derrière un reverse proxy, la requête arrive à Vitesse en HTTP simple, et le `Host` peut être un nom interne. Déclarez votre proxy avec [`app.trust_proxy`](#derrière-un-proxy--trust_proxy), et ces méthodes lisent alors `X-Forwarded-Proto` et `X-Forwarded-Host` à la place.

> [!NOTE]
> L'hôte vient du client. Ne vous en servez pas pour construire des liens sensibles pour la sécurité (le lien d'un e-mail de réinitialisation de mot de passe, par exemple) : utilisez une URL de base tirée de votre configuration.

## Adresse du client

```rust
app.get("/ip", |req: Request| async move {
    match req.ip() {
        Some(ip) => format!("Votre IP : {ip}"),
        None => "inconnue".to_string(),
    }
});
```

- `req.ip()` renvoie l'adresse IP du client, sous forme d'`Option<IpAddr>`, comme `req.ip`. Par défaut, c'est l'adresse de l'autre extrémité de la connexion TCP.
- `req.remote_addr()` renvoie l'IP et le port de la connexion TCP, sous forme d'`Option<SocketAddr>`, comme `req.socket.remoteAddress`. Elle ne lit jamais les en-têtes de proxy.
- Sur un socket Unix (`app.run_unix(...)`, voir [Sockets Unix](server.md#sockets-unix)), la connexion n'a pas d'adresse IP : les deux valent `None`, sauf si vous réglez `trust_proxy` (ci-dessous), auquel cas `req.ip()` vient de `X-Forwarded-For`.

### Derrière un proxy : `trust_proxy`

Derrière un reverse proxy ou un répartiteur de charge (Nginx, Caddy, Heroku, le load balancer d'un cloud…), l'autre extrémité de la connexion est le proxy : `req.ip()` serait l'adresse du proxy pour toutes les requêtes, et `req.protocol()` vaudrait `"http"` même quand le visiteur utilise HTTPS. Le proxy transmet les valeurs d'origine dans des en-têtes : `X-Forwarded-For` (l'adresse du client, suivie des proxys par lesquels la requête est passée), `X-Forwarded-Proto` et `X-Forwarded-Host`. Comme avec `app.set('trust proxy', …)` dans Express, indiquez à Vitesse les proxys à croire :

```rust
let mut app = App::new();
app.trust_proxy("loopback"); // Nginx ou Caddy sur la même machine
```

| Valeur | Proxys de confiance | Express |
|---|---|---|
| `false` (le défaut) | aucun : les en-têtes sont ignorés | `false` |
| `"loopback"`, `"10.0.0.0/8, 192.168.1.10"` | les proxys dont l'adresse est dans ces réseaux | les mêmes chaînes |
| `1`, `2`… | ce nombre de proxys, les plus proches du serveur, quelle que soit leur adresse | les mêmes nombres |
| `true` | tous : le client est l'adresse la plus à gauche de `X-Forwarded-For` | `true` |

Les réseaux sont des adresses IP ou des plages CIDR, plus trois noms : `loopback` (`127.0.0.1/8`, `::1`), `linklocal` (`169.254.0.0/16`, `fe80::/10`) et `uniquelocal` (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `fc00::/7`). Combinez-les avec des virgules : `"loopback, uniquelocal"`. Une adresse invalide fait paniquer `trust_proxy` au démarrage. Un proxy connecté par un socket Unix, que seuls les processus de la même machine peuvent joindre, est considéré comme de confiance avec n'importe quelle valeur sauf `false` et `0`.

Quand la requête vient d'un proxy de confiance :

- `req.ip()` parcourt `X-Forwarded-For` depuis la droite (le proxy le plus proche) et s'arrête à la première adresse qui n'est pas un proxy de confiance : c'est le client.
- `req.protocol()` et `req.secure()` suivent `X-Forwarded-Proto: https`.
- `req.host()` et `req.hostname()` utilisent `X-Forwarded-Host` quand le proxy l'envoie.
- `req.remote_addr()` ne change pas : elle reste l'adresse du proxy.

Par exemple, avec `app.trust_proxy("loopback, 10.0.0.0/8")`, une requête venant du proxy local (`127.0.0.1`) et portant `X-Forwarded-For: 203.0.113.9, 10.0.0.2` vient de `203.0.113.9` : `10.0.0.2` est un load balancer interne, de confiance, et `203.0.113.9` ne l'est pas. Les middlewares intégrés s'appuient eux aussi sur `req.ip()` : [`middleware::rate_limit`](middleware.md#rate_limit) compte alors par visiteur plutôt que par proxy. Les configurations de proxy (Nginx, Caddy, Heroku) se trouvent dans [Mise en production](production.md).

> [!WARNING]
> N'importe qui peut envoyer des en-têtes `X-Forwarded-*`. Ne faites confiance qu'aux proxys que vous gérez, et assurez-vous que les clients ne peuvent pas joindre l'application sans passer par eux. Évitez `true`, sauf si votre proxy remplace `X-Forwarded-For` au lieu d'y ajouter une adresse : sinon, le client choisit sa propre adresse.

Le réglage analysé est un `vitesse::TrustProxy` (`"loopback".parse::<TrustProxy>()`), et `trust.trusts(ip, 0)` indique si une adresse est de confiance, au cas où vous auriez besoin de la même règle dans votre propre code.

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
| `set_method(méthode)` | `method-override` | |
| `path()` | `req.path` | `&str` |
| `uri()` | `req.originalUrl` | `&Uri` |
| `version()` | `req.httpVersion` | `Version` |
| `host()` | `req.host` | `Option<&str>` |
| `hostname()` | `req.hostname` | `Option<&str>` |
| `protocol()` | `req.protocol` | `&'static str` (`"http"` ou `"https"`) |
| `secure()` | `req.secure` | `bool` |
| `set_uri(uri)` | affecter `req.url` | |
| `route()` | `req.route.path` | `Option<&'static str>` |
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
| `accepts(&[...])` | `req.accepts([...])` | `Option<&str>` (`None` : répondre `406`) |
| `accepts_language(&[...])` | `req.acceptsLanguages(...)` | `Option<&str>` |
| `cookie(nom)` | `req.cookies.nom` | `Option<&str>` |
| `json::<T>().await` | `req.body` + `express.json()` | `Result<T, Error>` |
| `form::<T>().await` | `req.body` + `express.urlencoded()` | `Result<T, Error>` |
| `text().await` | `req.body` + `express.text()` | `Result<String, Error>` |
| `bytes().await` | `req.body` + `express.raw()` | `Result<Bytes, Error>` |
| `multipart()` (feature `multipart`) | `multer` | `Result<Multipart, Error>` (`415`) |
| `take_body()` | `req` (flux) | `Body` |
| `set_body(corps)` | | |
| `body_limit()` / `set_body_limit(octets)` | `express.json({ limit })` | `usize` / |
| `ip()` | `req.ip` (suit `trust_proxy`) | `Option<IpAddr>` |
| `remote_addr()` | `req.socket.remoteAddress` | `Option<SocketAddr>` |
| `state::<T>()` / `try_state::<T>()` | `req.app.locals` | `&T` / `Option<&T>` |
| `get::<T>()` / `set(valeur)` | `res.locals` | `Option<&T>` / `Option<T>` (la valeur précédente) |
| `extensions()` / `extensions_mut()` | | `&Extensions` / `&mut Extensions` |
