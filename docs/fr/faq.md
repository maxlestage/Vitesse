# FAQ et limites

Les réponses aux questions les plus fréquentes sur Vitesse, suivies d'une liste honnête de ce qu'il ne fait pas (encore). Si votre question n'y figure pas, ouvrez une issue sur GitHub.

## Généralités

### Vitesse est-il prêt pour la production ?

Vitesse est jeune (version 0.1), mais son moteur a été pensé pour la production dès le départ : la suite de tests couvre le moteur HTTP en détail (pipelining, corps `chunked`, `Expect: 100-continue`, limites de taille, arrêt propre) et tourne sous Linux, macOS et Windows à chaque modification. Les paniques deviennent des réponses `500` au lieu de faire tomber le serveur, les requêtes malformées sont rejetées, et `SIGTERM` laisse finir les requêtes en cours.

D'ici la version 1.0, l'API peut encore évoluer d'une version mineure à l'autre. Dépendez de `vitesse = { git = "https://github.com/maxlestage/Vitesse" }` (Cargo n'installe alors que les mises à jour compatibles `0.1.x`), lisez le journal des modifications avant de mettre à jour, servez le HTTPS ([directement](https.md) ou derrière un reverse proxy, voir [Production](production.md)), et faites des tests de charge avec votre propre trafic.

### Le projet est-il vraiment 100 % Rust ?

Oui : le framework, les exemples, l'outil de benchmark (`bench/runner`) et le site (servi par Vitesse lui-même, avec des pages et des îlots WebAssembly écrits avec [Active](https://maxlestage.github.io/Active/)) sont en Rust, et le dépôt ne contient ni JavaScript ni TypeScript. Deux nuances, pour être précis :

- un navigateur ne peut pas démarrer du WebAssembly sans quelques lignes de JavaScript, et il n'exécute un service worker (ce qui fait marcher le site hors ligne) que sous forme de script : la compilation du site (wasm-bindgen) génère donc un petit fichier de chargement, et le serveur du site génère son service worker ; aucun des deux n'est écrit à la main ni stocké dans le dépôt ;
- la documentation montre des extraits Express (JavaScript), mais uniquement en guise de comparaison avant/après pour les personnes qui viennent d'Express.

Le benchmark mesure aussi des serveurs écrits dans d'autres langages, puisque c'est tout l'intérêt de la comparaison : le serveur Drogon est en C++ (`bench/drogon`), et wrk, qui génère la charge, exécute des scripts de scénario écrits en Lua (`bench/lua`).

### Pourquoi pas actix-web ou axum ?

Ce sont deux excellents frameworks, matures, et Vitesse couvre désormais l'essentiel de ce qu'ils proposent : HTTPS et HTTP/2 (plus HTTP/3), WebSocket, Server-Sent Events, envois de fichiers multipart, taille maximale du corps par route, authentification, limites de débit et de concurrence, identifiants de requête, compression, proxys de confiance, délais configurables, sockets Unix, et l'écosystème de middlewares [tower](https://crates.io/crates/tower) grâce à la feature `tower`. [Venir d'axum ou d'actix-web](from-axum-actix.md) donne les équivalents, fonctionnalité par fonctionnalité.

Vitesse est un bon choix si :

- vous venez d'Express et voulez garder la même façon de penser : une seule `Request`, des middlewares avec `next`, des routeurs, et des handlers qui renvoient simplement leur réponse ;
- vous voulez les meilleures performances : dans le [benchmark](performance.md), Vitesse traite plus de requêtes par seconde que les deux, avec moins de CPU par requête.

Ce qui reste différent :

- **pas d'extracteurs, par choix** : un handler demande à sa `Request` ce dont il a besoin (`req.param_as`, `req.json`…) au lieu de déclarer des arguments typés ;
- **pas encore d'API 1.x stable** : Vitesse en est à la version 0.1 et peut encore évoluer avant la 1.0, alors qu'axum et actix-web ont derrière eux des années d'utilisation en production ;
- **un écosystème plus petit** : moins de crates, d'exemples et de réponses en ligne sont écrits pour Vitesse, même si les couches et les services tower comblent une bonne partie de l'écart.

### En quoi est-il différent d'Express ?

L'API est volontairement proche, mais les handlers renvoient leur réponse au lieu de modifier `res`, les données sont typées, les erreurs remontent avec `?`, et le programme est compilé, ce qui le rend environ 50 fois plus rapide dans le benchmark. Le guide [Venir d'Express](from-express.md) détaille tout cela.

