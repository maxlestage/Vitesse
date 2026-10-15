# Routeurs et sous-applications

Un `Router` regroupe des routes liées, avec leurs propres middlewares, et se monte sur l'application sous un préfixe : c'est l'équivalent de `express.Router()`. Les routeurs gardent un projet qui grandit bien organisé, avec un module par ressource.

## Créer et monter un routeur

```js
const users = express.Router();
users.get('/', listUsers);
users.get('/:id', showUser);
app.use('/users', users);
```

```rust
use vitesse::prelude::*;

let mut users = Router::new();
users.get("/", |_| async { "liste des utilisateurs" });
users.get("/:id", |req: Request| async move {
    format!("utilisateur {}", req.param("id").unwrap_or("?"))
});

let mut app = App::new();
app.mount("/users", users); // GET /users, GET /users/:id
```

Un `Router` a les mêmes méthodes de routage que `App` : `get`, `post`, `put`, `patch`, `delete`, `head`, `options`, `all`, `route(méthode, chemin, handler)`, ainsi que `ws`, `mount`, `static_dir` et `serve_dir`. Comme sur `App`, elles renvoient un `&mut Router` : vous pouvez donc les enchaîner.

```rust
let mut users = Router::new();
users
    .get("/", list_users)
    .post("/", create_user)
    .get("/:id", show_user)
    .delete("/:id", delete_user);
```

Les chemins se combinent comme on s'y attend : le `/` du routeur devient `/users`, `/:id` devient `/users/:id`, et `/users/` équivaut à `/users`.

> [!WARNING]
> Déclarez le routeur avec `let mut users = Router::new();`, puis appelez ses méthodes dans des instructions séparées. `let users = Router::new().get(...);` ne compile pas : les méthodes renvoient un `&mut Router` emprunté à une valeur temporaire, pas le `Router` lui-même.

## Les middlewares d'un routeur

`router.middleware(...)` ajoute un middleware à **toutes les routes de ce routeur** et, comme `router.use()` en Express, à toute autre requête sous son préfixe :

```rust
async fn require_token(req: Request, next: Next) -> Response {
    match req.header("authorization") {
        Some("Bearer secret") => next.run(req).await,
        _ => Error::unauthorized("jeton manquant").into_response(),
    }
}

let mut admin = Router::new();
admin.middleware(require_token);
admin.get("/stats", |_| async { "statistiques secrètes" });

app.mount("/admin", admin);                          // protégé
app.get("/", |_| async { "page d'accueil publique" }); // non concerné
```

Pour une requête, l'ordre est le suivant : middlewares globaux (`app.middleware`), puis middlewares du routeur dans leur ordre d'ajout, puis middlewares de la route (`.with`), puis le handler. Le middleware d'un routeur s'applique à toutes ses routes, qu'il ait été ajouté avant ou après elles, du moment qu'il l'est avant `mount`.

### Les requêtes auxquelles aucune route ne répond

