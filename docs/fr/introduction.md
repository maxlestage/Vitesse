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

Requêtes par seconde, et entre parenthèses le temps CPU consommé par le serveur pour chaque requête (plus c'est bas, mieux c'est) :

| Scénario | Express 5 | Drogon 1.9 (C++) | axum 0.8 | actix-web 4 | **Vitesse** |
|---|---:|---:|---:|---:|---:|
| `GET /` (texte) | 5 921 (180 µs) | 203 244 (9,7 µs) | 168 165 (11,7 µs) | 274 195 (7,1 µs) | **291 415 (6,1 µs)** |
| `GET /json` | 5 765 (184 µs) | 120 489 (16,5 µs) | 165 988 (11,9 µs) | 248 987 (7,9 µs) | **313 438 (5,9 µs)** |
| `GET /json` envoyé par un navigateur (12 en-têtes) | 5 627 (188 µs) | 92 041 (21,7 µs) | 131 710 (15,0 µs) | 179 505 (10,9 µs) | **290 478 (6,8 µs)** |
| `GET /users/:id` (paramètre + JSON) | 5 627 (187 µs) | 98 486 (20,1 µs) | 151 303 (13,1 µs) | 209 173 (9,4 µs) | **304 323 (6,2 µs)** |
| `POST /echo` (lit et renvoie du JSON) | 4 538 (235 µs) | 69 876 (28,4 µs) | 105 570 (18,8 µs) | 170 129 (11,7 µs) | **253 941 (7,6 µs)** |
| `GET /` pipeliné ×16 | 8 352 (128 µs) | 724 059 (2,7 µs) | 213 678 (9,3 µs) | 1 272 510 (1,6 µs) | **2 781 541 (0,64 µs)** |

En résumé :

- **Contre actix-web**, souvent présenté comme le framework Rust le plus rapide : jusqu'à **+62 %** de débit avec une vraie requête de navigateur, +45 à +49 % avec des paramètres ou un corps JSON, **2,2 fois plus** en pipeline, et 15 à 59 % de CPU en moins par requête.
- **Contre axum** : de 1,7 à 2,4 fois plus de requêtes par seconde, deux fois moins de CPU par requête, et 13 fois plus en pipeline.
- **Contre Drogon (C++)** : de 1,4 à 3,8 fois plus rapide.
- **Contre Express** : environ 50 fois plus rapide.

Toutes ces mesures ont été faites sur une VM de 4 vCPU : le serveur épinglé sur 2 cœurs, wrk sur les 2 autres, 128 connexions keep-alive et 10 s par scénario. Le serveur Express utilisé pour ces chiffres a depuis été retiré du dépôt pour qu'il reste 100 % Rust (il figure toujours dans l'historique git), et le benchmark compare désormais Drogon, axum, actix-web et Vitesse. La page [Performances](performance.md) détaille la méthode, explique comment reproduire les chiffres (avec l'outil Rust de `bench/runner`) et pourquoi Vitesse est rapide. Les résultats bruts figurent aussi dans le README.

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

La référence complète de l'API est générée à partir du code source. Lancez `cargo doc --open` dans un projet qui dépend de Vitesse, ou parcourez le code sur [GitHub](https://maxlestage.github.io/Vitesse/).
