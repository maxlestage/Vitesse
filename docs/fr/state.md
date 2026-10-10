# État partagé

Configuration, pool de connexions à une base de données, cache, compteurs : certaines données doivent être partagées par toutes les requêtes. Avec Vitesse, vous les enregistrez une fois avec `app.state(valeur)` et les lisez depuis n'importe quel handler ou middleware avec `req.state::<T>()`, l'équivalent typé de `app.locals` en Express.

## Enregistrer et lire l'état

```rust
use vitesse::prelude::*;

struct Config {
    app_name: String,
    max_items: usize,
}

let mut app = App::new();
app.state(Config { app_name: "Mon API".into(), max_items: 50 });

app.get("/", |req: Request| async move {
    let config = req.state::<Config>();
    format!("{} ({} éléments max.)", config.app_name, config.max_items)
});
```

L'état est retrouvé **par son type** : `req.state::<Config>()` renvoie la `Config` que vous avez enregistrée. Quelques règles en découlent :

- **Une valeur par type.** Appeler `app.state(...)` deux fois avec le même type remplace la première valeur. Plutôt que d'enregistrer une `String` ou un `u32` nus, donnez à chaque élément d'état son propre type (une structure, ou un *newtype* comme `struct ApiKey(String)`), ou regroupez tout dans une structure `AppState`.
- **Le type doit être `Send + Sync + 'static`**, car il est partagé par tous les threads du serveur.
- **`req.state::<T>()` renvoie un `&'static T`** : ni `Arc`, ni clone, ni verrou. Vous pouvez garder la référence de part et d'autre d'un `.await`.
- **Oublier de l'enregistrer** est une erreur de programmation : `req.state::<T>()` panique, la requête reçoit un `500` et les journaux du serveur affichent le message « no state of type `mon_app::Config`: call `app.state(...)` at startup ». Quand l'état est facultatif, utilisez `req.try_state::<T>()`, qui renvoie une `Option<&T>`.

L'état est aussi accessible dans les middlewares, avec le même `req.state::<T>()`.

## Un état modifiable

L'état est partagé par tous les threads : Rust ne vous laisse donc le modifier qu'au travers de types conçus pour les accès concurrents (la « mutabilité intérieure »).

| Besoin | Type |
|---|---|
| Un compteur, un drapeau | `AtomicU64`, `AtomicBool`… (`std::sync::atomic`) |
| Une collection souvent lue, parfois modifiée | `std::sync::RwLock` |
| Des écritures courtes et simples | `std::sync::Mutex` |
| Un verrou gardé pendant un `.await` | `tokio::sync::Mutex` ou `tokio::sync::RwLock` |

### Les atomiques

Le moyen le plus rapide de compter :

```rust
use std::sync::atomic::{AtomicU64, Ordering};

app.state(AtomicU64::new(0));
app.get("/visits", |req: Request| async move {
    let n = req.state::<AtomicU64>().fetch_add(1, Ordering::Relaxed) + 1;
    format!("visite n°{n}")
});
```

### `Mutex` et `RwLock`

Un stockage en mémoire, partagé par toutes les requêtes :

```rust
use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use vitesse::prelude::*;

#[derive(Clone, Serialize)]
struct Todo {
    id: u64,
    title: String,
}

#[derive(Deserialize)]
struct NewTodo {
    title: String,
}

#[derive(Default)]
struct Store {
    next_id: AtomicU64,
    todos: RwLock<HashMap<u64, Todo>>,
}

async fn create(req: Request) -> vitesse::Result<(StatusCode, Json<Todo>)> {
    let new: NewTodo = req.json().await?;
    let store = req.state::<Store>();
    let id = store.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let todo = Todo { id, title: new.title };
    store.todos.write().unwrap().insert(id, todo.clone());
    Ok((StatusCode::CREATED, Json(todo)))
}

async fn list(req: Request) -> Json<Vec<Todo>> {
    let todos = req.state::<Store>().todos.read().unwrap();
    Json(todos.values().cloned().collect())
}

let mut app = App::new();
app.state(Store::default());
app.post("/todos", create);
app.get("/todos", list);
```

> [!WARNING]
> Ne gardez jamais le verrou d'un `std::sync::Mutex` ou d'un `RwLock` pendant un `.await` : le handler ne compilerait plus (`future cannot be sent between threads safely`), et c'est tant mieux, car le verrou bloquerait les autres requêtes. Lisez le corps (`req.json().await?`) *avant* de prendre le verrou, comme ci-dessus. Si vous devez vraiment garder un verrou pendant une attente, utilisez `tokio::sync::Mutex`, dont le `lock()` s'attend lui-même : `let mut guard = req.state::<MyState>().items.lock().await;`.

## Une structure pour toute l'application

Dans une vraie application, regroupez tout dans une seule structure : un seul appel à `app.state(...)`, et un seul type à demander.

