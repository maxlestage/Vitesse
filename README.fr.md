[English](README.md) · **Français** · [Español](README.es.md)

# Vitesse ⚡

**Le confort d'Express.js, la vitesse de Rust.**

[![CI](https://github.com/maxlestage/Vitesse/actions/workflows/ci.yml/badge.svg)](https://github.com/maxlestage/Vitesse/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/vitesse.svg)](https://crates.io/crates/vitesse)
[![docs.rs](https://img.shields.io/docsrs/vitesse)](https://docs.rs/vitesse)
[![Licence : MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#licence)

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

- **Familier** : routes et paramètres, middlewares avec `next`, routeurs,
  fichiers statiques, cookies, JSON et formulaires, et un client de test en
  mémoire.
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

<sub>VM 4 vCPU : serveur épinglé sur 2 cœurs, [wrk](https://github.com/wg/wrk)
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
cargo add vitesse
cargo add serde --features derive   # pour vos structures JSON
```

Ou, pour suivre la version en développement sur GitHub :

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse" }
serde = { version = "1", features = ["derive"] }
```

Vitesse demande Rust 1.85 ou plus récent. Pas besoin de `#[tokio::main]` :
`app.run(port)` crée le runtime, utilise tous les cœurs et s'arrête
proprement sur `Ctrl+C` / `SIGTERM`. Si vous avez déjà un runtime tokio,
utilisez plutôt `app.listen(port).await`.

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

## Documentation

- **Site** : https://maxlestage.github.io/Vitesse/#/docs
- **Dans ce dépôt** : [docs/fr/README.md](docs/fr/README.md), de
  [votre première application](docs/fr/first-app.md) à la
  [mise en production](docs/fr/production.md), avec un guide pour
  [venir d'Express](docs/fr/from-express.md).
- **Référence de l'API** : [docs.rs/vitesse](https://docs.rs/vitesse)

La documentation existe aussi en [anglais](docs/en/README.md) et en
[espagnol](docs/es/README.md).

## Déployer sur Heroku

[![Deploy to Heroku](https://www.herokucdn.com/deploy/button.svg)](https://www.heroku.com/deploy?template=https://github.com/maxlestage/Vitesse)

Un geste suffit pour déployer l'application de démonstration
(`examples/demo.rs`) sur votre compte Heroku : Heroku construit lui-même
l'image Docker, vous n'avez même pas besoin d'un ordinateur (Heroku n'a plus
d'offre gratuite : un dyno payant est nécessaire). Le guide pas à pas couvre
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
| `express.json()` + `req.body` | `req.json::<T>().await?` |
| `next()` | `next.run(req).await` |
| `req.params.id` | `req.param("id")` ou `req.param_as::<u64>("id")?` |
| `req.query.q` | `req.query("q")` ou `req.query_as::<T>()?` |
| `res.send('texte')` | renvoyer `"texte"` |
| `res.status(201).json(obj)` | `res::status(201).json(obj)` ou `(201, Json(obj))` |
| `res.redirect('/login')` | `Redirect::to("/login")` |
| `app.listen(3000)` | `app.run(3000)` |

La table complète se trouve dans [Venir d'Express](docs/fr/from-express.md).

## Pourquoi c'est rapide

La quasi-totalité du temps d'une requête simple est passée dans le noyau, à
lire et écrire le socket : un serveur rapide est un serveur qui ajoute le
moins possible autour. Vitesse fait exactement **un `read` et un `write` par
requête**, et un seul de chaque pour tout un lot de requêtes pipelinées.

- **Un moteur HTTP/1.1 maison** : la tête des requêtes est analysée par
  [httparse](https://github.com/seanmonstar/httparse) (SIMD) sans copie, les
  en-têtes et l'URI ne sont construits que si un handler les demande, et les
  réponses sont écrites directement dans un tampon réutilisé.
- **Presque aucune allocation** : les requêtes, les tampons et les tables
  d'en-têtes des réponses sont recyclés par thread.
- **Un thread par cœur** (Linux) : chaque cœur a sa propre boucle
  d'événements et son propre socket `SO_REUSEPORT`, et une requête ne change
  jamais de thread.
- **Un routeur sans regex** : un arbre de segments, sans allocation pour les
  routes statiques.
- **Zéro compteur atomique partagé par requête** : l'application est figée
  au démarrage (`&'static`), handlers, middlewares et état sont lus sans
  `Arc`.

Les détails sont dans [Performances](docs/fr/performance.md).

## Limites actuelles

Vitesse fait volontairement peu de choses, comme Express. Ne sont pas
(encore) inclus : HTTP/2 et TLS (à placer derrière un reverse proxy comme
Nginx ou Caddy, comme on le fait souvent avec Express : voir
[Mise en production](docs/fr/production.md)), WebSocket, compression,
moteurs de templates, et paramètres partiels dans un segment
(`/vols/:de-:vers`).

## Lancer le projet

```sh
cargo run --release --example hello      # Hello World
cargo run --release --example rest_api   # API CRUD complète
cargo run --release --example demo       # l'application déployée sur Heroku (lit $PORT)
cargo test                               # tests unitaires, d'intégration et doctests
cargo run --release --manifest-path bench/runner/Cargo.toml   # benchmark (Linux, wrk, et Drogon si installé)
docker build -t vitesse-demo . && docker run --rm -p 8080:8080 vitesse-demo
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
les modifiez, lancez aussi `cargo test` dans leur dossier.

La documentation se trouve dans [`docs/`](docs), en anglais, en français et
en espagnol : quand vous modifiez une page, mettez aussi à jour les autres
langues, ou signalez-le dans votre pull request. Les changements notables
vont dans [`CHANGELOG.md`](CHANGELOG.md).

### Publier une version (mainteneurs)

1. Augmentez `version` dans `Cargo.toml` et mettez à jour `CHANGELOG.md`.
2. Sur GitHub, ouvrez **Releases** → **Draft a new release**, créez un tag
   `vX.Y.Z` qui correspond à la version, et publiez la release.
3. Le workflow `Release` lance les tests et publie la crate sur crates.io.
   Il a besoin du secret de dépôt `CARGO_REGISTRY_TOKEN` (un jeton d'API
   crates.io avec les portées `publish-new` et `publish-update`). Il peut
   aussi être lancé à la main depuis l'onglet **Actions**, en mode « dry
   run » (vérification seule) par défaut.

## Licence

Sous licence [Apache 2.0](LICENSE-APACHE) ou [MIT](LICENSE-MIT), au choix.

Sauf mention contraire de votre part, toute contribution que vous soumettez
intentionnellement pour inclusion dans ce projet, au sens de la licence
Apache-2.0, sera placée sous cette double licence, sans aucune condition
supplémentaire.
