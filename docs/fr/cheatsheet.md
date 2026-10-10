# Aide-mémoire de l'API

Toute l'API publique de Vitesse sur une seule page, regroupée par thème : chaque ligne donne l'usage et ce qu'il fait. Pour les signatures exactes et tous les détails, voyez la référence sur [docs.rs/vitesse](https://docs.rs/vitesse).

## Squelette

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());

    app.get("/", |_| async { "Hello World!" });
    app.get("/users/:id", |req: Request| async move {
        let id: u64 = req.param_as("id")?;
        Ok::<_, Error>(Json(json!({ "id": id })))
    });

    app.run(3000)
}
```

`use vitesse::prelude::*;` importe `App`, `Router`, `Request`, `Response`, `Next`, `Error`, `Json`, `Html`, `Redirect`, `Cookie`, `Body`, `ServeDir`, `StatusCode`, `Method`, `IntoResponse`, `HandlerExt`, `json!`, le module `res`, le module `middleware` et le module `ws` (WebSocket).

## Features Cargo

| Feature | Par défaut | Apporte |
|---|---|---|
| `ws` | Activée | `app.ws`, `router.ws` et le module `vitesse::ws` |
| `http3` | Désactivée | `app.http3`, `server.http3_addr` et le module `vitesse::http3` |

```toml
[dependencies]
vitesse = { version = "0.1", features = ["http3"] }        # ajoute HTTP/3
# vitesse = { version = "0.1", default-features = false } # sans WebSocket
```

Voir [Installation](installation.md#les-features-cargo).

## Application

| Usage | Description |
|---|---|
| `App::new()` | Une application vide (`express()`) |
| `app.run(addr)` | Démarre le serveur avec son propre runtime ; bloquant ; s'arrête proprement sur `Ctrl+C` / `SIGTERM` |
| `app.listen(addr).await` | Démarre le serveur sur le runtime tokio courant ; s'arrête proprement sur `Ctrl+C` / `SIGTERM` |
| `app.bind(addr).await?` | Ouvre le port et renvoie un `Server`, sans encore servir |
| `app.middleware(mw)` | Middleware global (`app.use`) : toutes les requêtes, dans l'ordre, avant le routage |
| `app.fallback(handler)` | Handler appelé quand aucune route ne correspond (par défaut : `404 {"error":"Cannot GET /x"}`) |
| `app.on_error(f)` | Personnalise toutes les réponses d'erreur ; `f` est une `Fn(Error) -> impl IntoResponse` |
| `app.state(valeur)` | Enregistre un état global, un par type (`app.locals`) |
| `app.body_limit(octets)` | Taille maximale d'un corps lu en mémoire (par défaut `DEFAULT_BODY_LIMIT`, 1 Mio) |
| `app.workers(n)` | Nombre de threads pour `run` (par défaut : un par processeur) |
| `app.thread_per_core(bool)` | Mode un thread par cœur pour `run` (par défaut `true`, Linux uniquement) |
| `app.http3(config)` | Sert aussi l'application en HTTP/3 (QUIC, UDP), à côté de HTTP/1.1 (feature `http3`) |

Tous ces réglages renvoient `&mut App` : les appels s'enchaînent. Voir [Configuration du serveur](server.md).

## Routage

Ces méthodes existent sur `App` comme sur `Router`, et s'enchaînent.

| Usage | Description |
|---|---|
| `app.get(chemin, handler)` | Route `GET` (utilisée aussi pour `HEAD`) |
| `app.post(…)`, `.put(…)`, `.patch(…)`, `.delete(…)` | Routes pour ces méthodes |
| `app.head(chemin, handler)` | Route `HEAD` explicite |
| `app.options(chemin, handler)` | Route `OPTIONS` explicite (par défaut : `204` avec `Allow`) |
| `app.all(chemin, handler)` | Toutes les méthodes |
| `app.route(méthode, chemin, handler)` | Une méthode quelconque, par ex. `Method::from_bytes(b"PURGE").unwrap()` |
| `app.mount(préfixe, routeur)` | Monte un `Router` sous un préfixe (`app.use('/api', router)`) |
| `app.static_dir(préfixe, dossier)` | Sert un dossier sous `préfixe` |
| `app.serve_dir(préfixe, serve_dir)` | Idem, avec un `ServeDir` configuré |
| `app.ws(chemin, handler)` | Route WebSocket (`GET`) : `handler(req, socket)` prend la main sur la connexion (express-ws) |

| Motif | Correspond à |
|---|---|
| `/users` | Exactement `/users` (et `/users/`) |
| `/users/:id` | Un segment, lu avec `req.param("id")` ; noms en lettres, chiffres et `_` |
| `/files/*path` | Le reste du chemin, lu avec `req.param("path")` ; correspond aussi à `/files` |
| `/*` | Tous les chemins ; le paramètre s'appelle `*` |

Priorité : statique > paramètre > joker. Les paramètres sont décodés (`%C3%A9` → `é`), la correspondance est sensible à la casse, une mauvaise méthode répond `405` avec `Allow`, et une route invalide ou en double panique au démarrage. Voir [Routage](routing.md).

## Handlers

| Usage | Description |
|---|---|
| `async fn show(req: Request) -> impl IntoResponse` | Un handler nommé |
| `\|req: Request\| async move { … }` | Une closure qui utilise la requête |
| `\|_\| async { … }` | Une closure qui ignore la requête |
| `Ok::<_, Error>(valeur)` | Dernière expression d'une closure qui utilise `?` |
| `handler.with(mw)` | Ajoute un middleware à une seule route (`HandlerExt`) ; renvoie un `Chained` |
| `handler.with(a).with(b)` | Plusieurs middlewares de route, exécutés dans cet ordre |
| `impl Handler for MonType` | Handler personnalisé : `fn call(&'static self, req: Request) -> BoxFuture<Response>` |

## Requête

| Usage | Description |
|---|---|
| `req.method()` | La méthode HTTP (`&Method`) |
| `req.path()` | Le chemin, sans la query string |
| `req.uri()` | L'URI complète (`&Uri`) |
| `req.version()` | La version HTTP |
| `req.set_uri(uri)` | Remplace l'URI (réécriture d'URL dans un middleware global) |
| `req.header(nom)` | Un en-tête en `Option<&str>`, sans tenir compte de la casse (`req.get('host')`) |
| `req.header_all(nom)` | Toutes les valeurs d'un en-tête répété |
| `req.headers()`, `req.headers_mut()` | Tous les en-têtes (`HeaderMap`) |
| `req.content_type()` | Le `Content-Type` |
| `req.is("json")` | Vérifie le type du corps : type complet, sous-type ou `text/*` |
| `req.hostname()` | L'hôte demandé, sans le port |
| `req.cookie(nom)` | La valeur d'un cookie (`req.cookies.nom`) |
| `req.param(nom)` | Un paramètre de route, `Option<&str>` |
| `req.param_as::<T>(nom)?` | Un paramètre converti en `T` ; `400` en cas d'échec |
| `req.params()` | Tous les paramètres `(nom, valeur)` |
| `req.query(nom)` | Une valeur décodée de la query string, `Option<Cow<str>>` |
| `req.query_as::<T>()?` | La query string désérialisée dans une structure ; `400` en cas d'échec |
| `req.query_pairs()` | Toutes les paires de la query string |
| `req.query_string()` | La query string brute |
| `req.json::<T>().await?` | Corps JSON (`express.json()`) ; `400` s'il est invalide, `413` s'il est trop gros |
| `req.form::<T>().await?` | Formulaire encodé (`express.urlencoded()`) |
| `req.text().await?` | Corps en texte UTF-8 ; `400` s'il est invalide |
| `req.bytes().await?` | Corps brut (`Bytes`) |
| `req.take_body()` | Le corps en flux (`Body`), non limité par `body_limit` |
| `req.set_body(corps)` | Remplace le corps |
| `req.state::<T>()` | L'état global (`&'static T`) ; panique, donc `500`, s'il manque |
| `req.try_state::<T>()` | Idem, en `Option` |
| `req.set(valeur)` | Attache une donnée à la requête (`res.locals`) ; le type doit être `Clone` |
| `req.get::<T>()` | Lit une donnée attachée par un middleware |
| `req.extensions()`, `req.extensions_mut()` | Les `http::Extensions` brutes |
| `req.ip()`, `req.remote_addr()` | L'IP du client, et l'IP avec le port |

Le corps est lu à la demande et mis en cache : les méthodes de lecture peuvent être appelées plusieurs fois. Voir [Requêtes](requests.md).

## Réponses

### Ce qu'un handler peut renvoyer

| Valeur renvoyée | Réponse |
|---|---|
| `&'static str`, `String`, `Cow<'static, str>` | `200`, `text/plain; charset=utf-8` |
| `Json(valeur)`, `serde_json::Value` (`json!`) | `200`, `application/json` |
| `Html(corps)` | `200`, `text/html; charset=utf-8` |
| `Bytes`, `Vec<u8>`, `&'static [u8]` | `200`, `application/octet-stream` |
| `()` | `200`, corps vide |
| `StatusCode::NOT_FOUND` | Ce statut, avec sa raison en texte |
| `(statut, valeur)` | `valeur` avec ce statut (`u16`, `i32` ou `StatusCode`) |
| `Option<T>` | `T`, ou `404` si `None` |
| `Result<T, E>` | `T` ou `E` (les deux doivent être des réponses) |
| `Error` | Son statut et `{"error": "message"}` |
| `Redirect::to(url)` | `302` avec `Location` |
| `Body` | `200` avec ce corps (par ex. un flux) |
| `Response`, `http::Response<Body>` | Telle que construite |

### Construire une `Response`

| Usage | Description |
|---|---|
| `Response::new()` | Une réponse `200` vide |
| `.status(code)` | Statut (`res.status()`) |
| `.header(nom, valeur)` | Définit un en-tête en remplaçant l'existant (noms ou valeurs invalides ignorés) |
| `.append_header(nom, valeur)` | Ajoute un en-tête sans remplacer |
| `.content_type(valeur)` | `Content-Type` (`res.type()`) |
| `.text(corps)`, `.html(corps)`, `.json(valeur)` | Corps avec le type de contenu correspondant |
| `.send(corps)` | Corps, sans toucher au type de contenu |
| `.cookie(cookie)` | Ajoute un `Set-Cookie` (`res.cookie()`) |
| `.clear_cookie(nom)` | Supprime un cookie dans le navigateur (`res.clearCookie()`) |
| `.attachment(nom_de_fichier)` | Téléchargement sous ce nom (`res.attachment()`) |
| `res.status_code()`, `res.get_header(nom)` | Lit le statut, un en-tête |
| `res.set_status(code)`, `res.set_header(nom, valeur)` | Modifie en place (`&mut`) |
| `res.headers()`, `res.headers_mut()` | Tous les en-têtes |
| `res.body()`, `res.body_mut()`, `res.into_body()` | Le corps |
| `res.error()`, `res.take_error()` | L'`Error` à l'origine de la réponse, s'il y en a une |
| `res.extensions()`, `res.extensions_mut()` | Données typées attachées à la réponse |
| `res.into_http()`, `Response::from_http(r)` | Conversion vers et depuis `http::Response<Body>` |

### Les raccourcis `res`

| Usage | Description |
|---|---|
| `res::status(code)` | Une `Response` avec ce statut, à enchaîner (`res::status(201).json(v)`) |
| `res::send(corps)`, `res::text(t)`, `res::html(h)`, `res::json(v)` | Une réponse `200` avec ce corps |
| `res::redirect(url)` | Redirection `302` |
| `res::send_status(code)` | Le statut et sa raison en texte (`res.sendStatus()`) |
| `res::file(chemin).await` | Envoie un fichier avec `ETag`, `304` et `Range` (`206`) ; `404` s'il n'existe pas (`res.sendFile()`) |
| `res::download(chemin, nom).await` | Idem, en pièce jointe (`res.download()`) |

### Redirections, cookies et corps

| Usage | Description |
|---|---|
| `Redirect::to(url)` | `302 Found` |
| `Redirect::permanent(url)` | `301 Moved Permanently` |
| `Redirect::see_other(url)` | `303 See Other` (après un POST) |
| `Redirect::temporary(url)` | `307 Temporary Redirect` (conserve la méthode) |
| `Cookie::new(nom, valeur)` | Un cookie avec `Path=/` |
| `.path(p)`, `.domain(d)`, `.max_age(durée)` | Attributs du cookie |
| `.secure(bool)`, `.http_only(bool)`, `.same_site(SameSite::Lax)` | Attributs du cookie (`vitesse::SameSite` : `Strict`, `Lax`, `None`) |
| `Body::empty()`, `Body::from(x)` | Corps vide, ou depuis `&'static str`, `String`, `Vec<u8>`, `Bytes`… |
| `Body::from_stream(flux)` | Corps envoyé en flux (`chunked`) depuis un `Stream` de `Result<impl Into<Bytes>, E>` |
| `Body::wrap(corps)` | Enveloppe n'importe quel `http_body::Body` (proxy) |
| `body.size()`, `body.to_bytes().await` | Taille si elle est connue ; lit tout en mémoire |

Voir [Réponses](responses.md).

## Erreurs

| Usage | Description |
|---|---|
| `Error::new(statut, message)` | Une erreur avec un statut et un message pour le client |
| `Error::from_status(statut)` | Le message est la raison standard (`Not Found`…) |
| `Error::bad_request(msg)` | `400` |
| `Error::unauthorized(msg)` | `401` |
| `Error::forbidden(msg)` | `403` |
| `Error::not_found(msg)` | `404` |
| `Error::conflict(msg)` | `409` |
| `Error::payload_too_large()` | `413` |
| `Error::unprocessable(msg)` | `422` |
| `Error::internal(msg)` | `500` |
| `.with_source(err)` | Attache la cause d'origine (journalisée pour les erreurs 5xx) |
| `err.status()`, `err.message()`, `err.source()` | Lit l'erreur |
| `vitesse::Result<T>` | Alias de `Result<T, vitesse::Error>` |
| `?` sur n'importe quelle erreur standard | `500 {"error":"Internal Server Error"}` ; la cause est seulement journalisée |
| Panique dans un handler ou un middleware | `500` ; le serveur continue de tourner |

Voir [Erreurs](errors.md).

## Middlewares

| Usage | Description |
|---|---|
| `async fn mw(req: Request, next: Next) -> Response` | Un middleware sous forme de fonction |
| `\|req: Request, next: Next\| async move { … }` | Un middleware sous forme de closure |
| `next.run(req).await` | Poursuit la chaîne et renvoie la `Response` (`next()`) |
| Renvoyer une réponse sans appeler `next` | Arrête la chaîne (authentification, cache…) |
| `app.middleware(mw)` | Pour toutes les requêtes |
| `router.middleware(mw)` | Pour les routes de ce routeur, et les réponses `404`, `405` et `OPTIONS` sous son préfixe |
| `handler.with(mw)` | Pour une seule route |
| `impl Middleware for MonType` | `fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response>` |

| Middleware fourni | Équivalent | Remarques |
|---|---|---|
| `middleware::logger()` | `morgan('dev')` | Une ligne par requête sur la sortie standard |
| `middleware::cors()` | `cors()` | Toutes les origines (`*`) par défaut |
| `.allow_origin("https://…")` | `origin` | Seulement ces origines (cumulable) |
| `.allow_methods([Method::GET, …])` | `methods` | Par défaut : `GET, HEAD, PUT, PATCH, POST, DELETE` |
| `.allow_headers("…")`, `.expose_headers("…")` | `allowedHeaders`, `exposedHeaders` | Par défaut, les en-têtes demandés par le navigateur sont autorisés |
| `.allow_credentials(true)` | `credentials` | À combiner avec `allow_origin` : si toutes les origines sont autorisées, la réponse indique `*` et les navigateurs refusent les cookies |
| `.max_age(durée)` | `maxAge` | Durée de cache des requêtes préliminaires |
| `middleware::helmet()` | `helmet()` | En-têtes de sécurité |
| `middleware::timeout(durée)` | `connect-timeout` | `503` si la durée est dépassée |
| `middleware::serve_static(dossier)` | `express.static()` | Équivaut à `ServeDir::new(dossier)` |

Voir [Middlewares](middleware.md).

## Routeurs

| Usage | Description |
|---|---|
| `Router::new()` | Un routeur vide (`express.Router()`) |
| `router.get(…)`, `.post(…)`, … `.route(…)`, `.ws(…)` | Les mêmes méthodes de routage que `App` |
| `router.mount(préfixe, autre)` | Routeurs imbriqués |
| `router.static_dir(…)`, `router.serve_dir(…)` | Fichiers statiques dans un routeur |
| `router.middleware(mw)` | Middleware du routeur : ses routes et toute autre requête sous son préfixe (`router.use`) |
| `app.mount("/api", router)` | Le monte ; le préfixe peut contenir des paramètres (`/users/:id/posts`) |

Voir [Routeurs](routers.md).

## Fichiers statiques

| Usage | Description |
|---|---|
| `ServeDir::new(dossier)` | Sert le dossier `dossier` |
| `.index(Some("accueil.html"))`, `.index(None)` | Fichier servi pour un dossier (par défaut `index.html`), ou aucun |
| `.max_age(durée)` | Cache navigateur (`Cache-Control: max-age`) |
| `.dotfiles(true)` | Autorise les fichiers cachés (refusés par défaut) |
| `app.static_dir("/assets", "public")` | Comme route : `GET /assets/*` |
| `app.serve_dir("/assets", ServeDir::new("public").max_age(d))` | Comme route, avec des options |
| `app.middleware(ServeDir::new("public"))` | Comme middleware : les fichiers absents laissent passer vers les routes |

Inclus : types MIME, `index.html`, `ETag` et `Last-Modified` (`304`), requêtes partielles `Range` (`206`), envoi en flux des gros fichiers, protection contre `../` et les fichiers cachés. Voir [Fichiers statiques](static-files.md).

## WebSocket

| Usage | Description |
|---|---|
| `app.ws("/chat/:room", \|req, mut socket\| async move { … })` | Route WebSocket ; `426` pour du HTTP ordinaire, `400` pour une clé invalide, `405` pour une autre méthode |
| `socket.recv().await` | Message suivant : `Some(Ok(msg))`, `Some(Err(e))`, ou `None` une fois fermé |
| `socket.send(valeur).await` | `String` / `&str` en texte, `Vec<u8>` / `Bytes` / `&[u8]` en binaire, ou un `ws::Message` |
| `socket.close(1000, "bye").await` | Fermeture avec un code et une raison |
| `socket.protocol()` | Le sous-protocole choisi, s'il y en a un |
| `socket.split()` | `(WebSocketSender, WebSocketReceiver)`, pour envoyer et recevoir depuis deux tâches |
| `ws::Message::Text(String)`, `Binary(Bytes)`, `Ping(Bytes)`, `Pong(Bytes)`, `Close(Option<CloseFrame>)` | Les messages ; les pings reçoivent une réponse automatique |
| `msg.as_text()`, `msg.as_bytes()`, `msg.is_close()` | Raccourcis |
| `err.is_closed()` | La connexion est déjà fermée (`ws::Error`) |
| `ws::Upgrade::new(&req)?` | Dans une route `GET` : vérifie la poignée de main (`426`, `400`, `405`) |
| `.protocols(["v2", "v1"])`, `.offered_protocols()` | Choisit un sous-protocole proposé par le client ; la liste proposée |
| `.max_message_size(octets)` | Taille maximale des messages et des trames (par défaut `ws::DEFAULT_MAX_MESSAGE_SIZE`, 16 Mio) |
| `.on_upgrade(req, \|req, socket\| async move { … })` | Renvoie la réponse `101 Switching Protocols`, puis exécute le handler |

`WebSocket` implémente `Stream` et `Sink` ; les middlewares globaux et de routeur s'exécutent sur la poignée de main. Voir [WebSocket](websocket.md).

## Serveur

| Usage | Description |
|---|---|
| `3000`, `"3000"` | Écoute sur `0.0.0.0:3000` |
| `"127.0.0.1:8080"`, `"[::]:3000"`, `"localhost:3000"` | Écoute sur cette adresse |
| `String`, `SocketAddr`, `([127, 0, 0, 1], 8080)` | Autres formes acceptées (`ListenAddr`) |
| `server.local_addr()` | L'adresse réellement utilisée (port `0`) |
| `server.http3_addr()` | L'adresse UDP de HTTP/3, s'il est configuré |
| `server.run().await` | Sert jusqu'à `Ctrl+C` / `SIGTERM`, puis s'arrête proprement |
| `server.with_graceful_shutdown(signal).await` | Sert jusqu'à la fin de `signal`, au lieu de `Ctrl+C` / `SIGTERM` ; les requêtes en cours ont 10 s pour se terminer |

Voir [Configuration du serveur](server.md).

## HTTP/3

| Usage | Description |
|---|---|
| `app.http3(Http3::from_pem_files("fullchain.pem", "privkey.pem")?)` | HTTP/3 en UDP, à côté de HTTP/1.1 (`vitesse::http3::Http3`) |
| `Http3::from_pem(chaîne, clé)` | Chaîne de certificats et clé privée depuis la mémoire (PEM) |
| `Http3::from_rustls(config)` | Votre propre `rustls::ServerConfig` (TLS 1.3 ; ALPN `h3` ajouté s'il manque) |
| `.port(443)` | Port UDP (par défaut : le même numéro que le port TCP) |
| `.alt_svc(false)` | Pas d'en-tête `alt-svc` sur les réponses HTTP/1.1 (activé par défaut) |
| `.alt_svc_port(443)` | Port public annoncé dans `alt-svc` (derrière un proxy, Docker ou du NAT) |
| `req.version()` | `HTTP/3.0` pour une requête reçue en HTTP/3 |

Mêmes routes, middlewares et handlers dans les deux protocoles ; les corps de requête sont lus en entier avant le handler. Voir [HTTP/3 et QUIC](http3.md).

## Tests

| Usage | Description |
|---|---|
| `TestClient::new(app)` | Un client en mémoire (`vitesse::test::TestClient`) |
| `client.get(uri)`, `.post(uri)`, `.put(uri)`, `.patch(uri)`, `.delete(uri)` | Démarre une requête |
| `client.request(Method::HEAD, uri)` | Une méthode quelconque |
| `.header(nom, valeur)`, `.body(données)` | En-tête, corps brut |
| `.json(&valeur)`, `.form(&valeur)` | Corps JSON, formulaire encodé |
| `.await`, `.send().await` | Envoie ; renvoie une `TestResponse` |
| `res.status()`, `res.header(nom)`, `res.headers()` | Statut et en-têtes |
| `res.text()`, `res.bytes()`, `res.json::<T>()` | Le corps |
| `client.get("/route-ws")` | Sur une route WebSocket : seulement la réponse à la poignée de main (`426` pour du HTTP ordinaire) ; testez les conversations sur un vrai port |

Voir [Tests](testing.md).

## Réexportations

| Élément | Description |
|---|---|
| `vitesse::Bytes` | Le type `bytes::Bytes` |
| `vitesse::http`, `HeaderMap`, `Method`, `StatusCode`, `header` | Le crate `http` et ses types courants |
| `vitesse::serde_json`, `json!` | Le crate `serde_json` et sa macro |
| `vitesse::tokio` | Le crate `tokio`, avec les fonctionnalités qu'utilise Vitesse |
| `vitesse::http3::rustls` | Le crate `rustls` utilisé par HTTP/3 (feature `http3`) |
| `vitesse::DEFAULT_BODY_LIMIT` | `1024 * 1024` octets |
| `Handler`, `Middleware`, `IntoResponse`, `IntoStatus`, `HandlerExt`, `ListenAddr` | Les traits publics |
| `BoxFuture<T>`, `BoxError`, `Chained`, `Next`, `Server`, `SameSite`, `Cookie`, `Body` | Les autres types publics |