### Quel runtime asynchrone utilise-t-il ?

[tokio](https://tokio.rs), et seulement tokio. `app.run` crée son propre runtime, tandis que `app.listen` et `app.bind` s'exécutent dans le vôtre. Tout crate construit sur tokio fonctionne dans vos handlers : pilotes de base de données, clients HTTP, Redis… Vitesse réexporte le crate sous le nom `vitesse::tokio` (avec les fonctionnalités qu'il utilise lui-même) ; ajoutez tokio à votre `Cargo.toml` pour utiliser `#[tokio::main]` ou `#[tokio::test]`. Les autres runtimes (async-std, smol) ne sont pas pris en charge.

### Quelle version de Rust faut-il ?

Rust 1.85 ou plus récent (l'édition 2024), comme l'indique `rust-version` dans le `Cargo.toml` de Vitesse. Mettez à jour avec `rustup update`.

### Fonctionne-t-il sous Windows et macOS ?

Oui : la suite de tests tourne sous Linux, macOS et Windows. Le mode un thread par cœur (une boucle d'événements et un socket `SO_REUSEPORT` par cœur) est une optimisation propre à Linux ; sur les autres systèmes, `app.run` utilise le runtime multi-thread de tokio, un peu moins rapide mais avec exactement la même API. `Ctrl+C` arrête proprement le serveur partout, `SIGTERM` sous Unix. Les sockets Unix (`app.run_unix`) ne sont pas disponibles sous Windows. L'outil de benchmark (`bench/runner`) ne fonctionne que sous Linux.

## Fonctionnalités

### Vitesse gère-t-il HTTPS et HTTP/2 ?

Oui, avec deux features Cargo à activer. `tls` sert HTTPS (TLS 1.2 et 1.3, avec rustls : pas d'OpenSSL à installer) : `app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?)`. `http2` ajoute HTTP/2, que les navigateurs négocient sur TLS, et que les clients qui commencent en HTTP/2 (h2c) obtiennent aussi sans TLS. Les mêmes routes et les mêmes middlewares répondent aux deux protocoles. Voir [HTTPS et HTTP/2](https.md).

Un reverse proxy (Nginx, Caddy) devant reste une bonne option, pour des certificats automatiques ou plusieurs applications sur une même machine, et des plateformes comme Heroku gèrent HTTPS pour vous : voir [Production](production.md).

### Vitesse gère-t-il WebSocket et HTTP/3 ?

Oui, les deux :

- **WebSocket** est intégré (la feature `ws`, activée par défaut) : `app.ws("/chat", |req, socket| async move { … })`, dans le style d'`express-ws`. Voir [WebSocket](websocket.md).
- **HTTP/3** sur QUIC est disponible avec la feature `http3`, à activer : `app.http3(Http3::from_pem_files(...)?)` sert la même application en UDP, à côté de HTTP/1.1. Voir [HTTP/3 et QUIC](http3.md).

WebSocket fonctionne en HTTP/1.1 (`ws://`, ou `wss://` avec la feature `tls`), pas en HTTP/2 ni en HTTP/3. Pour envoyer des données du serveur vers le client dans un seul sens, les Server-Sent Events sont souvent plus simples : voir la question suivante.

### Vitesse gère-t-il les Server-Sent Events ?

Oui, avec `vitesse::sse` (toujours disponible, et dans le prélude sous le nom `sse`) : renvoyez une réponse `Sse`, que le navigateur lit avec `EventSource`. Le plus simple est un canal, alimenté par une tâche :

```rust
use std::time::Duration;

use vitesse::prelude::*;
use vitesse::sse::Event;

let mut app = App::new();
app.get("/clock", |_| async {
    let (tx, stream) = sse::channel(16);
    tokio::spawn(async move {
        for i in 0.. {
            // `send` échoue une fois le navigateur parti.
            if tx.send(Event::new().event("tick").data(i.to_string())).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    stream
});
```

Vitesse envoie un commentaire de keep-alive toutes les 15 secondes, pour que les proxys ne ferment pas une connexion silencieuse. Cela fonctionne en HTTP/1.1, en HTTP/2 et en HTTP/3. Voir [Server-Sent Events](responses.md#server-sent-events).

### La compression ?

Oui, avec la feature `compress`. `middleware::compress()` compresse les réponses en Brotli, ou en gzip pour les navigateurs qui n'acceptent pas Brotli, quand cela en vaut la peine (corps textuels d'au moins 1 Kio). `middleware::decompress()` fait l'inverse pour les corps de requête envoyés avec `Content-Encoding: gzip`, `br` ou `deflate`, dans la limite de taille du corps (qui arrête aussi les « bombes zip ») :

```rust
app.middleware(middleware::compress());
app.middleware(middleware::decompress());
```

Derrière un reverse proxy, vous pouvez aussi laisser le proxy compresser les réponses à la place. Voir [Middlewares](middleware.md#compress).

### Les moteurs de templates ?

Non intégrés, comme Express sans moteur de vues. Utilisez le crate de templates de votre choix ([askama](https://crates.io/crates/askama), [minijinja](https://crates.io/crates/minijinja), [tera](https://crates.io/crates/tera)…) et renvoyez le résultat avec `Html(rendu)`. Chargez ou compilez vos templates une seule fois au démarrage et partagez-les via l'[état](state.md).

### Comment utiliser une base de données ?

Avec n'importe quel pilote asynchrone (par exemple [sqlx](https://crates.io/crates/sqlx)). Créez le pool de connexions une seule fois au démarrage, enregistrez-le avec `app.state(pool)` et lisez-le dans les handlers avec `req.state::<Pool>()`. La création d'un pool étant généralement asynchrone, démarrez le serveur avec `#[tokio::main]` et `app.listen` (qui s'arrête proprement, tout comme `app.run`), comme le montre [Configuration du serveur](server.md).

### Les envois de fichiers (multipart) ?

Oui, avec la feature `multipart`. `req.multipart()?` lit un corps `multipart/form-data` champ par champ, au fil de son arrivée, et `field.save(chemin)` écrit un fichier sur le disque en flux, sans le garder en mémoire. Augmentez la limite de taille du corps sur la seule route d'envoi, avec `middleware::body_limit` :

```rust
async fn upload(req: Request) -> vitesse::Result<String> {
    let mut form = req.multipart()?; // 415 si le corps n'est pas multipart
    let (mut files, mut bytes) = (0, 0);
    while let Some(field) = form.next_field().await? {
        if field.file_name().is_some() {
            files += 1;
            // N'utilisez jamais tel quel le nom de fichier du client comme chemin.
            bytes += field.save(format!("uploads/{files}.bin")).await?;
        }
    }
    Ok(format!("{files} fichiers, {bytes} octets"))
}

app.post("/upload", upload.with(middleware::body_limit(100 * 1024 * 1024)));
```

Voir [Envois de fichiers](requests.md#envois-de-fichiers-multipart).

### Les sessions et l'authentification ?

Pour l'authentification, deux middlewares sont fournis : `middleware::basic_auth(("admin", "secret"))` (le navigateur demande un nom d'utilisateur et un mot de passe) et `middleware::bearer_auth(jeton)` (`Authorization: Bearer …`, pour les API). Tous deux acceptent aussi une closure, pour vérifier les identifiants auprès de vos propres utilisateurs, et les comparent en temps constant. Les identifiants partent avec chaque requête : ne les utilisez qu'en HTTPS. Voir [`basic_auth` et `bearer_auth`](middleware.md#basic_auth-et-bearer_auth).

Pour les sessions, Vitesse n'a pas de stockage à lui : utilisez [tower-sessions](https://crates.io/crates/tower-sessions) grâce à la feature `tower`, comme avec axum :

```rust
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};

app.middleware(middleware::layer(SessionManagerLayer::new(MemoryStore::default())));
app.get("/visits", |req: Request| async move {
    let session = req.get::<Session>().unwrap().clone();
    let visits: u32 = session.get("visits").await?.unwrap_or(0) + 1;
    session.insert("visits", visits).await?;
    Ok::<_, Error>(format!("visit #{visits}"))
});
```

Son cookie est `Secure` par défaut : il lui faut donc HTTPS (ou `.with_secure(false)` pendant le développement), et `MemoryStore` oublie tout au redémarrage : utilisez un stockage persistant en production. Voir [Sessions avec tower-sessions](from-axum-actix.md#sessions-avec-tower-sessions).

Vous pouvez aussi construire les vôtres : lisez les cookies avec `req.cookie(nom)`, posez-les avec `Cookie` (`http_only`, `secure`, `same_site`…), vérifiez un jeton dans un middleware et attachez l'utilisateur à la requête avec `req.set(user)`. Voir [Middlewares](middleware.md).

### Pourquoi CORS refuse-t-il mes cookies ?

`middleware::cors()` autorise toutes les origines avec `Access-Control-Allow-Origin: *`. Pour une requête avec identifiants (cookies, `Authorization`), les navigateurs rejettent une réponse qui indique `*`, et c'est toujours ce que vous obtenez avec `allow_credentials(true)` si vous ne listez aucune origine. C'est voulu, comme en Express : renvoyer n'importe quelle origine permettrait à n'importe quel site de lire les réponses d'un utilisateur connecté. Listez explicitement vos origines de confiance :

```rust
app.middleware(
    middleware::cors()
        .allow_origin("https://app.example.com")
        .allow_credentials(true),
);
```

### Comment journaliser les requêtes et les erreurs ?

`middleware::logger()` affiche une ligne par requête, comme `morgan('dev')` : `GET /users/42 200 0.084 ms`. La cause des erreurs serveur (une erreur `5xx` avec une source, par exemple une erreur de base de données convertie avec `?`) est affichée sur la sortie d'erreur, préfixée par `[vitesse]`.

Pour des journaux structurés, écrivez votre propre middleware avec le crate de journalisation de votre choix. Deux outils rendent les journaux plus faciles à exploiter : `middleware::request_id()` donne un identifiant à chaque requête (`req.get::<middleware::RequestId>()`), renvoyé dans l'en-tête de réponse `X-Request-Id`, et `req.route()` donne le motif de la route correspondante (`/users/:id`), pour regrouper les journaux par route plutôt que par chemin. Avec la feature `tower`, le `TraceLayer` de tower-http fonctionne aussi, via `middleware::layer`. Voir [Journaux](production.md#journaux) pour un exemple complet.

### Pourquoi `req.ip()` renvoie-t-il l'adresse du proxy ?

Parce que c'est le proxy qui est connecté à Vitesse. L'adresse du client se trouve dans l'en-tête `X-Forwarded-For`, qu'un client pourrait aussi envoyer : indiquez à Vitesse les proxys auxquels faire confiance, comme le réglage `trust proxy` d'Express, et `req.ip()` le lit pour vous :

```rust
app.trust_proxy("loopback"); // un proxy sur la même machine ; ou "10.0.0.0/8", un nombre de sauts, true
```

`req.protocol()` et `req.hostname()` suivent alors aussi `X-Forwarded-Proto` et `X-Forwarded-Host`. Voir [Connaître l'adresse IP du client](production.md#connaître-ladresse-ip-du-client).

### Peut-on utiliser les middlewares de tower ou de tower-http ?

Oui, avec la feature `tower`. `middleware::layer(...)` exécute n'importe quelle `Layer` tower comme un middleware Vitesse, pour toute l'application, sur un routeur ou sur une seule route : `CorsLayer`, `TraceLayer` ou `SetResponseHeaderLayer` de tower-http, `ConcurrencyLimitLayer` de tower, tower-governor, tower-sessions…

```rust
use tower_http::cors::CorsLayer;

app.middleware(middleware::layer(CorsLayer::permissive()));
```

Dans l'autre sens, `app.into_service()` transforme votre application en `Service` tower : servez-la avec hyper, imbriquez-la dans un routeur axum (`Router::nest_service`), ou testez-la avec `tower::ServiceExt::oneshot`. Voir [Couches tower](middleware.md#couches-tower).

### Peut-on ajouter des routes pendant que le serveur tourne ?

Non. L'application est figée au démarrage du serveur (c'est l'une des raisons de sa rapidité : ni verrous, ni compteurs de références). Fixez vos routes au démarrage, et utilisez l'[état](state.md) pour les données qui changent.

## Déploiement et communauté

### Comment déployer une application Vitesse ?

Compilez un binaire en mode release (`cargo build --release`), écoutez sur le port donné par la variable d'environnement `PORT`, et lancez-le. Guides pas à pas : [Heroku depuis votre téléphone](heroku-mobile.md), [Docker](docker.md), et [Production](production.md) pour les reverse proxies et le HTTPS.

### Comment signaler un bug ou contribuer ?

Ouvrez une issue sur GitHub, avec votre version de Vitesse, votre système et, idéalement, un exemple minimal qui reproduit le problème. Les pull requests sont aussi les bienvenues ; pour un changement important, ouvrez d'abord une issue pour en discuter. Avant de proposer vos modifications, lancez les mêmes vérifications que la CI :

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

La documentation se trouve dans `docs/`, en anglais, en français et en espagnol : quand vous modifiez une page, mettez aussi à jour les autres langues, ou signalez-le dans votre pull request.

### Quelle est la licence ?

Vitesse est un logiciel propriétaire : © Maxime Nathan Lestage, tous droits réservés. Son code source est publié sur crates.io et GitHub pour que vous puissiez le lire, l'auditer et le compiler, et vous pouvez l'utiliser librement comme dépendance de vos propres programmes, y compris commerciaux. Le modifier ou redistribuer son code source demande l'accord écrit de l'auteur. Les conditions complètes sont dans le fichier [LICENSE](https://maxlestage.github.io/Vitesse/LICENSE.txt) du dépôt.

## Limites actuelles

Comme Express, Vitesse fait volontairement peu de choses. Ne sont pas (encore) inclus :

- **Les extracteurs**, par choix : un handler prend une seule `Request` et lui demande ce dont il a besoin ;
- **WebSocket sur HTTP/2 et HTTP/3** : [WebSocket](websocket.md) fonctionne en HTTP/1.1, avec ou sans TLS ;
- **Le server push HTTP/2** et le mécanisme **`Upgrade: h2c`** : HTTP/2 fonctionne sur TLS, et en clair avec « prior knowledge » (voir [HTTPS et HTTP/2](https.md)) ;
- **Le 0-RTT et la lecture en flux des corps de requête en HTTP/3** : en HTTP/3, les corps de requête sont lus en entier avant l'exécution du handler ;
- **Les moteurs de templates** : utilisez un crate de templates et renvoyez `Html(...)` ;
- **Un stockage de sessions et des cookies signés intégrés** : utilisez [tower-sessions](from-axum-actix.md#sessions-avec-tower-sessions) grâce à la feature `tower` ;
- **Les paramètres partiels dans un segment** (`/vols/:de-:vers`), ainsi que les paramètres facultatifs (`/:id?`) et les expressions régulières dans les routes ;
- **La génération d'URL à partir de routes nommées** (le `url_for` d'actix-web) ;
- **D'autres runtimes que tokio**.

Quelques autres points à connaître :

- les limites de taille du moteur sont fixes (60 Kio pour la tête de requête, 64 en-têtes), mais ses délais sont des réglages : `keep_alive` (60 s), `header_timeout` (30 s) et `shutdown_timeout` (10 s), tout comme `max_connections` (voir [Configuration du serveur](server.md#délais-et-connexions)) ;
- `body_limit` s'applique à toute l'application, et `middleware::body_limit` le modifie par route ou par routeur ;
- les routes sont sensibles à la casse (`/Users` ≠ `/users`) ;
- on ne peut pas ajouter de routes une fois le serveur démarré ;
- `X-Forwarded-For`, `X-Forwarded-Proto` et `X-Forwarded-Host` ne sont lus qu'une fois vos proxys déclarés avec `app.trust_proxy(...)` ;
- `middleware::rate_limit` compte dans la mémoire de chaque processus : avec plusieurs instances, chacune compte de son côté.
