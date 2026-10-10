# Fichiers statiques

Vitesse sert les fichiers d'un dossier (CSS, JavaScript, images, build d'un front-end…) avec tout ce qu'on attend de `express.static` : types MIME, `index.html`, cache navigateur avec `ETag` et réponses `304`, requêtes partielles pour la vidéo, envoi en flux des gros fichiers et protection contre la sortie du dossier.

## Servir un dossier sous un préfixe

```js
app.use('/assets', express.static('public'));
```

```rust
use vitesse::prelude::*;

let mut app = App::new();
app.static_dir("/assets", "public");
```

`public/css/app.css` est maintenant servi à l'adresse `/assets/css/app.css`, et `public/index.html` à `/assets/`. En coulisses, `static_dir` déclare deux routes, `GET` et `HEAD` sur `/assets/*`. Un fichier qui n'existe pas répond `404 {"error":"Cannot GET /assets/nope.css"}`, qui passe par [`app.on_error`](errors.md) comme n'importe quelle autre erreur.

> [!NOTE]
> Un dossier relatif est résolu à partir du dossier depuis lequel le serveur est lancé (en général la racine du projet avec `cargo run`), pas à partir du fichier source. En production, lancez le binaire depuis le bon dossier ou utilisez un chemin absolu.

## À la racine, avant vos routes

Pour servir des fichiers à la racine du site, utilisez `ServeDir` comme **middleware** : si le fichier demandé existe, il est envoyé ; sinon, la requête continue vers vos routes. C'est exactement `app.use(express.static('public'))` :

```rust
app.middleware(ServeDir::new("public"));
app.get("/api/hello", |_| async { "Bonjour !" }); // toujours accessible
```

`middleware::serve_static("public")` est un synonyme. Dans ce mode, seules les requêtes `GET` et `HEAD` sont prises en compte, et un fichier absent ou refusé ne produit pas d'erreur : la requête poursuit simplement son chemin.

> [!TIP]
> En tant que middleware global, `ServeDir` interroge le système de fichiers pour chaque requête `GET`, avant le routage. Pour une API très sollicitée, préférez un préfixe (`app.static_dir("/assets", ...)`), qui ne coûte quelque chose qu'aux requêtes sous ce préfixe.

`app.static_dir("/", "public")` fonctionne aussi, mais le compromis est différent : il déclare une route joker `GET /*`. Vos autres routes restent prioritaires (un joker a la priorité la plus basse), mais tout chemin `GET` inconnu reçoit alors le `404` des fichiers statiques, et n'atteint jamais `app.fallback`.

## Les options : `ServeDir`

Pour plus de contrôle, configurez un `ServeDir` et montez-le avec `serve_dir` (ou `app.middleware(...)`) :

```rust
use std::time::Duration;

app.serve_dir(
    "/assets",
    ServeDir::new("public")
        .max_age(Duration::from_secs(3600))
        .index(None)
        .dotfiles(false),
);
```

| Option | Défaut | Effet |
|---|---|---|
| `index(Some("accueil.html"))` / `index(None)` | `Some("index.html")` | fichier servi pour un dossier ; `None` le désactive |
| `max_age(durée)` | `0` | durée du cache navigateur : `Cache-Control: public, max-age=...` |
| `dotfiles(true)` | `false` | autorise les fichiers et dossiers cachés (`.well-known`…) |

`static_dir`, `serve_dir` et `ServeDir` existent aussi sur un [`Router`](routers.md), ce qui permet de protéger des fichiers avec les middlewares du routeur :

```rust
let mut private = Router::new();
private.middleware(require_login); // votre middleware d'authentification
private.static_dir("/", "fichiers-prives");
app.mount("/private", private); // GET /private/rapport.pdf
```

## Les dossiers et `index.html`

Une requête vers un dossier sert son fichier d'index : `/assets/docs/` renvoie `public/docs/index.html`. Comme avec Express, une requête sans la barre oblique finale (`/assets/docs`) est redirigée par un `301` vers `/assets/docs/` (en conservant la query string), pour que les liens relatifs de la page continuent de fonctionner. Avec `index(None)`, les dossiers ne sont pas servis.

## Le cache navigateur : `ETag`, `Last-Modified` et `304`

Chaque fichier est envoyé avec :

- `Content-Type`, deviné d'après l'extension ;
- `ETag`, un validateur faible calculé à partir de la taille et de la date de modification (`W/"1a2b-65f0c3d1"`) ;
- `Last-Modified` ;
- `Cache-Control: public, max-age=...` (`max-age=0` par défaut : le navigateur vérifie auprès du serveur avant de réutiliser sa copie) ;
- `Accept-Ranges: bytes`.

Quand le navigateur a déjà la bonne version (`If-None-Match` ou `If-Modified-Since`), Vitesse répond `304 Not Modified`, sans corps :

```http
GET /assets/app.css HTTP/1.1
If-None-Match: W/"1a2b-65f0c3d1"

HTTP/1.1 304 Not Modified
etag: W/"1a2b-65f0c3d1"
```

> [!TIP]
> Si le build de votre front-end met une empreinte dans les noms de fichiers (`app.3f9c2b.js`), ces fichiers ne changent jamais : servez-les avec une longue durée de cache (`max_age(Duration::from_secs(31_536_000))`, un an), et laissez `index.html`, qui les référence, au `max-age=0` par défaut. `max_age` s'applique à tout un `ServeDir` : utilisez-en deux, un par dossier.

## Les requêtes partielles

