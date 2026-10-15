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

`use vitesse::prelude::*;` importe `App`, `Router`, `Request`, `Response`, `Next`, `Error`, `Json`, `Html`, `Redirect`, `Cookie`, `Body`, `ServeDir`, `StatusCode`, `Method`, `IntoResponse`, `HandlerExt`, `json!`, le module `res`, le module `middleware`, le module `sse` (Server-Sent Events) et le module `ws` (WebSocket).

## Features Cargo

| Feature | Par défaut | Apporte |
|---|---|---|
| `ws` | Activée | `app.ws`, `router.ws` et le module `vitesse::ws` |
| `tls` | Désactivée | `app.tls` et le module `vitesse::tls` (HTTPS) |
| `http2` | Désactivée | HTTP/2 : sur TLS (ALPN) avec `tls`, et en clair (h2c, « prior knowledge ») |
| `http3` | Désactivée | `app.http3`, `server.http3_addr` et le module `vitesse::http3` |
| `multipart` | Désactivée | `req.multipart()` et le module `vitesse::multipart` |
| `compress` | Désactivée | `middleware::compress()` et `middleware::decompress()` |
| `session` | Désactivée | `app.cookie_secret`, les cookies signés, `middleware::session` et le module `vitesse::session` |
| `templates` | Désactivée | `app.views`, `app.templates`, `req.render` et le module `vitesse::templates` (minijinja) |
| `tower` | Désactivée | `middleware::layer`, `app.into_service()` et le module `vitesse::tower` |

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tls", "http2"] } # ajoute HTTPS et HTTP/2
# vitesse = { git = "https://github.com/maxlestage/Vitesse", default-features = false } # sans WebSocket
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
| `app.on_request(\|req: &mut Request\| …)` | Hook avant les middlewares et le routage (`onRequest`) ; `Err(erreur)` arrête la requête |
| `app.pre_handler(\|req: &mut Request\| …)` | Hook après le routage, avant les middlewares et le handler de la route (`preHandler`) |
| `app.on_response(\|res: &mut Response\| …)` | Hook sur chaque réponse avant son envoi (`onSend`) |
| `app.register(plugin)` | Ajoute un plugin : une `FnOnce(&mut App)` ou un `impl Plugin` (le `register` de Fastify) |
| `app.openapi("/openapi.json", OpenApi::new(titre, version))` | Sert le document OpenAPI 3.1 des routes ; `.swagger_ui("/docs")` ajoute Swagger UI |
| `app.openapi_document(&spec)` | Le même document en `serde_json::Value`, sans le servir |
| `app.state(valeur)` | Enregistre un état global, un par type (`app.locals`) |
| `app.body_limit(octets)` | Taille maximale d'un corps lu en mémoire (par défaut `DEFAULT_BODY_LIMIT`, 1 Mio) ; par route : `middleware::body_limit` |
| `app.trust_proxy(valeur)` | Reverse proxys de confiance (`app.set('trust proxy')`) : `false` (par défaut), `true`, un nombre de sauts, ou `"loopback, 10.0.0.0/8"` |
| `app.case_sensitive(false)` | Les routes correspondent quelle que soit la casse, comme par défaut dans Express (défaut de Vitesse : `true`) |
| `app.cookie_secret(secret)` | Secret qui signe les cookies et le cookie de session, 32 octets au moins (`cookieParser(secret)`, feature `session`) |
| `app.views(dossier)` | Dossier des gabarits HTML pour `req.render` (`app.set('views')`, feature `templates`) |
| `app.templates(env)` | Votre propre `Environment` minijinja : filtres, fonctions, gabarits intégrés (feature `templates`) |
| `app.workers(n)` | Nombre de threads pour `run` (par défaut : un par processeur) |
| `app.thread_per_core(bool)` | Mode un thread par cœur pour `run` (par défaut `true`, Linux uniquement) |
| `app.keep_alive(durée)` | Les connexions keep-alive inactives sont fermées au bout de cette durée (par défaut 60 s) ; `Duration::ZERO` désactive le keep-alive |
| `app.header_timeout(durée)` | Délai pour envoyer une tête de requête complète, puis `408` (par défaut 30 s) |
| `app.shutdown_timeout(durée)` | Délai de grâce laissé aux requêtes en cours lors de l'arrêt propre (par défaut 10 s) |
| `app.max_connections(n)` | Connexions ouvertes par boucle d'acceptation ; les autres attendent (par défaut : illimité) |
| `app.tls(config)` | Sert HTTPS, et HTTP/2 aux clients qui le proposent (feature `tls`) |
| `app.http3(config)` | Sert aussi l'application en HTTP/3 (QUIC, UDP), à côté de HTTP/1.1 (feature `http3`) |
| `app.into_service()` | L'application sous forme de `Service` tower (`vitesse::tower::AppService`, feature `tower`) |
| `app.run_unix(chemin)`, `app.listen_unix(chemin).await` | Sert sur un socket Unix au lieu d'un port TCP (Unix uniquement) |
| `app.bind_unix(chemin).await?` | Ouvre le socket Unix et renvoie un `UnixServer` (Unix uniquement) |

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
| `.name("user")` | Nomme la route ajoutée juste avant, pour `req.path_for` et `req.url_for` ; les noms sont globaux à l'application, préfixes de montage compris |
| `.doc(Operation::new().summary(…))` | Décrit la route ajoutée juste avant dans le document OpenAPI (`vitesse::openapi`) |
| `app.group(préfixe, \|groupe\| { … })` | Un `Router` rempli par la closure (ou une `fn(&mut Router)`) et monté sous `préfixe` |
| `app.param(nom, mw)` | Middleware exécuté avant le handler de chaque route qui a le paramètre `nom` (`app.param` d'Express) |
| `app.bind_param(nom, \|req, valeur\| async { … })` | Liaison de modèles : le `Ok(T)` du chargeur est dans `req.get::<T>()`, son `Err` est la réponse |

| Motif | Correspond à |
|---|---|
| `/users` | Exactement `/users` (et `/users/`) |
| `/users/:id` | Un segment, lu avec `req.param("id")` ; noms en lettres, chiffres et `_` |
| `/flights/:from-:to` | Plusieurs paramètres dans un segment, séparés par du texte : `LAX-SFO` donne `from=LAX`, `to=SFO` |
| `/files/:name.:ext` | `archive.tar.gz` donne `name=archive`, `ext=tar.gz` : le premier paramètre prend le moins possible |
| `/api/v:version/status` | Du texte et un paramètre dans un segment : `/api/v2/status` |
| `r"/users/:id(\d+)"` | Un paramètre contraint par une expression régulière, qui doit correspondre à la valeur entière |
| `/archive/:year?/:month?` | Des paramètres optionnels, à la fin seulement : `/archive`, `/archive/2024`, `/archive/2024/05` (`req.param("month")` vaut `None` en son absence) |
| `/files/*path` | Le reste du chemin, lu avec `req.param("path")` ; correspond aussi à `/files` |
| `/*` | Tous les chemins ; le paramètre s'appelle `*` |

Priorité : statique > segment contraint ou composé > paramètre > joker : `r"/users/:id(\d+)"` et `/users/:name` peuvent donc coexister. Les paramètres sont décodés (`%C3%A9` → `é`), la correspondance est sensible à la casse (sauf avec `app.case_sensitive(false)`), une mauvaise méthode répond `405` avec `Allow`, et une route invalide ou en double, ou une expression régulière invalide, panique au démarrage. Voir [Routage](routing.md#paramètres-de-chemin) et [Routes nommées](routing.md#routes-nommées-et-génération-durl).

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
| `req.version()` | La version HTTP : `HTTP/1.1`, `HTTP/2.0`, `HTTP/3.0`… |
| `req.set_uri(uri)` | Remplace l'URI (réécriture d'URL dans un middleware global) |
| `req.set_method(méthode)` | Change la méthode avant le routage (surcharge de méthode, dans un middleware global) |
| `req.params_as::<T>()?` | Tous les paramètres de route, désérialisés dans une structure (`Path<T>` d'axum) ; `400` en cas d'échec |
| `req.valid_json::<T>().await?`, `req.valid_form::<T>().await?`, `req.valid_query::<T>()?` | Lit, puis vérifie les règles `Validate` : `422` avec les messages de chaque champ |
| `req.route()` | Le motif de la route correspondante, préfixe de montage compris (`Some("/users/:id")`) ; `None` si aucune route ne correspond |
| `req.path_for("user", &[&42])?` | Le chemin d'une route nommée, valeurs encodées pour l'URL : `"/users/42"` ; `500` pour un nom inconnu ou une valeur manquante |
| `req.url_for("user", &[&42])?` | La même chose, avec le protocole et l'hôte de la requête : `"https://example.com/users/42"` |
| `req.header(nom)` | Un en-tête en `Option<&str>`, sans tenir compte de la casse (`req.get('host')`) |
| `req.header_all(nom)` | Toutes les valeurs d'un en-tête répété |
| `req.headers()`, `req.headers_mut()` | Tous les en-têtes (`HeaderMap`) |
| `req.content_type()` | Le `Content-Type` |
| `req.is("json")` | Vérifie le type du corps : type complet, sous-type ou `text/*` |
| `req.accepts(&["html", "json"])` | Le meilleur type pour l'en-tête `Accept` (`req.accepts()`) ; `None` : répondre `406` |
| `req.accepts_language(&["en", "fr"])` | La meilleure langue pour `Accept-Language` (`req.acceptsLanguages()`) |
| `req.hostname()` | L'hôte demandé, sans le port (`X-Forwarded-Host` derrière un proxy de confiance) |
| `req.host()` | L'hôte demandé, avec son port |
| `req.protocol()` | `"https"` ou `"http"` (`X-Forwarded-Proto` derrière un proxy de confiance) |
| `req.secure()` | `true` sur TLS, en HTTP/3, ou avec `X-Forwarded-Proto: https` venant d'un proxy de confiance |
| `req.cookie(nom)` | La valeur d'un cookie (`req.cookies.nom`) |
| `req.signed_cookie(nom)` | La valeur d'un cookie signé, `Option<String>` ; `None` s'il manque ou a été modifié (`req.signedCookies.nom`, feature `session`) |
| `req.sign_cookie(cookie)` | Signe un cookie avant `.cookie(...)` ; panique sans `app.cookie_secret` (feature `session`) |
| `req.session()` | La session du visiteur, `&Session` (`req.session`) ; `500` sans le middleware `session` |
| `req.render(nom, contexte)` | Rend un gabarit : `Result<Html<String>, Error>` (`res.render()`, feature `templates`) |
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
| `req.multipart()?` | Un corps `multipart/form-data`, champ par champ (feature `multipart`) ; `415` si ce n'est pas du multipart |
| `req.body_limit()`, `req.set_body_limit(octets)` | La limite de taille du corps de cette requête ; à modifier dans un middleware |
| `req.take_body()` | Le corps en flux (`Body`), non limité par `body_limit` |
| `req.set_body(corps)` | Remplace le corps |
| `req.state::<T>()` | L'état global (`&'static T`) ; panique, donc `500`, s'il manque |
| `req.try_state::<T>()` | Idem, en `Option` |
| `req.set(valeur)` | Attache une donnée à la requête (`res.locals`) ; le type doit être `Clone` |
| `req.get::<T>()` | Lit une donnée attachée par un middleware |
| `req.extensions()`, `req.extensions_mut()` | Les `http::Extensions` brutes |
| `req.ip()` | L'IP du client ; suit `X-Forwarded-For` derrière un proxy de confiance (`app.trust_proxy`) |
| `req.remote_addr()` | L'adresse et le port de la connexion elle-même (`None` sur un socket Unix) |

Le corps est lu à la demande et mis en cache : les méthodes de lecture peuvent être appelées plusieurs fois. Voir [Requêtes](requests.md).

### Formulaires multipart (feature `multipart`)

| Usage | Description |
|---|---|
| `let mut form = req.multipart()?;` | Lit le formulaire au fil de son arrivée ; le corps entier est limité par `req.body_limit()` (`413`) |
| `while let Some(field) = form.next_field().await? { … }` | Les champs, dans l'ordre ; lisez (ou abandonnez) l'un avant de demander le suivant |
| `field.name()` | Le nom du champ (`<input name="avatar">`) |
| `field.file_name()` | Pour un fichier, son nom chez le client : ne l'utilisez jamais tel quel comme chemin |
| `field.content_type()`, `field.headers()` | Le `Content-Type` du champ, tous ses en-têtes |
| `field.chunk().await?` | Le morceau suivant du contenu ; `None` à la fin |
| `field.bytes().await?`, `field.text().await?` | Tout le contenu en mémoire, en octets ou en texte UTF-8 |
| `field.save(chemin).await?` | Écrit le contenu dans un fichier au fil de son arrivée et renvoie sa taille (`u64`) ; le fichier partiel est supprimé si l'envoi échoue |

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
| `cookie.name()`, `cookie.value()`, `cookie.with_value(v)` | Relit un cookie ; le même cookie avec une autre valeur |
| `Body::empty()`, `Body::from(x)` | Corps vide, ou depuis `&'static str`, `String`, `Vec<u8>`, `Bytes`… |
| `Body::from_stream(flux)` | Corps envoyé en flux (`chunked`) depuis un `Stream` de `Result<impl Into<Bytes>, E>` |
| `Body::wrap(corps)` | Enveloppe n'importe quel `http_body::Body` (proxy) |
| `body.size()`, `body.to_bytes().await` | Taille si elle est connue ; lit tout en mémoire |

### Server-Sent Events (`vitesse::sse`)

| Usage | Description |
|---|---|
| `let (tx, stream) = sse::channel(16);` | Renvoyez `stream` depuis le handler, et envoyez les événements depuis une tâche lancée avec `tokio::spawn` |
| `tx.send(event).await` | Envoie un événement ; `Err(Closed)` une fois le navigateur parti |
| `tx.try_send(event)` | Sans attendre : `Err(TrySendError::Full(event))` si le tampon est plein, `Err(TrySendError::Closed(event))` si le navigateur est parti |
| `tx.is_closed()`, `tx.closed().await` | Le navigateur est-il parti ? Attendre qu'il parte ; `Sender` est `Clone` |
| `Sse::new(flux)` | Une réponse à partir de n'importe quel `Stream` de `Result<Event, E>` |
| `.keep_alive(KeepAlive::new().interval(d).text("ping"))`, `.no_keep_alive()` | Commentaire de keep-alive sur une connexion silencieuse (par défaut : toutes les 15 s) |
| `Event::new().data("…")` | Un événement ; des données sur plusieurs lignes donnent plusieurs lignes `data:` |
| `.json_data(&valeur)?` | Données sérialisées en JSON |
| `.event("tick")`, `.id("42")`, `.retry(durée)`, `.comment("…")` | Type d'événement, identifiant (`Last-Event-ID`), délai de reconnexion, commentaire |
| `event.to_string()` | L'événement tel qu'il est envoyé sur le réseau |

La réponse porte `content-type: text/event-stream` et `cache-control: no-cache`, et se termine quand tous les `Sender` sont abandonnés. Voir [Réponses](responses.md#server-sent-events).

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
| `.with_details(json!({ … }))` | Données structurées envoyées à côté du message : `{"error":"…","details":{…}}` ; relues avec `err.details()` |
| `err.status()`, `err.message()`, `err.source()` | Lit l'erreur |
| `vitesse::Result<T>` | Alias de `Result<T, vitesse::Error>` |
| `?` sur n'importe quelle erreur standard | `500 {"error":"Internal Server Error"}` ; la cause est seulement journalisée |
| Panique dans un handler ou un middleware | `500` ; le serveur continue de tourner |

Voir [Erreurs](errors.md).

## Validation

| Usage | Description |
|---|---|
| `impl Validate for T { fn validate(&self, v: &mut Validator) { … } }` | Les règles d'un type (`vitesse::validate`) |
| `v.field("nom", &self.nom).not_empty().max_length(50)` | Les règles d'un champ ; il s'arrête à sa première règle en échec |
| `v.optional(nom, &opt)`, `v.required(nom, &opt)` | Une `Option` : règles sautées si `None`, ou « is required » |
| `.min_length(n)`, `.max_length(n)`, `.not_empty()` | Caractères d'une chaîne, éléments d'une collection |
| `.min(x)`, `.max(x)`, `.between(a, b)`, `.one_of(&[…])` | Bornes et choix |
| `.email()`, `.url()`, `.alphanumeric()` | Formats d'une chaîne |
| `.check(\|v\| bool, "message")`, `.message("…")` | Une règle à vous ; remplace le message de la règle juste avant |
| `v.nested("adresse", &self.adresse)`, `v.each("articles", &self.articles)` | Valeurs imbriquées : `adresse.ville`, `articles[0].nom` |
| `v.error(nom, message)` | Un problème signalé directement (règles sur plusieurs champs) |

Voir [Validation des données](validation.md) et [OpenAPI](openapi.md).

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
| `middleware::body_limit(octets)` | `express.json({ limit })` | Limite de taille du corps pour une route (`.with()`) ou un routeur |
| `middleware::request_id()` | `express-request-id` | Identifiant dans `X-Request-Id` (repris de la requête, ou un nouvel UUID), dans `req.get::<RequestId>()` et sur la réponse |
| `.header("x-correlation-id")`, `.trust_incoming(false)` | | Un autre en-tête ; toujours générer un nouvel identifiant |
| `middleware::basic_auth(("admin", "secret"))` | `express-basic-auth` | `401` + `WWW-Authenticate` sans identifiants valides ; accepte aussi une closure `\|user, password\| -> bool` ou une liste de paires |
| `.realm("Admin")` | | Le domaine (*realm*) affiché par le navigateur (par défaut `Restricted`) |
| `req.get::<middleware::BasicUser>()` | | Le nom de l'utilisateur authentifié |
| `middleware::bearer_auth(jeton)` | | `Authorization: Bearer …` ; un `&'static str`, une `String`, une liste, ou une closure `\|token\| -> bool` |
| `middleware::constant_time_eq(a, b)` | | Compare deux secrets en temps constant |
| `middleware::concurrency_limit(n)` | | Au plus `n` requêtes en cours ; les autres attendent |
| `.max_wait(durée)` | | `503` + `Retry-After` au-delà de cette attente (`Duration::ZERO` : tout de suite) |
| `middleware::rate_limit(max, période)` | `express-rate-limit` | `max` requêtes par `période` et par IP, puis `429` + `Retry-After` ; en-têtes `X-RateLimit-Limit` et `X-RateLimit-Remaining` |
| `.key(\|req: &Request\| -> Option<String>)` | `keyGenerator` | Une autre clé de client (clé d'API, identifiant d'utilisateur) ; `None` : pas de limite |
| `middleware::session(stockage)` | `express-session` | Sessions côté serveur, lues avec `req.session()` (feature `session`) ; voir [plus bas](#cookies-signés-et-sessions-feature-session) |
| `middleware::compress()` | `compression()` | Réponses en Brotli ou gzip (feature `compress`) |
| `middleware::decompress()` | | Décompresse les corps de requête `gzip`, `br` et `deflate` (feature `compress`) |
| `middleware::layer(couche_tower)` | | N'importe quelle `Layer` tower / tower-http (feature `tower`) |
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
| `router.fallback(handler)` | Répond aux chemins sous le préfixe du routeur auxquels aucune route ne correspond, à la place du fallback de l'application |
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

## Cookies signés et sessions (feature `session`)

| Usage | Description |
|---|---|
| `app.cookie_secret(secret)` | Le secret (`&str`, `String` ou octets), 32 octets au moins, sinon panique ; lisez-le dans une variable d'environnement |
| `.cookie(req.sign_cookie(Cookie::new("theme", "dark")))` | Envoie `theme=dark.<signature>` : lisible mais pas modifiable, et pas chiffré (`res.cookie(…, { signed: true })`) |
| `req.signed_cookie("theme")` | `Some("dark")`, ou `None` s'il manque ou a été modifié (`req.signedCookies.theme`) |
| `app.middleware(middleware::session(MemoryStore::new()))` | Sessions côté serveur (`express-session`), avec l'identifiant dans le cookie `vitesse.sid` |
| `.cookie_name("sid")`, `.max_age(durée)`, `.rolling(true)` | Nom du cookie ; durée de vie après la dernière modification (par défaut 1 jour) ; prolongation à chaque requête |
| `.secure(bool)`, `.same_site(SameSite::Strict)`, `.domain(d)`, `.path(p)` | Attributs du cookie ; par défaut `HttpOnly`, `SameSite=Lax`, `Path=/`, et `Secure` en HTTPS |
| `req.session().get::<T>(clé)` | Une valeur, désérialisée : `Option<T>` |
| `session.insert(clé, valeur)?` | Enregistre n'importe quelle valeur `Serialize` |
| `session.remove(clé)`, `session.contains(clé)` | Retire une valeur et la renvoie ; vérifie une clé |
| `session.clear()`, `session.is_empty()` | Retire toutes les valeurs ; vérifie s'il n'y en a aucune |
| `session.regenerate()` | Un nouvel identifiant, les mêmes données : à appeler à la connexion |
| `session.destroy()` | Supprime la session, dans le stockage et dans le navigateur : la déconnexion |
| `session.id()` | L'identifiant, `None` tant que la session n'a jamais été enregistrée |
| `vitesse::session::MemoryStore` | Les sessions dans la mémoire du processus : perdues au redémarrage, non partagées entre instances |
| `impl SessionStore for MonStockage` | `load(id)`, `save(id, &record, ttl)` et `delete(id)`, qui renvoient chacune une `BoxFuture` ; un `Record` est une `serde_json::Map` |

Une session n'est enregistrée, et son cookie envoyé, que lorsqu'un handler la modifie (ou à chaque requête avec `rolling(true)`) ; le cookie est signé quand l'application a un `cookie_secret`. Voir [Cookies signés et sessions](sessions.md).

## Gabarits HTML (feature `templates`)

| Usage | Description |
|---|---|
| `app.views("templates")` | Le dossier des gabarits (`app.set('views')`) ; chaque gabarit est lu à sa première utilisation, puis gardé en cache |
| `req.render("users.html", json!({ … }))` | Rend un gabarit (`res.render()`) : `Result<Html<String>, Error>` ; n'importe quelle valeur `Serialize` comme contexte |
| `app.templates(env)` | Votre propre `vitesse::templates::minijinja::Environment` au lieu de `app.views` |
| `env.add_filter(nom, f)`, `env.add_function(nom, f)`, `env.add_global(nom, valeur)` | Vos propres filtres, fonctions et variables |
| `env.add_template(nom, include_str!("../templates/x.html"))` | Un gabarit intégré au binaire |
| `{{ value }}`, `{{ value\|upper }}` | Une valeur, échappée dans les gabarits `.html`, `.htm` et `.xml` ; un filtre |
| `{% for x in list %}…{% else %}…{% endfor %}`, `{% if %}…{% elif %}…{% endif %}` | Boucles et conditions |
| `{% extends "layout.html" %}`, `{% block content %}…{% endblock %}` | Gabarits de base |
| `{% include "partials/nav.html" %}`, `{% macro %}`, `{% import %}` | Fragments et macros |
| `{{ html\|safe }}` | Du HTML de confiance, non échappé |

Un gabarit manquant ou une erreur de rendu donne une `500` (la cause est journalisée) ; redémarrez le serveur pour voir la modification d'un gabarit. Voir [Gabarits HTML](templates.md).

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

`WebSocket` implémente `Stream` et `Sink` ; les middlewares globaux et de routeur s'exécutent sur la poignée de main. Les WebSockets fonctionnent en HTTP/1.1 et, avec la feature `http2`, en HTTP/2 (RFC 8441), mais pas en HTTP/3. Voir [WebSocket](websocket.md).

## Serveur

| Usage | Description |
|---|---|
| `3000`, `"3000"` | Écoute sur `0.0.0.0:3000` |
| `"127.0.0.1:8080"`, `"[::]:3000"`, `"localhost:3000"` | Écoute sur cette adresse |
| `String`, `SocketAddr`, `([127, 0, 0, 1], 8080)` | Autres formes acceptées (`ListenAddr`) |
| `server.local_addr()` | L'adresse réellement utilisée (port `0`) |
| `server.http3_addr()` | L'adresse UDP de HTTP/3, s'il est configuré |
| `server.run().await` | Sert jusqu'à `Ctrl+C` / `SIGTERM`, puis s'arrête proprement |
| `server.with_graceful_shutdown(signal).await` | Sert jusqu'à la fin de `signal`, au lieu de `Ctrl+C` / `SIGTERM` ; les requêtes en cours ont `shutdown_timeout` (10 s) pour se terminer |
| `unix_server.path()` | Le fichier du socket (`UnixServer`), par exemple pour modifier ses permissions |
| `unix_server.run().await`, `unix_server.with_graceful_shutdown(signal).await` | Comme pour `Server` ; le fichier du socket est supprimé à l'arrêt |

Voir [Configuration du serveur](server.md).

## HTTPS et HTTP/2

| Usage | Description |
|---|---|
| `app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?)` | HTTPS, TLS 1.2 et 1.3 (`vitesse::tls::Tls`, feature `tls`) |
| `Tls::from_pem(chaîne, clé)` | Chaîne de certificats et clé privée depuis la mémoire (PEM) |
| `Tls::from_rustls(config)` | Votre propre `rustls::ServerConfig` (TLS mutuel, plusieurs domaines…) ; ALPN `h2` et `http/1.1` ajoutés s'il n'en liste aucun |
| `tls.config()` | La configuration rustls (`&Arc<rustls::ServerConfig>`) |
| Feature `http2` | HTTP/2 sur TLS (navigateurs, ALPN), et h2c avec « prior knowledge » sur un port en clair |
| `req.version() == Version::HTTP_2` | Une requête reçue en HTTP/2 (`vitesse::http::Version`) |

Mêmes routes, middlewares et handlers dans tous les protocoles. Avec la feature `ws`, les navigateurs ouvrent aussi leurs WebSockets en HTTP/2 (RFC 8441), sur la même connexion que la page ; les handlers `app.ws` ne changent pas. Voir [HTTPS et HTTP/2](https.md).

## Proxys de confiance

| Usage | Description |
|---|---|
| `app.trust_proxy(false)` | Personne (par défaut) : `req.ip()` est l'adresse de la connexion |
| `app.trust_proxy("loopback")` | Les proxys à ces adresses ou dans ces réseaux : IP, CIDR (`10.0.0.0/8`), `loopback`, `linklocal`, `uniquelocal`, séparés par des virgules |
| `app.trust_proxy(1)` | Les `n` proxys les plus proches de l'application, quelle que soit leur adresse |
| `app.trust_proxy(true)` | Tous les proxys : le client est l'adresse la plus à gauche de `X-Forwarded-For` |
| `vitesse::TrustProxy` | `None`, `All`, `Hops(n)`, `Networks(Vec<Network>)` ; `"…".parse::<TrustProxy>()` pour gérer les erreurs |
| `trust.trusts(ip, hop)`, `network.contains(ip)` | Vérifie une adresse (`vitesse::Network` : `"10.0.0.0/8".parse()`) |

Effets : `req.ip()` (`X-Forwarded-For`), `req.protocol()` et `req.secure()` (`X-Forwarded-Proto`), `req.host()` et `req.hostname()` (`X-Forwarded-Host`). Voir [Mise en production](production.md#connaître-ladresse-ip-du-client).

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

Mêmes routes, middlewares et handlers dans les deux protocoles ; les corps de requête arrivent au handler en flux, avec les mêmes limites de taille. Avec `app.tls(...)` en plus, les réponses HTTPS portent `alt-svc` et les navigateurs passent directement à HTTP/3. Voir [HTTP/3 et QUIC](http3.md).

## tower (feature `tower`)

| Usage | Description |
|---|---|
| `app.middleware(middleware::layer(CorsLayer::permissive()))` | Une `Layer` tower comme middleware : aussi avec `router.middleware` et `handler.with` |
| `vitesse::tower::layer(l)` | Équivaut à `middleware::layer` |
| `app.into_service()` | L'application sous forme de `Service<http::Request<B>>` (`AppService`, `Clone` + `Copy`) : hyper, `Router::nest_service` d'axum, `ServiceExt::oneshot` |
| `vitesse::tower::{Layer, Service}` | Les traits de tower |
| `vitesse::tower::NextService` | La suite de la chaîne Vitesse, le service le plus interne d'une couche |

La couche peut modifier la méthode, l'URI, les en-têtes, les extensions et le corps ; ses erreurs deviennent des `500`. Le passage en WebSocket n'est pas disponible via `into_service` (`501`). Voir [Middlewares](middleware.md#couches-tower).

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
| `.header("cookie", "vitesse.sid=…")` | Renvoie un cookie lu dans `res.header("set-cookie")` : le client ne garde pas les cookies (sessions, cookies signés) |
| `app.into_service().oneshot(request).await` | Teste à travers tower (feature `tower` et `ServiceExt` de tower) |

Voir [Tests](testing.md).

## Réexportations

| Élément | Description |
|---|---|
| `vitesse::Bytes` | Le type `bytes::Bytes` |
| `vitesse::http`, `HeaderMap`, `Method`, `StatusCode`, `header` | Le crate `http` et ses types courants |
| `vitesse::serde_json`, `json!` | Le crate `serde_json` et sa macro |
| `vitesse::tokio` | Le crate `tokio`, avec les fonctionnalités qu'utilise Vitesse |
| `vitesse::http3::rustls`, `vitesse::tls::rustls` | Le crate `rustls` utilisé par HTTP/3 et HTTPS (features `http3` et `tls`) |
| `vitesse::templates::minijinja` | Le crate `minijinja` (feature `templates`) |
| `vitesse::session::{Session, Sessions, SessionStore, MemoryStore, Record}` | Les types des sessions (feature `session`) |
| `vitesse::DEFAULT_BODY_LIMIT` | `1024 * 1024` octets |
| `vitesse::VERSION` | La version du crate (`"0.1.0"`) |
| `Handler`, `Middleware`, `IntoResponse`, `IntoStatus`, `HandlerExt`, `ListenAddr` | Les traits publics |
| `BoxFuture<T>`, `BoxError`, `Chained`, `Next`, `Server`, `UnixServer`, `SameSite`, `Cookie`, `Body`, `TrustProxy`, `Network` | Les autres types publics |
