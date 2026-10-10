# Routage

Le routage choisit le handler qui répond à une requête, d'après sa méthode et son chemin. Vitesse reprend le vocabulaire d'Express (`app.get`, `:id`, `app.all`), mais son routeur est un arbre : l'ordre de déclaration des routes n'a pas d'importance, et une mauvaise méthode reçoit une vraie `405`.

## Déclarer des routes

Chaque méthode HTTP a sa fonction. Elles existent sur `App` et sur [`Router`](routers.md) :

| Vitesse | Express | Méthodes acceptées |
|---|---|---|
| `app.get(chemin, handler)` | `app.get()` | `GET`, et aussi `HEAD` (voir plus bas) |
| `app.post(chemin, handler)` | `app.post()` | `POST` |
| `app.put(chemin, handler)` | `app.put()` | `PUT` |
| `app.patch(chemin, handler)` | `app.patch()` | `PATCH` |
| `app.delete(chemin, handler)` | `app.delete()` | `DELETE` |
| `app.head(chemin, handler)` | `app.head()` | `HEAD` |
| `app.options(chemin, handler)` | `app.options()` | `OPTIONS` |
| `app.all(chemin, handler)` | `app.all()` | toutes les méthodes |
| `app.route(méthode, chemin, handler)` | `app[méthode]()` | la méthode passée, y compris non standard |

```rust
app.get("/articles", |_| async { "liste des articles" });
app.post("/articles", |_| async { (201, "article créé") });
app.put("/articles/:id", |_| async { "article remplacé" });
app.patch("/articles/:id", |_| async { "article modifié" });
app.delete("/articles/:id", |_| async { StatusCode::NO_CONTENT });
app.all("/ping", |_| async { "pong" });

// Une méthode non standard
let purge = Method::from_bytes(b"PURGE").unwrap();
app.route(purge, "/cache", |_| async { "cache vidé" });
```

> [!WARNING]
> `app.route` n'est pas le `app.route(chemin)` d'Express. Dans Vitesse, il enregistre une seule route pour la `Method` passée. Pour mettre plusieurs méthodes sur le même chemin, enchaînez plutôt les appels (voir plus bas).

### Handlers

Un handler est une fonction ou une closure `async` qui prend une `Request` et renvoie n'importe quoi qui implémente `IntoResponse` : du texte, `Json(...)`, un tuple `(statut, corps)`, un `Result`, etc. (voir [Envoyer la réponse](responses.md)).

```rust
// Une closure qui ignore la requête
app.get("/", |_| async { "Accueil" });

// Une closure qui s'en sert : précisez son type et ajoutez `async move`
app.get("/hello/:name", |req: Request| async move {
    format!("Bonjour, {} !", req.param("name").unwrap_or("inconnu"))
});

// Une fonction nommée
async fn list_users(_req: Request) -> Json<Vec<&'static str>> {
    Json(vec!["ada", "grace"])
}
app.get("/users", list_users);
```

### Enchaîner les appels

Chaque fonction de routage renvoie `&mut Self`, vous pouvez donc enchaîner les appels. Cela ressemble un peu au `app.route('/users').get(...).post(...)` d'Express :

```rust
app.get("/users", list_users)
    .post("/users", create_user)
    .get("/users/:id", show_user)
    .delete("/users/:id", delete_user);
```

Enchaînez sur votre variable, après `let mut app = App::new();`. Enchaîner directement sur `App::new()` vous laisserait une référence vers une valeur temporaire.

## Paramètres de chemin

Un segment qui commence par `:` capture cette partie du chemin. `req.param(nom)` le lit et renvoie une `Option<&str>` :

```rust
app.get("/users/:id/posts/:post", |req: Request| async move {
    let user = req.param("id").unwrap();
    let post = req.param("post").unwrap();
    format!("article {post} de l'utilisateur {user}")
});
```

`GET /users/42/posts/7` répond `article 7 de l'utilisateur 42`. Le `unwrap()` ne peut pas échouer ici, puisque la route ne correspond que si les deux paramètres sont présents.