Les middlewares d'un routeur s'exécutent aussi pour les requêtes sous son préfixe auxquelles aucune de ses routes ne répond : le `404` (le [fallback](#le-fallback-dun-routeur) du routeur s'il en a un, sinon celui de l'application), le `405 Method Not Allowed` et la réponse automatique à `OPTIONS` (`204` avec `Allow`). Dans l'exemple ci-dessus, `GET /admin/inconnu` passe lui aussi par `require_token` : sans jeton, le visiteur reçoit un `401` et ne sait même pas si la page existe.

C'est ce qui permet à un logger ou à CORS sur un routeur de fonctionner comme on s'y attend :

```rust
let mut api = Router::new();
api.middleware(middleware::logger()); // journalise aussi les 404 sous /api
api.middleware(middleware::cors());   // répond aussi aux requêtes préliminaires de /api/users
api.post("/users", |_| async { (201, "créé") });

app.mount("/api", api);
```

Avant un `POST /api/users` venant d'une autre origine, le navigateur envoie une requête préliminaire `OPTIONS /api/users` : `cors()` y répond avec un `204` et les en-têtes CORS. Un `GET /api/nope` est journalisé, puis reçoit son `404`.

Quelques précisions :

- Le préfixe est comparé segment par segment : un routeur monté sur `/api` couvre `/api` et `/api/...`, mais pas `/apix`.
- Les middlewares globaux (`app.middleware`) passent toujours en premier, pour chaque requête. Avec des routeurs imbriqués, viennent ensuite les middlewares du routeur extérieur, puis ceux du routeur intérieur.
- Si un middleware du routeur réécrit le chemin d'une telle requête avec `req.set_uri(...)`, le routage est refait sur le nouveau chemin.
- Un préfixe peut contenir des paramètres : un routeur monté sur `/users/:user_id/posts` couvre `/users/42/posts/...`, quelle que soit la valeur de `:user_id`. Les contraintes du préfixe sont vérifiées pour ces requêtes aussi : un routeur monté sur `/docs/:lang(en|fr|es)` ne couvre pas `/docs/de/...`, dont la `404` vient de l'application, sans les middlewares ni le fallback du routeur.

## Le fallback d'un routeur

Par défaut, un chemin inconnu sous le préfixe d'un routeur reçoit la `404` de l'application : votre `app.fallback`, ou la réponse par défaut `{"error":"Cannot GET /nope"}`. `router.fallback(handler)` y répond à la place. Une API peut ainsi garder des erreurs JSON pendant que le reste du site affiche une page HTML :

```rust
let mut api = Router::new();
api.get("/users", |_| async { Json(vec!["ada", "grace"]) });
api.fallback(|req: Request| async move {
    (404, Json(json!({ "error": format!("aucune route d'API pour {}", req.path()) })))
});

let mut app = App::new();
app.mount("/api", api);
app.fallback(|_| async { (404, Html("<h1>Page introuvable</h1>")) });
```

| Requête | Réponse |
|---|---|
| `GET /api/users` | la route |
| `GET /api/nope` | le fallback du routeur : `404 {"error":"aucune route d'API pour /api/nope"}` |
| `POST /api/users` | `405 Method Not Allowed` : le chemin existe, aucun fallback n'est donc appelé |
| `GET /nope` | le fallback de l'application : la page HTML |

- Le fallback s'exécute après les middlewares du routeur, comme l'une de ses routes : un routeur protégé par une authentification protège aussi son fallback, et les en-têtes qu'ajoutent ses middlewares figurent sur les réponses du fallback.
- Avec des routeurs imbriqués, c'est le routeur le plus profond doté d'un fallback qui l'emporte : un fallback sur le routeur monté sur `/api/admin` répond à `/api/admin/nope`, celui de `/api` répond au reste de `/api`. Un routeur sans fallback utilise celui de son parent, et en dernier lieu celui de l'application.
- Comme `app.fallback`, il peut répondre n'importe quoi, pas seulement une `404` : une redirection, une page HTML, ou l'`index.html` d'une application monopage qui vit sous le préfixe.

## Imbriquer des routeurs

Un routeur peut monter d'autres routeurs. Les middlewares du routeur parent enveloppent aussi les routes de ses enfants :

```rust
let mut users = Router::new();
users.get("/", |_| async { "utilisateurs" });

let mut posts = Router::new();
posts.get("/", |_| async { "articles" });

let mut api = Router::new();
api.middleware(require_token); // protège aussi /users et /posts
api.mount("/users", users);
api.mount("/posts", posts);

app.mount("/api/v1", api); // GET /api/v1/users, GET /api/v1/posts
```

## Groupes de routes : `app.group`

`app.group(préfixe, |groupe| { … })` crée un routeur, vous laisse le remplir et le monte : le `Route::prefix()->group()` de Laravel, le `app.route()` de Hono. C'est le plus court pour donner à quelques routes un préfixe et des middlewares à elles :

```rust
app.group("/admin", |admin| {
    admin.middleware(middleware::basic_auth(("admin", "s3cret")));
    admin.get("/stats", stats);          // GET /admin/stats, protégée
    admin.group("/users", |users| {      // les groupes s'imbriquent
        users.get("/:id", afficher);     // GET /admin/users/:id, protégée aussi
    });
});
```

La closure reçoit un `&mut Router` : une fonction qui en prend un convient aussi, et un module peut exporter ses routes sous forme d'une seule fonction, montable sous n'importe quel préfixe.

```rust
// src/routes/users.rs
pub fn routes(r: &mut Router) {
    r.get("/", lister).post("/", creer).get("/:id", afficher);
}

// src/main.rs
app.group("/api/users", routes::users::routes);
```

Tout ce que cette page dit des routeurs vaut pour les groupes : middlewares qui couvrent tout le préfixe, fallback, noms, etc.

## Des paramètres dans le préfixe

Un préfixe de montage peut contenir des paramètres, que les handlers lisent comme les autres :

```rust
let mut posts = Router::new();
posts.get("/", |req: Request| async move {
    format!("articles de l'utilisateur {}", req.param("user_id").unwrap_or("?"))
});

app.mount("/users/:user_id/posts", posts); // GET /users/42/posts
```

Tout ce que propose la [syntaxe des routes](routing.md#paramètres-de-chemin) fonctionne aussi dans un préfixe : plusieurs paramètres dans un segment (`/api/v:version`), les [contraintes](routing.md#contraintes-avec-des-expressions-régulières) et les [paramètres optionnels](routing.md#paramètres-optionnels). Ici, un seul routeur sert la documentation en trois langues, avec une page d'accueil par langue :

```rust
let mut docs = Router::new();
docs.get("/:page?", |req: Request| async move {
    let lang = req.param("lang").unwrap_or("en");
    let page = req.param("page").unwrap_or("index");
    format!("page {page} en {lang}")
});

app.mount("/docs/:lang(en|fr|es)", docs);
```

| Requête | Réponse |
|---|---|
| `GET /docs/fr` | `page index en fr` |
| `GET /docs/es/routing` | `page routing en es` |
| `GET /docs/de/routing` | `404` : `de` ne respecte pas la contrainte |

`req.route()` donne le motif complet, préfixe compris : `/docs/:lang(en|fr|es)/:page?`. Les paramètres optionnels doivent rester à la fin du chemin complet, préfixe et route compris : avec un préfixe comme `/:lang?`, toute route autre que le `/` du routeur fait paniquer `mount`. Placez plutôt le paramètre optionnel dans les routes du routeur, comme ci-dessus.

## Les hooks d'un routeur

Les routeurs et les groupes ont leurs propres hooks, qui ne s'exécutent que pour eux (l'encapsulation de Fastify) : `on_request` et `on_response`, des fonctions simples exécutées avant les middlewares du routeur, et `on_request_async`, qui s'exécute là où il a été ajouté parmi les middlewares :

```rust
app.group("/admin", |admin| {
    admin.on_request(|req: &mut Request| match req.header("x-admin") {
        Some(_) => Ok(()),
        None => Err(Error::forbidden("réservé aux administrateurs")),
    });
    admin.on_response(|res: &mut Response| {
        res.set_header("cache-control", "no-store");
    });
    admin.get("/stats", stats);
});
```

Comme les middlewares du routeur, ils couvrent aussi les réponses `404` et `405` sous son préfixe, et `on_response` voit les erreurs de ses propres hooks `on_request`. Les hooks de l'application (voir [Middlewares](middleware.md)) s'exécutent pour toutes les requêtes.

## Routes WebSocket

`router.ws(chemin, handler)` déclare une route [WebSocket](websocket.md), exactement comme `app.ws`. Le préfixe du routeur s'applique, et ses middlewares s'exécutent sur la requête de poignée de main, avant que la connexion ne soit acceptée : un middleware qui répond `401` refuse la connexion.

```rust
let mut live = Router::new();
live.middleware(require_token); // vérifie aussi la poignée de main WebSocket
live.ws("/feed", |_req, mut socket| async move {
    let _ = socket.send("bienvenue sur le fil en direct").await;
});

app.mount("/live", live); // ws://example.com/live/feed
```

## Routes nommées dans les routeurs

Une route déclarée dans un routeur peut recevoir un [nom](routing.md#routes-nommées-et-génération-durl) avec `.name(...)`, exactement comme sur `App`. Le nom appartient à toute l'application, et le chemin qu'il construit inclut le préfixe de montage :

```rust
let mut api = Router::new();
api.get(r"/items/:id(\d+)", show_item).name("item");
api.post("/items", |req: Request| async move {
    // Les paramètres du préfixe viennent en premier : ":version", puis ":id".
    let link = req.path_for("item", &[&"v1", &7])?; // "/api/v1/items/7"
    Ok::<_, Error>((201, Json(json!({ "link": link }))))
});

app.mount("/api/:version", api);
```

- N'importe quel handler peut utiliser le nom, dans le routeur ou ailleurs dans l'application : `req.path_for("item", …)` fonctionne aussi depuis `app.get("/", …)`.
- Les valeurs suivent l'ordre des paramètres dans le chemin complet : ceux du préfixe d'abord, puis ceux de la route.
- Les noms doivent être uniques dans toute l'application : deux routeurs qui nomment tous deux une route `"show"` font paniquer le second `mount` au démarrage. Préfixez-les (`"users.show"`, `"posts.show"`).
- `name` suit une route : l'appeler juste après `mount` panique, puisque `mount` ajoute des routes mais n'en est pas une.

## Comment fonctionne le montage

`mount` copie les routes du routeur dans l'arbre de routage de l'application, une fois pour toutes, au démarrage. Quelques conséquences :

- **Aucun coût par requête** : une route montée est exactement aussi rapide qu'une route déclarée directement sur `App`.
- **Le routeur est consommé** par `mount`. Pour monter deux fois les mêmes routes (`/api/v1` et `/api/latest`, par exemple), construisez le routeur dans une fonction et appelez-la deux fois.
- **Les conflits sont détectés au démarrage** : si la même méthode et le même chemin sont définis deux fois, même dans des routeurs différents, le programme panique dès son lancement, avec un message comme `duplicate route: GET /api/x is already defined`.
- **La priorité des routes** est globale : un segment statique passe avant un segment contraint ou composé, qui passe avant un simple paramètre, qui passe lui-même avant un joker (`/users/new`, puis `/users/:id(\d+)`, puis `/users/:name`), quel que soit le routeur d'origine. Voir [Comment les routes sont choisies](routing.md#comment-les-routes-sont-choisies).
- **Les noms des routes** sont globaux eux aussi : voir [Routes nommées dans les routeurs](#routes-nommées-dans-les-routeurs).
- Le gestionnaire d'erreurs (`app.on_error`) et l'état (`app.state`) appartiennent à l'application, pas aux routeurs. Un routeur peut toutefois avoir son propre [fallback](#le-fallback-dun-routeur), qui répond avant `app.fallback`.

## Plugins : `app.register`

Un plugin est un morceau d'application réutilisable : routes, middlewares, état, hooks… ajoutés en un appel, comme le `register` de Fastify. Toute fonction qui prend un `&mut App` est un plugin, et une structure qui implémente `vitesse::Plugin` peut porter des options :

```rust
use vitesse::Plugin;

fn sante(app: &mut App) {
    app.get("/healthz", |_| async { "ok" });
}

struct Metriques {
    chemin: &'static str,
}

impl Plugin for Metriques {
    fn register(self, app: &mut App) {
        app.state(Compteurs::default());
        app.on_response(|res: &mut Response| { /* compter les statuts */ });
        app.get(self.chemin, afficher_metriques);
    }
}

app.register(sante).register(Metriques { chemin: "/metrics" });
```

Un plugin agit sur toute l'application (middlewares globaux, hooks, état). Pour des routes encapsulées sous un préfixe, avec leurs propres middlewares, utilisez un [groupe](#groupes-de-routes--appgroup).

## Organiser un projet

Les routeurs permettent de découper facilement une application en modules, un par ressource. Une organisation typique :

```text
mon-app/
├── Cargo.toml
└── src/
    ├── main.rs        // construit l'App et démarre le serveur
    └── routes/
        ├── mod.rs     // assemble l'API
        ├── users.rs   // /api/users
        └── posts.rs   // /api/users/:user_id/posts
```

Chaque module expose une fonction qui renvoie son `Router` (`posts.rs` suit le même modèle que `users.rs`) :

```rust
// src/routes/users.rs
use vitesse::prelude::*;

pub fn router() -> Router {
    let mut users = Router::new();
    users.get("/", list).post("/", create).get("/:id", show);
    users
}

async fn list(_req: Request) -> Json<Vec<&'static str>> {
    Json(vec!["ada", "grace"])
}

async fn create(_req: Request) -> (StatusCode, &'static str) {
    (StatusCode::CREATED, "créé")
}

async fn show(req: Request) -> vitesse::Result<String> {
    let id: u32 = req.param_as("id")?;
    Ok(format!("utilisateur {id}"))
}
```

```rust
// src/routes/mod.rs
use vitesse::prelude::*;

pub mod posts;
pub mod users;

pub fn api() -> Router {
    let mut api = Router::new();
    api.mount("/users", users::router());
    api.mount("/users/:user_id/posts", posts::router());
    api
}
```

```rust
// src/main.rs
use vitesse::prelude::*;

mod routes;

fn build_app() -> App {
    let mut app = App::new();
    app.middleware(middleware::logger());
    app.mount("/api", routes::api());
    app
}

fn main() -> std::io::Result<()> {
    build_app().run(3000)
}
```

Garder une fonction `build_app()` séparée de `main` permet à vos tests de construire exactement la même application et de l'appeler sans réseau : voir [Tests](testing.md).
