# FAQ et limites

Les réponses aux questions les plus fréquentes sur Vitesse, suivies d'une liste honnête de ce qu'il ne fait pas (encore). Si votre question n'y figure pas, ouvrez une issue sur [GitHub](https://github.com/maxlestage/Vitesse/issues).

## Généralités

### Vitesse est-il prêt pour la production ?

Vitesse est jeune (version 0.1), mais son moteur a été pensé pour la production dès le départ : la suite de tests couvre le moteur HTTP en détail (pipelining, corps `chunked`, `Expect: 100-continue`, limites de taille, arrêt propre) et tourne sous Linux, macOS et Windows à chaque modification. Les paniques deviennent des réponses `500` au lieu de faire tomber le serveur, les requêtes malformées sont rejetées, et `SIGTERM` laisse finir les requêtes en cours.

D'ici la version 1.0, l'API peut encore évoluer d'une version mineure à l'autre. Dépendez de `vitesse = "0.1"` (Cargo n'installe alors que les mises à jour compatibles `0.1.x`), lisez le [journal des modifications](https://github.com/maxlestage/Vitesse/blob/master/CHANGELOG.md) avant de mettre à jour, placez un reverse proxy devant pour le HTTPS (voir [Production](production.md)), et faites des tests de charge avec votre propre trafic.

### Pourquoi pas actix-web ou axum ?

Ce sont deux excellents frameworks, matures. Vitesse est un bon choix si :

- vous venez d'Express et voulez garder la même façon de penser : une seule `Request`, des middlewares avec `next`, des routeurs, et des handlers qui renvoient simplement leur réponse, sans extracteurs ni couches de services à apprendre ;
- vous voulez les meilleures performances : dans le [benchmark](performance.md), Vitesse traite plus de requêtes par seconde que les deux, avec moins de CPU par requête.

Préférez actix-web ou axum si vous avez besoin de ce que Vitesse ne fait pas (encore) : HTTP/2, TLS ou WebSocket intégrés, l'écosystème de middlewares [tower](https://github.com/tower-rs/tower), ou une API 1.x stable.

### En quoi est-il différent d'Express ?

L'API est volontairement proche, mais les handlers renvoient leur réponse au lieu de modifier `res`, les données sont typées, les erreurs remontent avec `?`, et le programme est compilé, ce qui le rend environ 50 fois plus rapide dans le benchmark. Le guide [Venir d'Express](from-express.md) détaille tout cela.

### Quel runtime asynchrone utilise-t-il ?