- `req.params()` parcourt toutes les paires `(nom, valeur)`, dans l'ordre.
- Les valeurs sont décodées : `/users/Fran%C3%A7ois` donne `François`. Un `+` reste un `+`, car il ne signifie « espace » que dans la query string. `req.path()` renvoie toujours le chemin brut.
- Un paramètre occupe un segment entier. Il ne correspond jamais à un segment vide ni à quoi que ce soit contenant un `/`.
- Un nom ne peut contenir que des lettres, des chiffres et `_`. Les segments partiels comme `/vols/:de-:vers` et les expressions régulières ne sont pas pris en charge. Capturez le segment entier et découpez-le vous-même.

## Paramètres typés

`req.param_as::<T>(nom)` convertit un paramètre dans n'importe quel type qui implémente `FromStr` : entiers, `bool`, `f64`, `IpAddr`, vos propres types, etc. Si le paramètre manque ou ne se convertit pas, il renvoie une erreur `400 Bad Request`. Un `?` suffit donc :

```rust
async fn show_user(req: Request) -> vitesse::Result<String> {
    let id: u64 = req.param_as("id")?;
    Ok(format!("utilisateur n°{id}"))
}

app.get("/users/:id", show_user);
```

```text
GET /users/42   → 200 utilisateur n°42
GET /users/abc  → 400 {"error":"invalid parameter 'id': 'abc'"}
GET /users/-1   → 400 (un u64 ne peut pas être négatif)
```

Pour vos propres types, implémentez `FromStr` :

```rust
use std::str::FromStr;

enum Format {
    Json,
    Csv,
}

impl FromStr for Format {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "json" => Ok(Format::Json),
            "csv" => Ok(Format::Csv),
            _ => Err(()),
        }
    }
}

app.get("/export/:format", |req: Request| async move {
    let message = match req.param_as::<Format>("format")? {
        Format::Json => "export en JSON",
        Format::Csv => "export en CSV",
    };
    Ok::<_, Error>(message)
});
```

`GET /export/csv` répond `export en CSV`, et `GET /export/xml` reçoit une `400`.

## Jokers

Un segment `*nom` capture tout le reste du chemin, barres obliques comprises. Express 5 utilise la même syntaxe :

```rust
app.get("/files/*path", |req: Request| async move {
    format!("vous avez demandé {}", req.param("path").unwrap())
});
```

| Requête | `req.param("path")` |
|---|---|
| `/files/rapport.pdf` | `"rapport.pdf"` |
| `/files/2024/t1/rapport.pdf` | `"2024/t1/rapport.pdf"` |
| `/files` ou `/files/` | `""` (vide) |

- Un joker doit être le dernier segment. `/files/*path/edit` est refusé au démarrage.
- Un joker anonyme `*` fonctionne aussi. Sa valeur est rangée sous le nom `"*"`, que vous lisez avec `req.param("*")`.
- `/*` tout seul attrape tous les chemins qu'aucune route plus précise ne prend.

## Comment les routes sont choisies

### La route la plus précise l'emporte

Quand plusieurs routes pourraient correspondre, Vitesse les compare segment par segment : **statique > paramètre > joker**. L'ordre de déclaration n'a pas d'importance. Dans Express, c'est la première route enregistrée qui gagne.

```rust
app.get("/users/new", |_| async { "formulaire de création" });
app.get("/users/:id", |_| async { "un utilisateur" });
app.get("/users/*rest", |_| async { "tout le reste sous /users" });
```

| Requête | Route utilisée |
|---|---|
| `/users/new` | `/users/new` (le statique bat le paramètre) |
| `/users/42` | `/users/:id` |
| `/users/42/settings` | `/users/*rest` |
| `/users` | `/users/*rest` (joker vide) |

Si une branche ne mène nulle part, le routeur revient en arrière et essaie l'option suivante. Avec `/a/:x/c` et `/a/b/d`, une requête vers `/a/b/c` correspond à `/a/:x/c` avec `x = "b"`.

### Barre oblique finale, casse et query string

