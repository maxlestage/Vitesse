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

Un `Router` a les mêmes méthodes de routage que `App` : `get`, `post`, `put`, `patch`, `delete`, `head`, `options`, `all`, `route(méthode, chemin, handler)`, ainsi que `mount`, `static_dir` et `serve_dir`. Comme sur `App`, elles renvoient un `&mut Router` : vous pouvez donc les enchaîner.

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

`router.middleware(...)` ajoute un middleware à **toutes les routes de ce routeur**, et seulement à elles :

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

> [!NOTE]
> Les middlewares d'un routeur ne s'exécutent que pour les requêtes qui correspondent à l'une de ses routes. Une requête vers `/admin/inconnu` reçoit le `404` de l'application sans passer par `require_token`, alors qu'en Express un middleware ajouté avec `router.use()` s'exécute pour toute requête qui entre dans le routeur.

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

## Des paramètres dans le préfixe

Un préfixe de montage peut contenir des paramètres, que les handlers lisent comme les autres :

```rust
let mut posts = Router::new();
posts.get("/", |req: Request| async move {
    format!("articles de l'utilisateur {}", req.param("user_id").unwrap_or("?"))
});

app.mount("/users/:user_id/posts", posts); // GET /users/42/posts
```

## Comment fonctionne le montage

`mount` copie les routes du routeur dans l'arbre de routage de l'application, une fois pour toutes, au démarrage. Quelques conséquences :

- **Aucun coût par requête** : une route montée est exactement aussi rapide qu'une route déclarée directement sur `App`.
- **Le routeur est consommé** par `mount`. Pour monter deux fois les mêmes routes (`/api/v1` et `/api/latest`, par exemple), construisez le routeur dans une fonction et appelez-la deux fois.
- **Les conflits sont détectés au démarrage** : si la même méthode et le même chemin sont définis deux fois, même dans des routeurs différents, le programme panique dès son lancement, avec un message comme `duplicate route: GET /api/x is already defined`.
- **La priorité des routes** est globale : un segment statique passe avant un paramètre, qui passe avant un joker (`/users/new` avant `/users/:id`), quel que soit le routeur d'origine.
- Le handler de repli (`app.fallback`), le gestionnaire d'erreurs (`app.on_error`) et l'état (`app.state`) appartiennent à l'application, pas aux routeurs.

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
