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

Ce sont deux excellents frameworks, matures, et Vitesse couvre désormais l'essentiel de ce qu'ils proposent : HTTPS et HTTP/2 (plus HTTP/3), WebSocket (en HTTP/2 aussi), Server-Sent Events, contraintes dans les routes et routes nommées avec génération d'URL, envois de fichiers multipart, taille maximale du corps par route, cookies signés et sessions, gabarits HTML, authentification, limites de débit et de concurrence, identifiants de requête, compression, proxys de confiance, délais configurables, sockets Unix, et l'écosystème de middlewares [tower](https://crates.io/crates/tower) grâce à la feature `tower`. [Venir d'axum ou d'actix-web](from-axum-actix.md) donne les équivalents, fonctionnalité par fonctionnalité.

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

WebSocket fonctionne en HTTP/1.1 (`ws://`, ou `wss://` avec la feature `tls`) et, avec la feature `http2`, [en HTTP/2](websocket.md#websocket-sur-http2), que les navigateurs utilisent d'eux-mêmes sur un site en HTTPS. Il ne fonctionne pas en HTTP/3 : les navigateurs ouvrent alors leurs WebSockets en HTTP/2 ou en HTTP/1.1. Pour envoyer des données du serveur vers le client dans un seul sens, les Server-Sent Events sont souvent plus simples : voir la question suivante.

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

Oui, avec la feature `templates`, qui apporte [minijinja](https://crates.io/crates/minijinja) (la syntaxe Jinja2 : `{{ name }}`, `{% for %}`, `{% extends %}`…). Elle joue le rôle de `app.set('views', …)` et de `res.render(…)` dans Express :

```rust
app.views("templates"); // templates/users.html, templates/layout.html…
app.get("/users", |req: Request| async move {
    req.render("users.html", json!({ "users": ["Ada", "Grace"] }))
});
```

Les valeurs sont échappées pour le HTML dans les gabarits `.html`, et un gabarit manquant ou une erreur de rendu devient une `500` dont la cause est journalisée. Voir [Gabarits HTML](templates.md).

Les autres crates de gabarits ([askama](https://crates.io/crates/askama), [tera](https://crates.io/crates/tera)…) fonctionnent aussi : faites le rendu avec eux, et renvoyez le résultat avec `Html(rendu)`.

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

Les sessions et les cookies signés sont intégrés, avec la feature `session`, comme `express-session` et `cookie-parser` dans Express :

```rust
use vitesse::session::MemoryStore;

app.cookie_secret(std::env::var("COOKIE_SECRET").expect("COOKIE_SECRET n'est pas défini"));
app.middleware(middleware::session(MemoryStore::new()));
app.get("/visits", |req: Request| async move {
    let session = req.session();
    let visits = session.get::<u32>("visits").unwrap_or(0) + 1;
    session.insert("visits", visits)?;
    Ok::<_, Error>(format!("visite n°{visits}"))
});
```

Le navigateur ne reçoit qu'un identifiant aléatoire, dans un cookie `HttpOnly` (signé, ici, par le secret de l'application) ; les données restent sur le serveur. `MemoryStore` oublie tout au redémarrage et n'est pas partagé entre les instances : en production, branchez votre propre stockage (Redis, PostgreSQL…) en implémentant le trait `SessionStore`. Sans sessions, `req.sign_cookie(...)` et `req.signed_cookie(nom)` donnent des cookies que le client peut lire mais pas modifier. Voir [Cookies signés et sessions](sessions.md).

[tower-sessions](https://crates.io/crates/tower-sessions), le choix habituel avec axum, fonctionne aussi, grâce à la feature `tower` et à ses stockages tout prêts : voir [Sessions avec tower-sessions](from-axum-actix.md#sessions-avec-tower-sessions).

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

### Les routes acceptent-elles des expressions régulières ou des paramètres optionnels ?

Oui, avec la syntaxe d'Express 4 : `r"/users/:id(\d+)"` ne correspond qu'à des chiffres, `/archive/:year?/:month?` correspond aussi à `/archive`, et `/flights/:from-:to` tire deux paramètres d'un seul segment. Une route avec une contrainte l'emporte sur un simple paramètre à la même position : `/users/:id(\d+)` et `/users/:name` peuvent donc coexister. `app.case_sensitive(false)` fait ignorer la casse aux routes, comme Express par défaut. Voir [Paramètres de chemin](routing.md#paramètres-de-chemin).

### Peut-on générer des URL à partir du nom des routes ?

Oui, comme avec le `url_for` d'actix-web : nommez la route avec `.name(...)`, puis construisez son chemin avec `req.path_for` ou son URL absolue avec `req.url_for`. Les valeurs sont encodées pour l'URL, et les contraintes sont vérifiées :

```rust
app.get("/users/:id", show_user).name("user");
app.post("/users", |req: Request| async move {
    let link = req.path_for("user", &[&42])?; // "/users/42"
    Ok::<_, Error>(Redirect::see_other(link))
});
```

Voir [Routes nommées et génération d'URL](routing.md#routes-nommées-et-génération-durl).

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

Ne sont pas inclus, chacun pour une raison :

- **Les extracteurs**, par choix : un handler prend une seule `Request` et lui demande ce dont il a besoin ;
- **WebSocket sur HTTP/3** (RFC 9220) : le crate h3 ne l'implémente pas, et les navigateurs ne s'en servent pas. Ils ouvrent leurs WebSockets en HTTP/2 ou en HTTP/1.1, que [Vitesse sert](websocket.md#websocket-sur-http2) ;
- **Le server push HTTP/2** : Chrome l'a retiré en 2022 et Firefox en 2024, et axum et actix-web ne le proposent pas non plus ;
- le mécanisme **`Upgrade: h2c`** : la RFC 9113 l'a retiré de HTTP/2. HTTP/2 fonctionne sur TLS, et en clair avec « prior knowledge » (voir [HTTPS et HTTP/2](https.md#http2-en-clair-h2c)) ;
- **Le 0-RTT (*early data*) en HTTP/3** : une requête envoyée aussi tôt peut être rejouée par un attaquant, Vitesse attend donc la fin de la poignée de main (voir [HTTP/3](http3.md#ce-qui-change-et-ce-qui-nest-pas-encore-pris-en-charge)) ;
- **D'autres runtimes que tokio** (async-std, smol) ;
- **Une API 1.x stable** : Vitesse en est à la version 0.1, et son API peut encore changer avant la 1.0.

Quelques autres points à connaître :

- les limites de taille du moteur sont fixes (60 Kio pour la tête de requête, 64 en-têtes), mais ses délais sont des réglages : `keep_alive` (60 s), `header_timeout` (30 s) et `shutdown_timeout` (10 s), tout comme `max_connections` (voir [Configuration du serveur](server.md#délais-et-connexions)) ;
- `body_limit` s'applique à toute l'application, et `middleware::body_limit` le modifie par route ou par routeur, dans tous les protocoles ;
- les routes sont sensibles à la casse par défaut (`/Users` ≠ `/users`) : appelez `app.case_sensitive(false)` pour ignorer la casse, comme le fait Express ;
- on ne peut pas ajouter de routes une fois le serveur démarré ;
- les gabarits chargés avec `app.views(...)` sont lus une fois, puis gardés en mémoire : redémarrez le serveur pour voir une modification ;
- les sessions d'un `MemoryStore` vivent dans la mémoire de chaque processus : elles sont perdues au redémarrage et ne sont pas partagées entre les instances ;
- `X-Forwarded-For`, `X-Forwarded-Proto` et `X-Forwarded-Host` ne sont lus qu'une fois vos proxys déclarés avec `app.trust_proxy(...)` ;
- `middleware::rate_limit` compte dans la mémoire de chaque processus : avec plusieurs instances, chacune compte de son côté.