- La barre oblique finale est ignorée : `/users/` correspond à `/users`. Express fait de même par défaut.
- Les chemins sont **sensibles à la casse** : `/Users` ne correspond pas à `/users`. Express, lui, ignore la casse par défaut.
- La query string ne joue aucun rôle : `/search?q=rust` correspond à `/search`.

### Routes invalides ou en double

Vitesse détecte les erreurs de déclaration au démarrage de l'application, pas à l'arrivée d'une requête. `app.get(...)` panique avec un message explicite si :

- le chemin ne commence pas par `/` ;
- il contient un segment vide (`/a//b`) ;
- un nom de paramètre est invalide ;
- un joker n'est pas le dernier segment ;
- la même méthode est enregistrée deux fois sur le même chemin.

Deux motifs qui ne diffèrent que par le nom de leurs paramètres sont la même route, donc `/users/:id` et `/users/:name` entrent en conflit :

```text
thread 'main' panicked at src/main.rs:12:9:
duplicate route: GET /users/:name is already defined
```

## HEAD, OPTIONS et 405

Vitesse se charge des comportements HTTP standard :

- **HEAD** : une requête `HEAD` utilise la route `GET` et reçoit les mêmes en-têtes sans le corps, sauf si vous enregistrez `app.head(...)`.
- **405 Method Not Allowed** : quand le chemin existe mais pas pour cette méthode, la réponse est une `405` avec un en-tête `Allow` qui liste les méthodes disponibles. Express répond `404` dans ce cas.
- **OPTIONS** : sans route `app.options(...)` explicite, une requête `OPTIONS` reçoit `204 No Content` avec le même en-tête `Allow`.

```rust
app.get("/items", list_items);
app.post("/items", create_item);
```

```http
DELETE /items HTTP/1.1

HTTP/1.1 405 Method Not Allowed
content-type: application/json
allow: GET, HEAD, POST, OPTIONS

{"error":"Method Not Allowed"}
```

Sur le même chemin, `OPTIONS /items` reçoit une `204 No Content` avec `allow: GET, HEAD, POST, OPTIONS`.

`app.all(...)` accepte toutes les méthodes sur son chemin, il ne produit donc jamais de `405`. Si le même chemin a aussi une route pour une méthode précise, c'est elle qui l'emporte sur `all`.

> [!TIP]
> Les navigateurs envoient leurs requêtes CORS préliminaires en `OPTIONS`. Pour y répondre, utilisez `middleware::cors()`, qui les traite avant le routage. Voir [Middlewares](middleware.md).

## Fallback : personnaliser la 404

Quand aucune route ne correspond au chemin, Vitesse répond `404` avec `{"error":"Cannot GET /nope"}`, le même message qu'Express. `app.fallback` remplace ce handler. Il joue le rôle du dernier `app.use((req, res) => ...)` d'une application Express :

```rust
app.fallback(|req: Request| async move {
    (404, format!("Rien ici : {}", req.path()))
});
```

Un usage courant est l'application monopage (SPA) : envoyer `index.html` pour tout chemin inconnu et laisser le routeur du front-end prendre le relais.

```rust
app.fallback(|_| async { res::file("dist/index.html").await });
```

- Le fallback s'exécute après les middlewares globaux, comme n'importe quelle route.
- Il n'est pas appelé pour une `405`, puisque le chemin existe, simplement pas pour cette méthode.
- Pour changer d'un coup le format de *toutes* les réponses d'erreur (404, 405, 400, 500, etc.), utilisez plutôt `app.on_error`. Voir [Gestion des erreurs](errors.md).

## Organiser ses routes

Quand l'application grandit, regroupez les routes liées dans un `Router` et montez-le sous un préfixe, comme avec `express.Router()` :

```rust
let mut api = Router::new();
api.get("/users", list_users).post("/users", create_user);

app.mount("/api", api); // GET /api/users, POST /api/users
```

Les routeurs peuvent avoir leurs propres middlewares et s'imbriquer, voir [Routeurs](routers.md). Pour un middleware sur une seule route (`handler.with(auth)`), voir [Middlewares](middleware.md).