```rust
struct AppState {
    config: Config,
    visits: AtomicU64,
    store: Store,
}

app.state(AppState {
    config: Config { app_name: "Mon API".into(), max_items: 50 },
    visits: AtomicU64::new(0),
    store: Store::default(),
});

app.get("/about", |req: Request| async move {
    let state = req.state::<AppState>();
    state.visits.fetch_add(1, Ordering::Relaxed);
    state.config.app_name.clone()
});
```

## Partager l'état en dehors du serveur

`app.state(valeur)` déplace la valeur dans l'application. Si un autre code en a aussi besoin (une tâche de fond, par exemple), enveloppez-la dans un `Arc`, enregistrez-en un clone, et lisez-la avec `req.state::<Arc<T>>()` :

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use vitesse::prelude::*;

#[derive(Default)]
struct Stats {
    requests: AtomicU64,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let stats = Arc::new(Stats::default());

    let mut app = App::new();
    app.state(stats.clone());
    app.middleware(|req: Request, next: Next| async move {
        req.state::<Arc<Stats>>().requests.fetch_add(1, Ordering::Relaxed);
        next.run(req).await
    });
    app.get("/", |_| async { "ok" });

    // Une tâche de fond qui lit le même état.
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            println!("{} requêtes jusqu'ici", stats.requests.load(Ordering::Relaxed));
        }
    });

    app.listen(3000).await
}
```

Ici, le serveur est démarré avec `app.listen(...).await` dans `#[tokio::main]`, pour que la tâche de fond et le serveur partagent le même runtime tokio (voir [Serveur](server.md)).

Vous pouvez aussi capturer des valeurs dans une closure, comme en JavaScript. Un handler peut être appelé de nombreuses fois : clonez ce que vous capturez pour chaque requête.

```rust
let greeting = Arc::new(String::from("Bonjour"));
app.get("/hello", move |_| {
    let greeting = greeting.clone();
    async move { format!("{greeting} !") }
});
```

## Pools de connexions à une base de données

Un pool de connexions est l'exemple type d'état partagé : créez-le au démarrage, enregistrez-le, et utilisez-le depuis les handlers. Voici une illustration avec [sqlx](https://docs.rs/sqlx) et PostgreSQL (sqlx est une crate externe, et cet extrait est une esquisse à adapter, pas du code testé) ; le principe est le même avec deadpool, diesel-async ou un client Redis.

```toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres"] }
```

```rust
use sqlx::PgPool;
use vitesse::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPool::connect(&std::env::var("DATABASE_URL")?).await?;

    let mut app = App::new();
    app.state(pool);
    app.get("/users/:id", |req: Request| async move {
        let id: i64 = req.param_as("id")?;
        let name: Option<String> = sqlx::query_scalar("SELECT name FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(req.state::<PgPool>())
            .await?; // une erreur de base de données devient un 500
        let name = name.ok_or_else(|| Error::not_found("utilisateur introuvable"))?;
        Ok::<_, Error>(Json(json!({ "id": id, "name": name })))
    });

    app.listen(3000).await?;
    Ok(())
}
```

> [!TIP]
> La création d'un pool est asynchrone : faites-la dans `#[tokio::main]`, puis démarrez le serveur avec `app.listen(...).await` sur ce même runtime plutôt qu'avec `app.run(...)`, qui crée ses propres runtimes.

## Des données propres à une requête

`app.state` est partagé par *toutes* les requêtes. Pour une donnée qui n'appartient qu'à *une* requête (l'utilisateur authentifié, un identifiant de requête…), un middleware l'attache avec `req.set(valeur)` et les handlers la lisent avec `req.get::<T>()`, l'équivalent de `res.locals` ou de `req.user` en Express :

```rust
#[derive(Clone)]
struct CurrentUser {
    id: u64,
}

app.middleware(|mut req: Request, next: Next| async move {
    if let Some(id) = req.header("x-user-id").and_then(|v| v.parse().ok()) {
        req.set(CurrentUser { id });
    }
    next.run(req).await
});

app.get("/me", |req: Request| async move {
    match req.get::<CurrentUser>() {
        Some(user) => format!("utilisateur {}", user.id),
        None => "anonyme".to_string(),
    }
});
```

Les valeurs sont rangées par type, comme l'état, et doivent implémenter `Clone + Send + Sync`. Pour un accès plus bas niveau, `req.extensions()` et `req.extensions_mut()` exposent les `http::Extensions` sous-jacentes.

| | `app.state(valeur)` | `req.set(valeur)` |
|---|---|---|
| Durée de vie | toute l'application | une requête |
| Défini par | votre code de démarrage | un middleware |
| Lu avec | `req.state::<T>()` / `req.try_state::<T>()` | `req.get::<T>()` |
| Équivalent Express | `app.locals` | `res.locals`, `req.user` |