Les lecteurs vidéo et audio, ainsi que les gestionnaires de téléchargement, demandent des morceaux de fichier avec l'en-tête `Range`. Vitesse gère une plage par requête :

| En-tête `Range` | Réponse |
|---|---|
| `bytes=0-499`, `bytes=500-`, `bytes=-500` (les 500 derniers octets) | `206 Partial Content` avec `Content-Range: bytes 0-499/1234` |
| une plage au-delà de la fin du fichier | `416 Range Not Satisfiable` avec `Content-Range: bytes */1234` |
| plusieurs plages (`bytes=0-1,5-6`) | le fichier entier, `200` |

Pour reprendre un téléchargement, un client peut ajouter `If-Range` : la plage n'est alors servie que si `If-Range` est égal à la date `Last-Modified` du fichier. Sinon, le fichier a changé, et il est envoyé en entier avec un `200`. Les ETag de Vitesse sont faibles : un ETag dans `If-Range` ne correspond donc jamais, et donne lui aussi le fichier entier.

## Les gros fichiers

Les fichiers jusqu'à 256 Kio sont lus d'un coup ; les plus gros sont envoyés en flux, par morceaux de 64 Kio, avec leur `Content-Length` exact. La mémoire utilisée reste donc constante, même pour une vidéo de plusieurs gigaoctets. Pour une requête `HEAD`, seuls les en-têtes sont envoyés.

## Les types MIME

| Extensions | `Content-Type` |
|---|---|
| `html`, `htm` | `text/html; charset=utf-8` |
| `css` | `text/css; charset=utf-8` |
| `js`, `mjs`, `cjs` | `text/javascript; charset=utf-8` |
| `json`, `map` | `application/json` |
| `txt`, `log`, `md`, `csv` | `text/plain`, `text/markdown`, `text/csv` (`charset=utf-8`) |
| `xml`, `webmanifest` | `application/xml`, `application/manifest+json` |
| `svg`, `png`, `jpg`/`jpeg`, `gif`, `webp`, `avif`, `ico`, `bmp` | `image/...` (`image/svg+xml` pour SVG, `image/x-icon` pour `ico`) |
| `woff`, `woff2`, `ttf`, `otf` | `font/...` |
| `pdf`, `zip`, `gz`, `tar`, `wasm` | `application/pdf`, `application/zip`, `application/gzip`, `application/x-tar`, `application/wasm` |
| `mp3`, `ogg`, `wav`, `mp4`, `webm` | `audio/mpeg`, `audio/ogg`, `audio/wav`, `video/mp4`, `video/webm` |
| tout le reste | `application/octet-stream` |

## Sécurité

`ServeDir` ne sert jamais que des fichiers situés dans son dossier :

- les segments `..`, les barres obliques inverses et les octets nuls sont refusés, y compris lorsqu'ils sont encodés (`%2e%2e%2f`) ;
- les fichiers et dossiers cachés (`.env`, `.git/`, `.htpasswd`…) sont refusés, sauf si vous activez `dotfiles(true)` ;
- seules les méthodes `GET` et `HEAD` sont servies.

Un chemin refusé est traité comme un fichier absent : `404` avec `static_dir`, middleware ou route suivante avec `app.middleware(ServeDir::new(...))`.

> [!WARNING]
> Servez un dossier dédié (`public/`, `dist/`), jamais la racine du projet : votre `Cargo.toml`, vos sources ou un fichier de configuration deviendraient téléchargeables. Les liens symboliques présents dans le dossier sont suivis : n'y placez pas de lien vers des emplacements sensibles.

Vitesse ne compresse pas les réponses (gzip, brotli). En production, placez un reverse proxy ou un CDN devant le serveur pour la compression et le TLS (voir [Production](production.md)).

## Envoyer un fichier précis

Pour envoyer un fichier précis depuis un handler, utilisez `res::file(chemin).await`, ou `res::download(chemin, nom).await` pour déclencher un téléchargement (voir [Répondre](responses.md)). Comme `res.sendFile` en Express, ces helpers gèrent le cache et les plages exactement comme `ServeDir` : en-têtes `ETag` et `Last-Modified`, `304 Not Modified`, `206 Partial Content` (avec `If-Range`) et `416`, comme décrit plus haut. `res::download` conserve son en-tête `Content-Disposition: attachment` sur les réponses partielles aussi. Seule la durée de cache n'est pas configurable : ils envoient toujours `max-age=0`.

## Les applications monopages (SPA)

Une application React, Vue ou Svelte gère ses propres routes dans le navigateur : `/users/42/profile` doit renvoyer `index.html`, tandis que les vrais fichiers (`/assets/app.js`) et l'API fonctionnent normalement. Combinez le middleware `ServeDir` avec un handler de repli :

```rust
let mut api = Router::new();
api.get("/users", |_| async { Json(vec!["ada", "grace"]) });

let mut app = App::new();
app.mount("/api", api);
app.middleware(ServeDir::new("dist"));
app.fallback(|req: Request| async move {
    // Les requêtes d'API et les autres méthodes gardent un vrai 404.
    if req.method() != Method::GET || req.path().starts_with("/api/") {
        return Error::not_found(format!("Cannot {} {}", req.method(), req.path())).into_response();
    }
    res::file("dist/index.html").await
});
```

Pour une requête, l'ordre est le suivant : d'abord les fichiers de `dist/` (le middleware s'exécute avant le routage), puis les routes de l'API, puis `index.html` pour tout le reste. Un fichier nommé `dist/api/...` masquerait donc une route de l'API.
