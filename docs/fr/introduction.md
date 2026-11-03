# Introduction

Vitesse est un framework web minimaliste pour Rust qui reprend l'API d'Express.js. Vous écrivez `app.get("/users/:id", ...)`, vous lisez `req.param("id")`, vous répondez avec `res::status(201).json(...)` et vous enchaînez les middlewares avec `next`. La différence, c'est que tout s'exécute en code natif, avec les performances qui vont avec.

## Qu'est-ce que Vitesse ?

Vitesse est une bibliothèque pour construire des serveurs HTTP, des sites et des API JSON. Elle embarque son propre moteur HTTP/1.1, bâti sur [tokio](https://tokio.rs), un routeur en arbre qui n'exécute une expression régulière que lorsqu'une route en demande une, une chaîne de middlewares et un ensemble de middlewares prêts à l'emploi, ainsi que WebSocket et, en option, HTTPS, HTTP/2, HTTP/3 sur QUIC, les sessions et les gabarits HTML.

Voici un serveur complet :

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    app.run(3000)
}
```

Si vous avez déjà écrit du code Express, vous savez sans doute déjà le lire :

```js
const express = require('express');
const app = express();

app.get('/', (req, res) => res.send('Hello World!'));

app.listen(3000);
```

## À qui s'adresse-t-il ?

- **Aux développeurs Express et Node.js** qui veulent la vitesse, la sobriété en mémoire et la fiabilité de Rust, sans réapprendre toute leur façon de concevoir un serveur web.
- **Aux développeurs Rust** qui cherchent un framework petit et explicite. Pas d'extracteurs ni de macros procédurales : un handler reçoit une `Request` et renvoie une réponse.
- **À tous ceux qui paient des serveurs** : moins de CPU par requête, c'est moins de machines, ou des machines plus petites.

Pas besoin d'être expert en Rust. La plupart des handlers tiennent en quelques lignes, et cette documentation explique les subtilités propres à Rust au fil des pages.

## Philosophie

- **Un petit noyau, comme Express.** Vitesse couvre le routage, les middlewares, des outils pour lire les requêtes et construire les réponses, les fichiers statiques, WebSocket et un client de test. Les sessions et les gabarits sont des features à activer, et vous choisissez vos crates pour la base de données et le reste.
- **Une API familière.** La plupart des notions d'Express se retrouvent telles quelles dans Vitesse. La correspondance complète est dans [Venir d'Express](from-express.md).
- **Pas de magie.** Un handler demande à la requête ce dont il a besoin (`req.param("id")`, `req.json().await`). Les erreurs sont des valeurs ordinaires, et `?` les transforme en réponses HTTP.
- **Rapide par défaut.** Les chiffres ci-dessous ne demandent aucun réglage. `app.run(3000)` répartit déjà le travail sur tous les cœurs.
- **Robuste par défaut.** Un handler qui panique donne une `500`, un corps trop gros une `413`, et une requête malformée est rejetée. `Ctrl+C` et `SIGTERM` déclenchent un arrêt propre qui laisse finir les requêtes en cours.

## Fonctionnalités clés

- **Routage à la Express** : `get`, `post`, `put`, `patch`, `delete`, `all`, paramètres (`/users/:id`, `/flights/:from-:to`), contraintes (`/users/:id(\d+)`), paramètres optionnels (`/archive/:year?`) et jokers (`/files/*path`). `HEAD`, `OPTIONS` et `405 Method Not Allowed` sont gérés pour vous, et les routes nommées construisent leurs propres URL (`req.path_for("user", &[&42])`).
- **Entrées typées** : `req.param_as::<u64>("id")?`, `req.query_as::<T>()?`, `req.json::<T>().await?` et `req.form::<T>().await?` répondent `400` ou `413` d'eux-mêmes quand l'entrée est invalide.
- **Réponses souples** : renvoyez un `&str`, une `String`, `Json(...)`, `json!({...})`, `Html(...)`, un tuple `(statut, corps)`, une `Option` ou un `Result`. Vous pouvez aussi construire la réponse vous-même avec `res::status(201).header(...).json(...)`.
- **Middlewares** à trois niveaux (global, par routeur, par route) avec `next.run(req).await`. Fournis : `logger`, `cors`, `helmet`, `timeout`, identifiants de requête, authentification `Basic` et `Bearer`, limites de débit et de concurrence, taille maximale du corps par route, et compression dans les deux sens (feature `compress`).
- **Des routeurs** à monter sous un préfixe, chacun avec ses propres middlewares et son propre fallback, un **état partagé** pour toute l'application et des **données par requête** posées par les middlewares.
- **Un seul type d'erreur** (`vitesse::Error`), qui devient `{"error": "..."}`. `app.on_error` permet de changer le format de toutes les réponses d'erreur.
- **Fichiers statiques** avec types MIME, `ETag`/`Last-Modified`, requêtes `Range`, et protection contre `../` et les fichiers cachés.
- **Cookies, redirections, téléchargements et streaming** des corps dans les deux sens, **négociation de contenu** (`req.accepts(&["html", "json"])`) et **envois de fichiers** (feature `multipart`) lus en flux.
- **Cookies signés et sessions** (feature `session`) : `req.sign_cookie(...)`, et `req.session()` avec un stockage en mémoire ou le vôtre.
- **Gabarits HTML** (feature `templates`) : `app.views("templates")` et `req.render("page.html", json!({...}))`, avec la syntaxe Jinja2 de minijinja.
- **Server-Sent Events** avec `vitesse::sse` : poussez des mises à jour en direct vers l'`EventSource` du navigateur.
- **WebSocket** avec `app.ws("/chat", |req, socket| async move { … })`, dans le style d'`express-ws` : paramètres de route, middlewares et état fonctionnent comme dans n'importe quelle route, en HTTP/1.1 et en HTTP/2.
- **HTTPS et HTTP/2**, avec les features `tls` et `http2` à activer : aucun reverse proxy n'est nécessaire. **HTTP/3 sur QUIC** avec la feature `http3` : la même application répond aussi en UDP.
- **Des réglages pour la production** : keep-alive et délais, une limite de connexions, des proxys de confiance (`app.trust_proxy`, pour la vraie adresse du client), les sockets Unix.
- **Interopérabilité avec tower** (feature `tower`) : utilisez les middlewares de tower et de tower-http, ou servez l'application comme un `Service` tower.
- **Un `TestClient`** qui teste votre application en mémoire, sans ouvrir de port.
- **Un runtime géré** : pas besoin de `#[tokio::main]`. Vitesse fait tourner un thread par cœur sous Linux et s'arrête proprement.

## C'est rapide à quel point ?

Requêtes par seconde, et entre parenthèses le temps CPU consommé par le serveur pour chaque requête (plus c'est bas, mieux c'est), mesurées avec le nouveau moteur par thread de Vitesse :

| Scénario | axum 0.8 | actix-web 4 | **Vitesse** |
|---|---:|---:|---:|
| `GET /` (texte) | 104 439 (18,8 µs) | 174 475 (11,0 µs) | **175 956 (10,1 µs)** |
| `GET /json` | 101 206 (19,3 µs) | 152 498 (12,6 µs) | **173 191 (10,4 µs)** |
| `GET /json` envoyé par un navigateur (12 en-têtes) | 86 681 (22,7 µs) | 138 917 (14,1 µs) | **165 337 (11,0 µs)** |
| `GET /users/:id` (paramètre + JSON) | 96 321 (20,1 µs) | 152 877 (12,8 µs) | **178 731 (10,0 µs)** |
| `POST /echo` (lit et renvoie du JSON) | 81 171 (24,4 µs) | 115 069 (17,1 µs) | **171 438 (10,8 µs)** |
| `GET /` pipeliné ×16 | 171 985 (11,5 µs) | 983 771 (2,0 µs) | **1 770 173 (0,97 µs)** |

En résumé :

- **Contre actix-web**, souvent présenté comme le framework Rust le plus rapide : +0,8 % sur `GET /`, où tous deux butent sur le plancher du noyau, +13,6 % sur `GET /json`, +19 % avec une vraie requête de navigateur, +16,9 % avec un paramètre, **+49 %** sur l'écho JSON, **1,8 fois plus** en pipeline, et 8 à 52 % de CPU en moins par requête.
- **Contre axum** : de 1,7 à 2,1 fois plus de requêtes par seconde, environ deux fois moins de CPU par requête, et 10 fois plus en pipeline.
- **Contre Drogon (C++) et Express** : mesurés lors d'une session antérieure, sur une autre machine, avant le moteur par thread. Vitesse était alors de 1,4 à 3,8 fois plus rapide que Drogon et environ 50 fois plus rapide qu'Express.

Tous les chiffres du tableau ont été mesurés le 3 novembre 2026, en une seule session, sur une VM de 4 vCPU : le serveur épinglé sur 2 cœurs, wrk sur les 2 autres, 128 connexions keep-alive, 5 s par exécution et la médiane de 3 exécutions, les serveurs l'un après l'autre. Versions : axum 0.8, actix-web 4.18 et Rust 1.97 ; Vitesse utilise son allocateur par défaut (mimalloc), axum et actix-web l'allocateur système. Cette VM est plus lente que celle des mesures précédentes : comparez les colonnes entre elles, pas avec d'anciens chiffres. Drogon et Express n'ont pas pu être remesurés sur cette machine ; le serveur Express a depuis été retiré du dépôt pour qu'il reste 100 % Rust (il figure toujours dans l'historique git), et le benchmark compare désormais Drogon, axum, actix-web et Vitesse. La page [Performances](performance.md) détaille la méthode, explique comment reproduire les chiffres (avec l'outil Rust de `bench/runner`) et pourquoi Vitesse est rapide. Les résultats bruts figurent aussi dans le README.

## Limites actuelles

Quelques choses sont laissées de côté, chacune pour une raison :

- Les extracteurs : par choix, un handler prend une seule `Request` et lui demande ce dont il a besoin.
- WebSocket sur HTTP/3 : les navigateurs ne s'en servent pas, et ouvrent plutôt leurs [WebSockets](websocket.md) en HTTP/2 ou en HTTP/1.1.
- Le server push HTTP/2 et `Upgrade: h2c`, que les navigateurs et la spécification HTTP/2 ont abandonnés, et le 0-RTT en HTTP/3, qui expose les requêtes au rejeu.
- D'autres runtimes que tokio.
- Une API 1.x stable : Vitesse en est à la version 0.1.

La [FAQ](faq.md#limites-actuelles) en donne les raisons en détail.

## Organisation de la documentation

- **Premiers pas** : [Installation](installation.md), puis [Votre première application](first-app.md), un tutoriel pas à pas.
- **L'essentiel** : [Routage](routing.md), [Lire la requête](requests.md), [Envoyer la réponse](responses.md), [Gabarits HTML](templates.md), [Middlewares](middleware.md), [Routeurs](routers.md), [État partagé](state.md), [Cookies signés et sessions](sessions.md), [Gestion des erreurs](errors.md), [Fichiers statiques](static-files.md) et [WebSocket](websocket.md).
- **Pour aller plus loin** : [Tests](testing.md), [Configuration du serveur](server.md) (adresses, threads, délais, arrêt propre), [HTTPS et HTTP/2](https.md), [HTTP/3 et QUIC](http3.md), [Performances](performance.md), [Venir d'Express](from-express.md) et [Venir d'axum ou d'actix-web](from-axum-actix.md).
- **Déploiement** : [Déployer sur Heroku depuis un téléphone](heroku-mobile.md), [Docker](docker.md) et [Mise en production](production.md).
- **Référence** : l'[aide-mémoire](cheatsheet.md) et la [FAQ](faq.md).

> [!TIP]
> Si vous connaissez bien Express, faites [Votre première application](first-app.md), puis gardez [Venir d'Express](from-express.md) ouvert pendant que vous écrivez vos propres routes.

La référence complète de l'API est générée à partir du code source. Lancez `cargo doc --open` dans un projet qui dépend de Vitesse, ou parcourez le code sur [GitHub](https://github.com/maxlestage/Vitesse).
