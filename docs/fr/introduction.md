# Introduction

Vitesse est un framework web minimaliste pour Rust qui reprend l'API d'Express.js. Vous écrivez `app.get("/users/:id", ...)`, vous lisez `req.param("id")`, vous répondez avec `res::status(201).json(...)` et vous enchaînez les middlewares avec `next`. La différence, c'est que tout s'exécute en code natif, avec les performances qui vont avec.

## Qu'est-ce que Vitesse ?

Vitesse est une bibliothèque pour construire des serveurs HTTP, des sites et des API JSON. Elle embarque son propre moteur HTTP/1.1, bâti sur [tokio](https://tokio.rs), un routeur sans expressions régulières, une chaîne de middlewares et quelques middlewares prêts à l'emploi.

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

- **Un petit noyau, comme Express.** Vitesse couvre le routage, les middlewares, des outils pour lire les requêtes et construire les réponses, les fichiers statiques et un client de test. Base de données, templates, authentification : vous choisissez vos crates.
- **Une API familière.** La plupart des notions d'Express se retrouvent telles quelles dans Vitesse. La correspondance complète est dans [Venir d'Express](from-express.md).
- **Pas de magie.** Un handler demande à la requête ce dont il a besoin (`req.param("id")`, `req.json().await`). Les erreurs sont des valeurs ordinaires, et `?` les transforme en réponses HTTP.
- **Rapide par défaut.** Les chiffres ci-dessous ne demandent aucun réglage. `app.run(3000)` répartit déjà le travail sur tous les cœurs.
- **Robuste par défaut.** Un handler qui panique donne une `500`, un corps trop gros une `413`, et une requête malformée est rejetée. `Ctrl+C` et `SIGTERM` déclenchent un arrêt propre qui laisse finir les requêtes en cours.

## Fonctionnalités clés

- **Routage à la Express** : `get`, `post`, `put`, `patch`, `delete`, `all`, paramètres (`/users/:id`) et jokers (`/files/*path`). `HEAD`, `OPTIONS` et `405 Method Not Allowed` sont gérés pour vous.
- **Entrées typées** : `req.param_as::<u64>("id")?`, `req.query_as::<T>()?`, `req.json::<T>().await?` et `req.form::<T>().await?` répondent `400` ou `413` d'eux-mêmes quand l'entrée est invalide.
- **Réponses souples** : renvoyez un `&str`, une `String`, `Json(...)`, `json!({...})`, `Html(...)`, un tuple `(statut, corps)`, une `Option` ou un `Result`. Vous pouvez aussi construire la réponse vous-même avec `res::status(201).header(...).json(...)`.
- **Middlewares** à trois niveaux (global, par routeur, par route) avec `next.run(req).await`. `logger`, `cors`, `helmet` et `timeout` sont fournis.
- **Des routeurs** à monter sous un préfixe, un **état partagé** pour toute l'application et des **données par requête** posées par les middlewares.
- **Un seul type d'erreur** (`vitesse::Error`), qui devient `{"error": "..."}`. `app.on_error` permet de changer le format de toutes les réponses d'erreur.
- **Fichiers statiques** avec types MIME, `ETag`/`Last-Modified`, requêtes `Range`, et protection contre `../` et les fichiers cachés.
- **Cookies, redirections, téléchargements et streaming** des corps dans les deux sens.
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
- **Contre Express** : environ 50 fois plus rapide, et toujours 25 fois plus face à Express en cluster sur les mêmes 2 cœurs.

Toutes ces mesures ont été faites sur une VM de 4 vCPU : le serveur épinglé sur 2 cœurs, [wrk](https://github.com/wg/wrk) sur les 2 autres, 128 connexions keep-alive et 10 s par scénario. La page [Performances](performance.md) détaille la méthode, explique comment reproduire les chiffres (`bench/run.sh`) et pourquoi Vitesse est rapide. Les résultats bruts figurent aussi dans le [README](https://github.com/maxlestage/Vitesse#benchmark).

## Limites actuelles

Comme Express, Vitesse fait volontairement peu de choses. Ne sont pas (encore) inclus :

- HTTP/2 et TLS. Placez Vitesse derrière un reverse proxy comme Nginx ou Caddy, comme on le fait souvent avec Express (voir [Mise en production](production.md)).
- WebSocket.
- La compression des réponses.
- Les moteurs de templates.
- Les paramètres partiels dans un segment, comme `/vols/:de-:vers`.

## Organisation de la documentation

- **Premiers pas** : [Installation](installation.md), puis [Votre première application](first-app.md), un tutoriel pas à pas.
- **L'essentiel** : [Routage](routing.md), [Lire la requête](requests.md), [Envoyer la réponse](responses.md), [Middlewares](middleware.md), [Routeurs](routers.md), [État partagé](state.md), [Gestion des erreurs](errors.md) et [Fichiers statiques](static-files.md).
- **Pour aller plus loin** : [Tests](testing.md), [Configuration du serveur](server.md) (adresses, threads, arrêt propre), [Performances](performance.md) et [Venir d'Express](from-express.md).
- **Déploiement** : [Déployer sur Heroku depuis un téléphone](heroku-mobile.md), [Docker](docker.md) et [Mise en production](production.md).
- **Référence** : l'[aide-mémoire](cheatsheet.md) et la [FAQ](faq.md).

> [!TIP]
> Si vous connaissez bien Express, faites [Votre première application](first-app.md), puis gardez [Venir d'Express](from-express.md) ouvert pendant que vous écrivez vos propres routes.

La référence complète de l'API est générée à partir du code source. Lancez `cargo doc --open` dans un projet qui dépend de Vitesse, ou parcourez le code sur [GitHub](https://github.com/maxlestage/Vitesse).
