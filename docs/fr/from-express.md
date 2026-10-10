# Venir d'Express

Si vous connaissez Express, vous connaissez déjà l'essentiel de Vitesse : les mêmes routes, le même `req`, les mêmes middlewares avec `next`, les mêmes routeurs. Ce guide fait correspondre chaque notion d'Express à son équivalent dans Vitesse, montre la même API CRUD écrite avec les deux, et liste les différences qui surprennent souvent au début.

## Cinq idées à garder en tête

1. **Un handler renvoie sa réponse.** Il n'y a pas d'objet `res` à modifier : vous renvoyez une chaîne, un `Json(...)`, un code de statut ou une `Response` que vous avez construite.
2. **Tout est asynchrone.** Un handler est une fonction `async` (ou une closure `async`) qui reçoit la `Request`.
3. **Les données sont typées.** Les corps JSON et les query strings sont désérialisés dans des structures Rust avec [serde](https://serde.rs) ; une entrée invalide est rejetée avec une `400` avant même que votre code ne s'exécute.
4. **Les erreurs remontent avec `?`.** Un handler peut renvoyer un `Result` : `?` s'arrête à la première erreur et la transforme en réponse HTTP.
5. **Les erreurs de programmation sont détectées tôt.** Le compilateur vérifie les types, et une route invalide ou en double arrête le programme au démarrage, en indiquant la ligne fautive.

## Notion par notion

### Application et serveur

| Express | Vitesse |
|---|---|
| `const app = express()` | `let mut app = App::new();` |
| `app.listen(3000, callback)` | `app.run(3000)` (bloquant : affichez votre message avant) |
| `app.listen(3000)` dans un code déjà asynchrone | `app.listen(3000).await` |
| `process.env.PORT` | `std::env::var("PORT")` |
| `app.locals.db = db` | `app.state(db)`, lu avec `req.state::<Db>()` |
| `app.use((req, res) => res.status(404)…)` | `app.fallback(handler)` |

### Routage

| Express | Vitesse |
|---|---|
| `app.get('/users/:id', handler)` | `app.get("/users/:id", handler)` |
| `app.post` / `put` / `patch` / `delete` / `all` | Mêmes noms |
| `app.route('/todos').get(a).post(b)` | `app.get("/todos", a).post("/todos", b)` (les appels s'enchaînent) |
| Toute autre méthode | `app.route(Method::from_bytes(b"PURGE").unwrap(), "/cache", handler)` |
| `/files/*path` (Express 5) | `/files/*path`, lu avec `req.param("path")` |
| `express.Router()` | `Router::new()` |
| `app.use('/api', router)` | `app.mount("/api", router)` |
| `router.use(mw)` | `router.middleware(mw)`, qui s'exécute aussi pour les `404` sous le préfixe du routeur |
| `app.ws('/echo', (ws, req) => …)` (express-ws) | `app.ws("/echo", \|req, socket\| async move { … })` : la requête vient en premier, et une boucle remplace `ws.on('message')` (voir [WebSocket](websocket.md)) |

### Requête

| Express | Vitesse |
|---|---|
| `req.params.id` | `req.param("id")` (une `Option<&str>`), ou `req.param_as::<u64>("id")?` |
| `req.query.page` | `req.query("page")`, ou une structure entière avec `req.query_as::<T>()?` |
| `express.json()` + `req.body` | `req.json::<T>().await?` |
| `express.urlencoded()` + `req.body` | `req.form::<T>().await?` |
| `express.text()` / `express.raw()` | `req.text().await?` / `req.bytes().await?` |
| `req.get('host')` | `req.header("host")` |
| `req.cookies.session` (cookie-parser) | `req.cookie("session")`, intégré |
| `req.ip` / `req.hostname` | `req.ip()` / `req.hostname()` |
| `req.method` / `req.path` / `req.originalUrl` | `req.method()` / `req.path()` / `req.uri()` |
| `req.is('json')` | `req.is("json")` |
| `res.locals.user = user` | `req.set(user)`, lu avec `req.get::<User>()` |

### Réponse

| Express | Vitesse |
|---|---|
| `res.send('texte')` | Renvoyer `"texte"` (ou une `String`) |
| `res.json(obj)` | Renvoyer `Json(obj)` ou `json!({ ... })` |
| `res.status(201).json(obj)` | Renvoyer `(201, Json(obj))` ou `res::status(201).json(obj)` |
| `res.sendStatus(204)` | Renvoyer `StatusCode::NO_CONTENT` |
| `res.redirect('/login')` | Renvoyer `Redirect::to("/login")` (302) |
| `res.redirect(301, '/new')` | Renvoyer `Redirect::permanent("/new")` |
| `res.set('X-Foo', 'bar')` | `Response::new().header("x-foo", "bar")` |
| `res.type('text/csv')` | `.content_type("text/csv")` |
| `res.cookie('theme', 'dark', { httpOnly: true })` | `.cookie(Cookie::new("theme", "dark").http_only(true))` |
| `res.clearCookie('theme')` | `.clear_cookie("theme")` |
| `res.attachment('rapport.csv')` | `.attachment("rapport.csv")` |
| `res.sendFile(path)` | `res::file(path).await` |
| `res.download(path, 'rapport.pdf')` | `res::download(path, "rapport.pdf").await` |

### Middlewares et erreurs

| Express | Vitesse |
|---|---|
| `app.use(fn)` | `app.middleware(fn)` |
| `function mw(req, res, next)` | `async fn mw(req: Request, next: Next) -> Response` |
| `next()` | `next.run(req).await` |
| `app.get('/admin', auth, handler)` | `app.get("/admin", handler.with(auth))` |
| `next(err)` / `throw err` | `return Err(Error::bad_request("…"))`, ou `?` |
| `app.use((err, req, res, next) => …)` | `app.on_error(f)`, avec `f` une fonction `Fn(Error) -> impl IntoResponse` |
| `morgan('dev')` | `middleware::logger()` |
| `cors()` | `middleware::cors()` |
| `helmet()` | `middleware::helmet()` |
| `connect-timeout` | `middleware::timeout(durée)` |
| `express.static('public')` | `app.middleware(ServeDir::new("public"))` |
| `app.use('/static', express.static('public'))` | `app.static_dir("/static", "public")` |

### Tests

| supertest | Vitesse |
|---|---|
| `request(app).get('/')` | `TestClient::new(app()).get("/").await` |
| `.set('authorization', 'Bearer x')` | `.header("authorization", "Bearer x")` |
| `.send({ title: 'x' })` | `.json(&json!({ "title": "x" }))` |
| `expect(res.status).toBe(200)` | `assert_eq!(res.status(), 200)` |
| `res.body` | `res.json::<T>()` |

## Une API CRUD, côte à côte

La même API de tâches en mémoire, d'abord avec Express :

```js
const express = require('express');

const app = express();
app.use(express.json());

const todos = new Map();
let nextId = 0;

app.get('/todos', (req, res) => {
  res.json([...todos.values()]);
});

app.get('/todos/:id', (req, res) => {
  const todo = todos.get(Number(req.params.id));
  if (!todo) return res.status(404).json({ error: 'tâche introuvable' });
  res.json(todo);
});

app.post('/todos', (req, res) => {
  const { title } = req.body ?? {};
  if (!title || !title.trim()) {
    return res.status(422).json({ error: 'le titre est obligatoire' });
  }
  const todo = { id: ++nextId, title, done: false };
  todos.set(todo.id, todo);
  res.status(201).json(todo);
});

app.patch('/todos/:id', (req, res) => {
  const todo = todos.get(Number(req.params.id));
  if (!todo) return res.status(404).json({ error: 'tâche introuvable' });
  const { title, done } = req.body ?? {};
  if (title !== undefined) todo.title = title;
  if (done !== undefined) todo.done = done;
  res.json(todo);
});

app.delete('/todos/:id', (req, res) => {
  if (!todos.delete(Number(req.params.id))) {
    return res.status(404).json({ error: 'tâche introuvable' });
  }
  res.sendStatus(204);
});

app.listen(3000, () => console.log('Écoute sur http://localhost:3000'));
```

Puis avec Vitesse :

```rust
use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use vitesse::prelude::*;

#[derive(Clone, Serialize)]
struct Todo {
    id: u64,
    title: String,
    done: bool,
}

#[derive(Deserialize)]
struct NewTodo {
    title: String,
}

#[derive(Deserialize)]
struct PatchTodo {
    title: Option<String>,
    done: Option<bool>,
}

/// Le stockage en mémoire, partagé par toutes les requêtes (le `Mutex` le rend sûr entre threads).
#[derive(Default)]
struct Store {
    next_id: u64,
    todos: BTreeMap<u64, Todo>,
}

type Db = Mutex<Store>;

fn not_found() -> Error {
    Error::not_found("tâche introuvable")
}

async fn list(req: Request) -> Json<Vec<Todo>> {
    let store = req.state::<Db>().lock().unwrap();
    Json(store.todos.values().cloned().collect())
}

async fn show(req: Request) -> vitesse::Result<Json<Todo>> {
    let id: u64 = req.param_as("id")?; // 400 si ce n'est pas un nombre
    let store = req.state::<Db>().lock().unwrap();
    let todo = store.todos.get(&id).ok_or_else(not_found)?;
    Ok(Json(todo.clone()))
}

async fn create(req: Request) -> vitesse::Result<(StatusCode, Json<Todo>)> {
    let new: NewTodo = req.json().await?; // 400 si le JSON est invalide ou si `title` manque
    if new.title.trim().is_empty() {
        return Err(Error::unprocessable("le titre est obligatoire"));
    }
    let mut store = req.state::<Db>().lock().unwrap();
    store.next_id += 1;
    let todo = Todo { id: store.next_id, title: new.title, done: false };
    store.todos.insert(todo.id, todo.clone());
    Ok((StatusCode::CREATED, Json(todo)))
}

async fn update(req: Request) -> vitesse::Result<Json<Todo>> {
    let id: u64 = req.param_as("id")?;
    let patch: PatchTodo = req.json().await?;
    let mut store = req.state::<Db>().lock().unwrap();
    let todo = store.todos.get_mut(&id).ok_or_else(not_found)?;
    if let Some(title) = patch.title {
        todo.title = title;
    }
    if let Some(done) = patch.done {
        todo.done = done;
    }
    Ok(Json(todo.clone()))
}

async fn remove(req: Request) -> vitesse::Result<StatusCode> {
    let id: u64 = req.param_as("id")?;
    let mut store = req.state::<Db>().lock().unwrap();
    store.todos.remove(&id).ok_or_else(not_found)?;
    Ok(StatusCode::NO_CONTENT)
}

fn app() -> App {
    let mut app = App::new();
    app.state(Db::default());

    app.get("/todos", list)
        .post("/todos", create)
        .get("/todos/:id", show)
        .patch("/todos/:id", update)
        .delete("/todos/:id", remove);

    app
}

fn main() -> std::io::Result<()> {
    println!("Écoute sur http://localhost:3000");
    app().run(3000)
}
```

Ce qui a changé :

- **La forme des données est déclarée une fois.** `NewTodo` indique que `title` est une chaîne obligatoire : `{}` ou `{"title": 42}` est rejeté avec une `400` et un message qui explique le problème. En Express, cette validation est à votre charge.
- **Les paramètres sont analysés, pas seulement lus.** `Number('abc')` donne silencieusement `NaN` (et ici une 404) ; `req.param_as::<u64>("id")?` répond tout de suite `400`.
- **« Introuvable » est une valeur.** `ok_or_else(not_found)?` transforme une entrée manquante en `404` et quitte la fonction, à la place d'un `return res.status(404)…` anticipé.
- **La `Map` globale devient un état.** Les requêtes s'exécutent en parallèle sur plusieurs threads : le stockage est donc enregistré avec `app.state` et protégé par un `Mutex`.
- **Les statuts sont dans le type de retour.** `(StatusCode::CREATED, Json(todo))` pour une 201, `StatusCode::NO_CONTENT` pour une 204 : impossible d'oublier d'envoyer une réponse.

## Middlewares, côte à côte

```js
// Journalise chaque requête avec sa durée.
app.use((req, res, next) => {
  const start = Date.now();
  res.on('finish', () => console.log(`${req.method} ${req.path} ${Date.now() - start} ms`));
  next();
});

// Protège une route.
function auth(req, res, next) {
  if (req.get('authorization') !== 'Bearer secret') {
    return res.status(401).json({ error: 'connectez-vous' });
  }
  res.locals.user = { name: 'ada' };
  next();
}

app.get('/admin', auth, (req, res) => res.send(`Bienvenue, ${res.locals.user.name}`));
```

```rust
use std::time::Instant;
use vitesse::prelude::*;

#[derive(Clone)]
struct User {
    name: String,
}

async fn auth(mut req: Request, next: Next) -> Response {
    if req.header("authorization") != Some("Bearer secret") {
        return Error::unauthorized("connectez-vous").into_response();
    }
    req.set(User { name: "ada".into() }); // res.locals.user = …
    next.run(req).await                    // next()
}

async fn admin(req: Request) -> String {
    let user = req.get::<User>().unwrap(); // posé par `auth`
    format!("Bienvenue, {}", user.name)
}

fn app() -> App {
    let mut app = App::new();

    // Journalise chaque requête avec sa durée.
    app.middleware(|req: Request, next: Next| async move {
        let start = Instant::now();
        let line = format!("{} {}", req.method(), req.path());
        let res = next.run(req).await;
        println!("{line} {:?}", start.elapsed());
        res
    });

    // Protège une route.
    app.get("/admin", admin.with(auth));
    app
}
```

Pas besoin de `res.on('finish')` : le code placé après `next.run(req).await` s'exécute une fois que la suite de la chaîne a produit la réponse, et il peut même la modifier (`res.header(...)`) avant de la renvoyer. Un middleware qui veut arrêter la requête renvoie simplement une réponse sans appeler `next`. Voir [Middlewares](middleware.md).

## Erreurs, côte à côte

```js
app.get('/users/:id', async (req, res) => {
  const user = await findUser(req.params.id);
  if (!user) return res.status(404).json({ error: 'utilisateur introuvable' });
  res.json(user);
});

app.use((err, req, res, next) => {
  console.error(err);
  res.status(err.status || 500).json({ message: err.message });
});
```

```rust
// `find_user` est votre appel à la base de données : il renvoie `Result<Option<User>, _>`.
async fn show_user(req: Request) -> vitesse::Result<Json<User>> {
    let id: u64 = req.param_as("id")?;
    let user = find_user(id).await?.ok_or_else(|| Error::not_found("utilisateur introuvable"))?;
    Ok(Json(user))
}

app.get("/users/:id", show_user);

app.on_error(|err: Error| {
    res::status(err.status()).json(json!({ "message": err.message() }))
});
```

`Error` porte un statut HTTP et un message destiné au client. Toute autre erreur (base de données, entrées-sorties, parsing…) se convertit automatiquement avec `?` en une `500` dont le détail est seulement journalisé, jamais envoyé au client. `app.on_error` s'applique à toutes les réponses d'erreur, quel que soit l'endroit où il est enregistré : vos erreurs, les 404, les 405, les corps invalides et les paniques. Voir [Erreurs](errors.md).

## Tests, côte à côte

```js
const request = require('supertest');
const app = require('./app');

test('crée une tâche', async () => {
  const res = await request(app).post('/todos').send({ title: 'Apprendre Rust' });
  expect(res.status).toBe(201);
  expect(res.body.title).toBe('Apprendre Rust');
});
```

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use vitesse::serde_json::Value;
    use vitesse::test::TestClient;

    #[tokio::test]
    async fn cree_une_tache() {
        let client = TestClient::new(app());

        let res = client.post("/todos").json(&json!({ "title": "Apprendre Rust" })).await;

        assert_eq!(res.status(), 201);
        let body: Value = res.json();
        assert_eq!(body["title"], "Apprendre Rust");
    }
}
```

`TestClient` appelle l'application en mémoire, sans ouvrir de port. Tout est expliqué dans [Tests](testing.md).

## Les différences clés

### `async move` et les closures

```rust
app.get("/", |_| async { "Bonjour !" });                // requête inutilisée
app.get("/hello/:name", |req: Request| async move {     // requête utilisée
    format!("Bonjour {} !", req.param("name").unwrap_or("inconnu"))
});
```

Une closure reçoit la requête et renvoie un bloc `async`. Quand le bloc utilise `req`, écrivez `async move` pour qu'il prenne possession de la requête, et précisez le type (`req: Request`) pour que le compilateur le connaisse. Quand un handler grossit, une `async fn` nommée est souvent plus lisible qu'une closure.

### Du JSON typé avec serde

Dérivez `Deserialize` pour ce que vous recevez et `Serialize` pour ce que vous envoyez. Les champs facultatifs sont des `Option<T>` ; les champs inconnus sont ignorés par défaut. Quand la structure n'est pas connue à l'avance, utilisez `serde_json::Value`, l'équivalent d'un simple objet JavaScript (`vitesse::serde_json` est réexporté).

### Les erreurs avec `?`

`?` remplace à la fois `try/catch` et `next(err)`. Les fonctions de Vitesse qui peuvent échouer (`req.json()`, `req.param_as()`, `req.query_as()`…) renvoient déjà une `vitesse::Error` avec le bon statut. Dans une closure qui utilise `?`, indiquez le type d'erreur au compilateur avec `Ok::<_, Error>(valeur)` comme dernière expression, ou utilisez une fonction nommée qui renvoie `vitesse::Result<T>`.

### Vérifié à la compilation, et au démarrage

Un handler qui renvoie autre chose qu'une réponse, une faute de frappe dans un nom de méthode, un `.await` oublié : le compilateur refuse de construire le programme. Les routes sont vérifiées dès leur ajout : un motif invalide ou une même route définie deux fois fait paniquer le programme au démarrage, avec la ligne fautive.

### L'état partagé doit être sûr entre threads

Les requêtes sont traitées en parallèle sur plusieurs threads : pas de variables globales modifiables. Enregistrez vos données avec `app.state(...)` et protégez ce qui change avec un atomique, un `Mutex` ou un `RwLock`. Lire l'état ne coûte rien (`req.state::<T>()` renvoie une simple référence). Voir [État partagé](state.md).

## Les pièges

- **Les middlewares globaux s'exécutent avant le routage, pour chaque requête.** Que `app.middleware(...)` soit appelé avant ou après vos routes ne change rien : tous les middlewares globaux s'exécutent, dans l'ordre d'ajout, avant le routage, 404 comprises. Pour ne cibler que certaines routes, utilisez un [routeur](routers.md) (dont les middlewares couvrent tout le préfixe, comme `router.use`) ou `handler.with(mw)`.
- **Les routes sont sensibles à la casse.** `/Users` ne correspond pas à `/users` (Express ignore la casse par défaut). Une barre oblique finale est ignorée : `/users/` correspond à `/users`, comme en Express.
- **Une mauvaise méthode répond `405`, pas `404`,** avec un en-tête `Allow`. `HEAD` utilise la route `GET` et `OPTIONS` répond `204` automatiquement.
- **Un seul handler par méthode et par chemin.** Définir deux fois la même route fait paniquer au démarrage ; l'habitude d'Express d'enchaîner plusieurs handlers sur une même route avec `next()` devient un middleware.
- **`req.json()` ne vérifie pas le `Content-Type`.** `express.json()` ignore les corps qui ne sont pas du JSON ; Vitesse analyse ce qu'il reçoit. Pour exiger l'en-tête, vérifiez `req.is("json")` et renvoyez une `415`.
- **Les query strings sont plates.** `?user[name]=ada` ne devient pas un objet imbriqué. Pour une clé répétée comme `?tag=a&tag=b`, parcourez `req.query_pairs()`.
- **Pas de paramètres facultatifs ni d'expressions régulières,** ni de paramètres partiels dans un segment (`/:id?`, `/:id(\d+)`, `/vols/:de-:vers`) : déclarez des routes distinctes, ou analysez le segment vous-même.
- **Les erreurs sont en JSON par défaut** (`{"error": "..."}`), et les détails internes n'atteignent jamais le client, même en développement. Utilisez `app.on_error` pour un autre format.
- **`req.set(valeur)` exige `Clone`.** Ajoutez `#[derive(Clone)]` aux types que vous attachez à une requête.
- **Le corps est lu à la demande, et une seule fois depuis le réseau.** Le résultat est mis en cache : appeler `req.text()` puis `req.bytes()` fonctionne.
- **Recharger à chaque modification.** `cargo run` recompile ce qui a changé ; pour une boucle à la `nodemon`, utilisez un outil comme [cargo-watch](https://github.com/watchexec/cargo-watch) (`cargo watch -x run`). Compilez avec `cargo build --release` pour la production.

Pour aller plus loin, l'[aide-mémoire](cheatsheet.md) liste toute l'API sur une seule page.
