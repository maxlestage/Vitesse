[![Vitesse : le confort d'Express, la vitesse de Rust](https://raw.githubusercontent.com/maxlestage/Vitesse/master/site/assets/og-image.png)](https://maxlestage.github.io/Vitesse/)

[English](README.md) · **Français** · [Español](README.es.md)

# Vitesse ⚡

**Le confort d'Express.js, la vitesse de Rust.**

[![crates.io](https://img.shields.io/crates/v/vitesse.svg)](https://crates.io/crates/vitesse)
[![docs.rs](https://img.shields.io/docsrs/vitesse)](https://docs.rs/vitesse)
[![Licence : propriétaire](https://img.shields.io/badge/license-proprietary-2a2a2e.svg)](#licence)

Vitesse est un framework web minimaliste qui reprend l'API d'Express
(`app.get`, `req.params`, `res.status(201).json(...)`, `app.use`, `Router`,
`express.static`…) en Rust natif, avec son propre moteur HTTP/1.1 sur
[tokio](https://tokio.rs), WebSocket intégré, et HTTPS, HTTP/2 et HTTP/3 sur
QUIC en option.

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    app.run(3000)
}
```

- **Familier** : routes et paramètres, middlewares avec `next`, routeurs,
  fichiers statiques, cookies, JSON, formulaires et envois de fichiers, et un
  client de test en mémoire.
- **Temps réel et protocoles modernes** : WebSocket avec `app.ws`,
  Server-Sent Events, HTTPS et HTTP/2 (features `tls` et `http2`), et HTTP/3
  sur QUIC en une ligne (feature `http3`).
- **Batteries incluses** : CORS, compression, identifiants de requête,
  authentification Basic et Bearer, limites de débit et de concurrence,
  proxies de confiance, cookies signés et sessions (feature `session`),
  gabarits HTML (feature `templates`), et n'importe quelle couche
  [tower](https://crates.io/crates/tower) (feature `tower`).
- **Des routes expressives** : `/flights/:from-:to`, `/users/:id(\d+)`,
  `/archive/:year?`, routes nommées et génération d'URL.
- **Rapide** : plus rapide qu'actix-web, axum et Drogon dans le benchmark
  ci-dessous.
- **Solide** : les paniques deviennent des réponses `500`, les limites de
  taille et les délais sont intégrés, et le serveur s'arrête proprement.

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
- **Contre Express** : environ 50 fois plus rapide.

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

<sub>VM 4 vCPU : serveur épinglé sur 2 cœurs, wrk
sur les 2 autres, 128 connexions keep-alive, 10 s par scénario, même machine
et même session pour tous. Node 22.22 / Express 5.3.0, Drogon 1.9.13 (GCC 13,
`-O3`), axum 0.8, actix-web 4.15, Rust 1.97, allocateur système partout. Sans
pipeline, les serveurs les plus rapides saturent wrk : le temps CPU par
requête, mesuré côté serveur, est alors le juge le plus fiable. Les mesures
varient de quelques pourcents d'une exécution à l'autre. Pour reproduire :
`cargo run --release --manifest-path bench/runner/Cargo.toml` (code des
serveurs dans `bench/`). Le serveur Express, mesuré dans la même session, a
depuis été retiré du dépôt pour que le projet reste 100 % Rust, sans
JavaScript : il figure toujours dans l'historique git
(`git show 484eed3:bench/express/server.js`), et l'outil de benchmark compare
désormais Drogon, axum, actix-web et Vitesse.</sub>

## Installation

```sh
cargo add vitesse --git https://github.com/maxlestage/Vitesse
cargo add serde --features derive   # pour vos structures JSON
```

Vitesse demande Rust 1.85 ou plus récent. Pas besoin de `#[tokio::main]` :
`app.run(port)` crée le runtime, utilise tous les cœurs et s'arrête
proprement sur `Ctrl+C` / `SIGTERM`. Si vous avez déjà un runtime tokio,
utilisez plutôt `app.listen(port).await`.

### Features Cargo

| Feature | Par défaut | Apporte |
|---|---|---|
| `ws` | Activée | WebSocket : `app.ws(...)` et `vitesse::ws` ([guide](docs/fr/websocket.md)) |
| `tls` | Désactivée | HTTPS : `app.tls(...)` et `vitesse::tls`, avec rustls ([guide](docs/fr/https.md)) |
| `http2` | Désactivée | HTTP/2 : via ALPN avec `tls`, et h2c en clair ([guide](docs/fr/https.md)) |
| `http3` | Désactivée | HTTP/3 sur QUIC : `app.http3(...)` et `vitesse::http3` ([guide](docs/fr/http3.md)) |
| `multipart` | Désactivée | Envois de fichiers : `req.multipart()` et `vitesse::multipart` ([guide](docs/fr/requests.md)) |
| `compress` | Désactivée | `middleware::compress()` et `middleware::decompress()` : Brotli, gzip ([guide](docs/fr/middleware.md#compress)) |
| `session` | Désactivée | Cookies signés et sessions : `app.cookie_secret(...)`, `middleware::session(...)` ([guide](docs/fr/sessions.md)) |
| `templates` | Désactivée | Gabarits HTML avec minijinja : `app.views(...)`, `req.render(...)` ([guide](docs/fr/templates.md)) |
| `tower` | Désactivée | Les couches tower comme middlewares, l'application comme `Service` tower ([guide](docs/fr/middleware.md)) |

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tls", "http2"] }
```

## Tour d'horizon

```rust
use serde::Deserialize;
use vitesse::prelude::*;

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

// Middleware de route : ne laisse passer que les requêtes avec le bon jeton.
async fn auth(req: Request, next: Next) -> Response {
    match req.header("authorization") {
        Some("Bearer secret") => next.run(req).await,
        _ => Error::unauthorized("jeton manquant ou invalide").into_response(),
    }
}

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    // Middlewares globaux (app.use) : pour chaque requête, dans l'ordre.
    app.middleware(middleware::logger());
    app.middleware(|req: Request, next: Next| async move {
        let start = std::time::Instant::now();
        let res = next.run(req).await;
        res.header("x-response-time", format!("{:.3}ms", start.elapsed().as_secs_f64() * 1000.0))
    });

    app.get("/", |_| async { "Hello World!" });

    // Paramètres de route : 400 automatique si `id` n'est pas un nombre.
    app.get("/users/:id", |req: Request| async move {
        let id: u32 = req.param_as("id")?;
        Ok::<_, Error>(Json(json!({ "id": id, "name": "Ada" })))
    });

    // Corps JSON en entrée, 201 Created en sortie (400 si le JSON est invalide).
    app.post("/users", |req: Request| async move {
        let user: NewUser = req.json().await?;
        Ok::<_, Error>((201, Json(json!({ "name": user.name }))))
    });

    // Un routeur monté sous un préfixe, protégé par un middleware.
    let mut admin = Router::new();
    admin.middleware(auth);
    admin.get("/stats", |_| async { json!({ "users": 1 }) });
    app.mount("/admin", admin);

    app.run(3000)
}
```

```sh
curl localhost:3000/users/42      # {"id":42,"name":"Ada"}
curl -X POST localhost:3000/users -H 'content-type: application/json' -d '{"name":"Ada"}'
curl localhost:3000/admin/stats -H 'authorization: Bearer secret'
```

### WebSocket

```rust
// Une route WebSocket, dans le style d'express-ws : la requête, puis le socket.
app.ws("/echo/:name", |req, mut socket| async move {
    let name = req.param("name").unwrap_or("inconnu").to_owned();
    while let Some(Ok(message)) = socket.recv().await {
        if let ws::Message::Text(text) = message {
            if socket.send(format!("{name} a dit : {text}")).await.is_err() {
                break;
            }
        }
    }
});
```

Essayez-la avec [websocat](https://crates.io/crates/websocat) :
`websocat ws://localhost:3000/echo/ada`. Sous-protocoles, limites de taille,
`split` et un salon de discussion complet : [WebSocket](docs/fr/websocket.md).

### HTTP/3

```rust
use vitesse::http3::Http3;

// Avec la feature `http3` : la même application, servie aussi sur QUIC (UDP).
app.http3(Http3::from_pem_files("fullchain.pem", "privkey.pem")?);
app.run(443) // HTTP/1.1 sur TCP 443, HTTP/3 sur UDP 443
```

Certificats, `Alt-Svc` et déploiement derrière Caddy ou Nginx :
[HTTP/3 et QUIC](docs/fr/http3.md).

### HTTPS et HTTP/2

```rust
use vitesse::tls::Tls;

// Avec les features `tls` et `http2` : TLS avec rustls, HTTP/2 pour les navigateurs.
app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?);
app.run(443)
```

Sans TLS, la feature `http2` répond aussi aux clients qui parlent HTTP/2 en
clair (h2c). Voir [HTTPS et HTTP/2](docs/fr/https.md).

### Server-Sent Events

```rust
use vitesse::sse::Event;

app.get("/clock", |_| async {
    let (tx, events) = sse::channel(16);
    tokio::spawn(async move {
        for tick in 0.. {
            // Échoue dès que le navigateur s'est déconnecté.
            if tx.send(Event::new().data(tick.to_string())).await.is_err() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });
    events
});
```

## Documentation

- **Site** : https://maxlestage.github.io/Vitesse/fr/docs/
- **Dans ce dépôt** : [docs/fr/README.md](docs/fr/README.md), de
  [votre première application](docs/fr/first-app.md) à la
  [mise en production](docs/fr/production.md), avec un guide pour
  [venir d'Express](docs/fr/from-express.md), et les guides
  [WebSocket](docs/fr/websocket.md) et [HTTP/3](docs/fr/http3.md).
- **Référence de l'API** : [docs.rs/vitesse](https://docs.rs/vitesse)

La documentation existe aussi en [anglais](docs/en/README.md) et en
[espagnol](docs/es/README.md).

## Déployer sur Heroku

Ouvrez `https://heroku.com/deploy?template=https://github.com/maxlestage/Vitesse` sur votre
téléphone pour déployer l'application de démonstration (`examples/demo.rs`) sur
votre compte Heroku : Heroku lit `app.json` et compile lui-même l'application avec
le buildpack Rust (`emk/rust` ; une construction Docker reste possible), vous n'avez
même pas besoin d'un ordinateur (Heroku n'a plus d'offre gratuite : un
dyno payant est nécessaire). Le guide pas à pas couvre
aussi les déploiements automatiques avec GitHub Actions :
[Déployer sur Heroku depuis un téléphone](docs/fr/heroku-mobile.md).

## D'Express à Vitesse

| Express | Vitesse |
|---|---|
| `const app = express()` | `let mut app = App::new();` |
| `app.get('/u/:id', (req, res) => …)` | `app.get("/u/:id", \|req: Request\| async move { … })` |
| `app.use(fn)` | `app.middleware(fn)` |
| `app.use('/api', router)` | `app.mount("/api", router)` |
| `express.Router()` | `Router::new()` |
| `app.use(express.static('public'))` | `app.middleware(ServeDir::new("public"))` |
| `app.ws('/chat', (ws, req) => …)` (express-ws) | `app.ws("/chat", \|req, socket\| async move { … })` |
| `express.json()` + `req.body` | `req.json::<T>().await?` |
| `next()` | `next.run(req).await` |
| `req.params.id` | `req.param("id")` ou `req.param_as::<u64>("id")?` |
| `req.query.q` | `req.query("q")` ou `req.query_as::<T>()?` |
| `res.send('texte')` | renvoyer `"texte"` |
| `res.status(201).json(obj)` | `res::status(201).json(obj)` ou `(201, Json(obj))` |
| `res.redirect('/login')` | `Redirect::to("/login")` |
| `app.listen(3000)` | `app.run(3000)` |

La table complète se trouve dans [Venir d'Express](docs/fr/from-express.md).
Vous venez d'un autre framework Rust ? [Venir d'axum ou
d'actix-web](docs/fr/from-axum-actix.md) fait correspondre les
fonctionnalités des deux, extracteur par extracteur et couche par couche.

## Pourquoi c'est rapide

La quasi-totalité du temps d'une requête simple est passée dans le noyau, à
lire et écrire le socket : un serveur rapide est un serveur qui ajoute le
moins possible autour. Vitesse fait exactement **un `read` et un `write` par
requête**, et un seul de chaque pour tout un lot de requêtes pipelinées.

- **Un moteur HTTP/1.1 maison** : la tête des requêtes est analysée par
  httparse (SIMD) sans copie, les
  en-têtes et l'URI ne sont construits que si un handler les demande, et les
  réponses sont écrites directement dans un tampon réutilisé.
- **Presque aucune allocation** : les requêtes, les tampons et les tables
  d'en-têtes des réponses sont recyclés par thread.
- **Un thread par cœur** (Linux) : chaque cœur a sa propre boucle
  d'événements et son propre socket `SO_REUSEPORT`, et une requête ne change
  jamais de thread.
- **Un routeur en arbre** : un arbre de segments, sans allocation pour les
  routes statiques ; les expressions régulières ne s'exécutent que pour les
  paramètres qui déclarent une contrainte.
- **Zéro compteur atomique partagé par requête** : l'application est figée
  au démarrage (`&'static`), handlers, middlewares et état sont lus sans
  `Arc`.

Les détails sont dans [Performances](docs/fr/performance.md).

## Limites actuelles

Volontairement non inclus : WebSocket sur HTTP/3 (aucun navigateur ne s'en
sert), le server push HTTP/2 (retiré des navigateurs), le mécanisme
`Upgrade: h2c` (retiré du standard HTTP/2 ; h2c en « prior knowledge »
fonctionne) et le 0-RTT en HTTP/3 (requêtes rejouables). tokio est le seul
runtime, et l'API peut changer jusqu'à la 1.0. La liste complète se trouve
dans la [FAQ](docs/fr/faq.md).

## Lancer le projet

```sh
cargo run --release --example hello      # Hello World
cargo run --release --example rest_api   # API CRUD complète
cargo run --release --example demo       # l'application déployée sur Heroku (lit $PORT)
cargo run --release --example chat       # salon de discussion WebSocket (websocat ws://localhost:3000/chat/ada)
cargo run --release --example http3 --features http3   # HTTP/1.1 + HTTP/3 sur le port 4433, certificat auto-signé
cargo run --release --example https --features tls,http2   # HTTPS + HTTP/2 sur le port 3443, certificat auto-signé
cargo run --release --example live --features multipart    # Server-Sent Events et envois de fichiers
cargo test                               # tests unitaires, d'intégration et doctests
cargo run --release --manifest-path bench/runner/Cargo.toml   # benchmark (Linux, wrk, et Drogon si installé)
docker build -t vitesse-demo . && docker run --rm -p 8080:8080 vitesse-demo
```

### Le site de présentation

Le dossier [`site/`](site) contient le site du projet, en français, en
anglais et en espagnol, avec la documentation de [`docs/`](docs). Il est
servi par Vitesse lui-même : le serveur est une application Vitesse dont les
pages sont écrites avec [Active](https://maxlestage.github.io/Active/)
(rendues sur le serveur, avec toutes les balises SEO), et les parties
interactives sont des îlots Active compilés en WebAssembly. C'est une
Progressive Web App : il s'installe sur l'écran d'accueil d'un téléphone et
reste lisible hors ligne. Le workflow `Site` l'exporte en fichiers statiques
et le publie sur GitHub Pages à chaque modification sur `master`.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129   # la version de site/Cargo.lock
cd site
# 1. Le code du navigateur (îlots et animations) → site/pkg
cargo build --lib --target wasm32-unknown-unknown --profile wasm-release
wasm-bindgen --target web --no-typescript --out-dir pkg target/wasm32-unknown-unknown/wasm-release/vitesse_site.wasm
# 2. Le serveur, sur http://localhost:3000 (PORT pour changer de port)
cargo run --release
# Ou tout le site en fichiers statiques, derrière le chemin de GitHub Pages
BASE_PATH=/Vitesse SITE_URL=https://maxlestage.github.io/Vitesse cargo run --release -- export dist
```

Le JavaScript dont les navigateurs ont besoin pour démarrer le WebAssembly et
le service worker sont générés (par wasm-bindgen et par le serveur) : le
dépôt ne contient toujours aucun fichier JavaScript.

## Contribuer

Signalements de bugs, idées, corrections de la documentation et pull
requests sont les bienvenus. Pour un changement important, ouvrez d'abord
une issue pour en discuter.

Avant d'ouvrir une pull request, lancez les mêmes vérifications que la CI
(qui tourne sous Linux, macOS et Windows) :

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

L'outil de benchmark ([`bench/runner`](bench/runner)) et le site
([`site/`](site)) sont des crates à part, hors du build principal : si vous
les modifiez, lancez aussi `cargo test` dans leur dossier (pour le site,
aussi `cargo clippy --lib --target wasm32-unknown-unknown -- -D warnings`).

La documentation se trouve dans [`docs/`](docs), en anglais, en français et
en espagnol : quand vous modifiez une page, mettez aussi à jour les autres
langues, ou signalez-le dans votre pull request. Les changements notables
vont dans [`CHANGELOG.md`](CHANGELOG.md).

### Publier une version (mainteneurs)

1. Augmentez `version` dans `Cargo.toml` et écrivez sa section dans
   `CHANGELOG.md`.
2. Poussez sur `master`. Une fois la CI au vert, le workflow `Release` pose
   le tag de la nouvelle version (`vX.Y.Z`), crée sa release GitHub avec
   cette section de `CHANGELOG.md` comme notes, lance les tests et publie la
   crate sur crates.io. Aucun formulaire à remplir.
3. La publication a besoin du secret de dépôt `CARGO_REGISTRY_TOKEN` (un
   jeton d'API crates.io avec les portées `publish-new` et
   `publish-update`). Sans lui, le tag et la release GitHub sont quand même
   créés, et la publication attend : ajoutez le secret, puis lancez le
   workflow à la main depuis l'onglet **Actions**, case **Only check the
   package** décochée. Une release rédigée à la main (**Releases** →
   **Draft a new release**) publie aussi, et un lancement manuel est en
   mode « dry run » (vérification seule) par défaut.

## Licence

Vitesse est un logiciel propriétaire : © 2026 Maxime Nathan Lestage, tous
droits réservés. Ce n'est pas un logiciel open source. Le code source est
publié sur crates.io et GitHub pour pouvoir être lu, audité et compilé, et
vous pouvez l'utiliser librement comme dépendance de vos propres programmes,
y compris commerciaux. Le modifier ou redistribuer son code source demande
l'accord écrit de l'auteur. Conditions complètes : [LICENSE](LICENSE).

Les contributions (code, documentation, idées) sont les bienvenues et
rejoignent le projet aux mêmes conditions : en soumettre une, c'est en céder
les droits à l'auteur, comme l'indique la licence.

## Crédits

Conçu et développé par Maxime Nathan Lestage,
au sein d'une même famille de projets Rust : [Active](https://maxlestage.github.io/Active/), [Vitesse](https://maxlestage.github.io/Vitesse/), [Securite](https://maxlestage.github.io/Securite/), [Memoire](https://maxlestage.github.io/Memoire/), [Paquets](https://maxlestage.github.io/Paquets/), [FerroSwift](https://maxlestage.github.io/FerroSwift/) et [Solidify](https://maxlestage.github.io/Solidify/).