[tokio](https://tokio.rs), et seulement tokio. `app.run` crée son propre runtime, tandis que `app.listen` et `app.bind` s'exécutent dans le vôtre. Tout crate construit sur tokio fonctionne dans vos handlers : pilotes de base de données, clients HTTP, Redis… Vitesse réexporte le crate sous le nom `vitesse::tokio` (avec les fonctionnalités qu'il utilise lui-même) ; ajoutez tokio à votre `Cargo.toml` pour utiliser `#[tokio::main]` ou `#[tokio::test]`. Les autres runtimes (async-std, smol) ne sont pas pris en charge.

### Quelle version de Rust faut-il ?

Rust 1.85 ou plus récent (l'édition 2024), comme l'indique `rust-version` dans le `Cargo.toml` de Vitesse. Mettez à jour avec `rustup update`.

### Fonctionne-t-il sous Windows et macOS ?

Oui : la suite de tests tourne sous Linux, macOS et Windows. Le mode un thread par cœur (une boucle d'événements et un socket `SO_REUSEPORT` par cœur) est une optimisation propre à Linux ; sur les autres systèmes, `app.run` utilise le runtime multi-thread de tokio, un peu moins rapide mais avec exactement la même API. `Ctrl+C` arrête proprement le serveur partout, `SIGTERM` sous Unix. Le script de benchmark (`bench/run.sh`) ne fonctionne que sous Linux.

## Fonctionnalités

### Vitesse gère-t-il HTTPS et HTTP/2 ?

Pas directement : Vitesse parle HTTP/1.1 en TCP simple. Placez-le derrière un reverse proxy (Nginx, Caddy, ou le load balancer de votre hébergeur) qui gère TLS et HTTP/2 et transmet les requêtes en HTTP/1.1, comme on le fait couramment avec Express. Des plateformes comme Heroku le font déjà pour vous. Voir [Production](production.md).

### Et WebSocket ?

Non pris en charge. Pour envoyer des données du serveur vers le client, une réponse en flux suffit souvent : `Body::from_stream` envoie chaque morceau dès qu'il est produit, ce qui est tout ce qu'il faut pour des [Server-Sent Events](https://developer.mozilla.org/fr/docs/Web/API/Server-sent_events) avec le type de contenu `text/event-stream` (voir [Réponses](responses.md)). Pour un vrai WebSocket bidirectionnel, faites tourner un service dédié à côté de Vitesse.

### La compression ?

Non intégrée. Laissez le reverse proxy compresser les réponses (gzip, brotli) : c'est de toute façon souvent l'option la plus efficace.

### Les moteurs de templates ?

Non intégrés, comme Express sans moteur de vues. Utilisez le crate de templates de votre choix ([askama](https://crates.io/crates/askama), [minijinja](https://crates.io/crates/minijinja), [tera](https://crates.io/crates/tera)…) et renvoyez le résultat avec `Html(rendu)`. Chargez ou compilez vos templates une seule fois au démarrage et partagez-les via l'[état](state.md).

### Comment utiliser une base de données ?

Avec n'importe quel pilote asynchrone (par exemple [sqlx](https://crates.io/crates/sqlx)). Créez le pool de connexions une seule fois au démarrage, enregistrez-le avec `app.state(pool)` et lisez-le dans les handlers avec `req.state::<Pool>()`. La création d'un pool étant généralement asynchrone, démarrez le serveur avec `#[tokio::main]` et `app.listen` (ou `app.bind` pour un arrêt propre), comme le montre [Configuration du serveur](server.md).

### Les envois de fichiers (multipart) ?

Il n'y a pas d'analyseur `multipart/form-data` intégré. Lisez le corps brut avec `req.bytes()` (dans la limite de `body_limit`, à relever pour les gros fichiers) ou en flux avec `req.take_body()`, et analysez-le avec un crate dédié comme [multer](https://crates.io/crates/multer).

### Les sessions et l'authentification ?

Pas de sessions intégrées, mais toutes les briques sont là : lisez les cookies avec `req.cookie(nom)`, posez-les avec `Cookie` (`http_only`, `secure`, `same_site`…), vérifiez un jeton dans un middleware et attachez l'utilisateur à la requête avec `req.set(user)`. Voir [Middlewares](middleware.md).

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

`middleware::logger()` affiche une ligne par requête, comme `morgan('dev')` : `GET /users/42 200 0.084 ms`. La cause des erreurs serveur (une erreur `5xx` avec une source, par exemple une erreur de base de données convertie avec `?`) est affichée sur la sortie d'erreur, préfixée par `[vitesse]`. Pour des journaux structurés, écrivez votre propre middleware avec le crate de journalisation de votre choix.

### Pourquoi `req.ip()` renvoie-t-il l'adresse du proxy ?

Parce que c'est le proxy qui est connecté à Vitesse. L'adresse du client se trouve dans l'en-tête `X-Forwarded-For` (`req.header("x-forwarded-for")`), auquel il ne faut se fier que s'il est posé par votre propre proxy. Voir [Production](production.md).

### Peut-on ajouter des routes pendant que le serveur tourne ?

Non. L'application est figée au démarrage du serveur (c'est l'une des raisons de sa rapidité : ni verrous, ni compteurs de références). Fixez vos routes au démarrage, et utilisez l'[état](state.md) pour les données qui changent.

## Déploiement et communauté

### Comment déployer une application Vitesse ?

Compilez un binaire en mode release (`cargo build --release`), écoutez sur le port donné par la variable d'environnement `PORT`, et lancez-le. Guides pas à pas : [Heroku depuis votre téléphone](heroku-mobile.md), [Docker](docker.md), et [Production](production.md) pour les reverse proxies et le HTTPS.

### Comment signaler un bug ou contribuer ?

Ouvrez une issue sur [GitHub](https://github.com/maxlestage/Vitesse/issues), avec votre version de Vitesse, votre système et, idéalement, un exemple minimal qui reproduit le problème. Les pull requests sont aussi les bienvenues ; pour un changement important, ouvrez d'abord une issue pour en discuter. Avant de proposer vos modifications, lancez les mêmes vérifications que la CI :

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

La documentation se trouve dans [`docs/`](https://github.com/maxlestage/Vitesse/tree/master/docs), en anglais, en français et en espagnol : quand vous modifiez une page, mettez aussi à jour les autres langues, ou signalez-le dans votre pull request.

### Quelle est la licence ?

Vitesse est distribué sous double licence [MIT](https://github.com/maxlestage/Vitesse/blob/master/LICENSE-MIT) ou [Apache 2.0](https://github.com/maxlestage/Vitesse/blob/master/LICENSE-APACHE), au choix (`MIT OR Apache-2.0`), comme la plupart de l'écosystème Rust.

## Limites actuelles

Comme Express, Vitesse fait volontairement peu de choses. Ne sont pas (encore) inclus :

- **HTTP/2 et TLS** : placez Vitesse derrière un reverse proxy comme Nginx ou Caddy (voir [Production](production.md)) ;
- **WebSocket** ;
- **La compression** : confiez-la au reverse proxy ;
- **Les moteurs de templates** : utilisez un crate de templates et renvoyez `Html(...)` ;
- **Les paramètres partiels dans un segment** (`/vols/:de-:vers`), ainsi que les paramètres facultatifs (`/:id?`) et les expressions régulières dans les routes ;
- **Les formulaires multipart** : utilisez un crate dédié sur le corps brut.

Quelques autres points à connaître :

- les limites du moteur sont fixes : 60 Kio pour la tête de requête, 64 en-têtes, environ 60 s avant de fermer une connexion inactive, 10 s de délai de grâce à l'arrêt (voir [Configuration du serveur](server.md)) ;
- `body_limit` s'applique à toute l'application, pas route par route ;
- les routes sont sensibles à la casse (`/Users` ≠ `/users`) ;
- on ne peut pas ajouter de routes une fois le serveur démarré ;
- `X-Forwarded-For` n'est pas interprété automatiquement par `req.ip()` ;
- tokio est le seul runtime pris en charge.
