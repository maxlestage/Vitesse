# Estado compartido

Configuración, un pool de conexiones a la base de datos, una caché, contadores: algunos datos deben compartirse entre todas las peticiones. En Vitesse los registras una vez con `app.state(valor)` y los lees desde cualquier handler o middleware con `req.state::<T>()`, el equivalente tipado de `app.locals` en Express.

## Registrar y leer el estado

```rust
use vitesse::prelude::*;

struct Config {
    app_name: String,
    max_items: usize,
}

let mut app = App::new();
app.state(Config { app_name: "Mi API".into(), max_items: 50 });

app.get("/", |req: Request| async move {
    let config = req.state::<Config>();
    format!("{} (máx. {} elementos)", config.app_name, config.max_items)
});
```

El estado se busca **por su tipo**: `req.state::<Config>()` devuelve la `Config` que registraste. De ahí salen algunas reglas:

- **Un valor por tipo.** Llamar dos veces a `app.state(...)` con el mismo tipo reemplaza el primer valor. En lugar de registrar un `String` o un `u32` a secas, dale a cada pieza de estado su propio tipo (una estructura, o un *newtype* como `struct ApiKey(String)`), o agrúpalo todo en una estructura `AppState`.
- **El tipo debe ser `Send + Sync + 'static`**, porque lo comparten todos los hilos del servidor.
- **`req.state::<T>()` devuelve un `&'static T`**: sin `Arc`, sin clones, sin bloqueos. Puedes conservar la referencia a través de los `.await`.
- **Olvidar registrarlo** es un error de programación: `req.state::<T>()` entra en pánico, la petición recibe un `500` y los logs del servidor muestran el mensaje «no state of type `mi_app::Config`: call `app.state(...)` at startup». Cuando el estado es opcional, usa `req.try_state::<T>()`, que devuelve un `Option<&T>`.

El estado también está disponible en los middlewares, con el mismo `req.state::<T>()`.

## Estado modificable

El estado lo comparten todos los hilos, así que Rust solo te deja modificarlo a través de tipos pensados para el acceso concurrente («mutabilidad interior»).

| Necesidad | Tipo |
|---|---|
| Un contador, una bandera | `AtomicU64`, `AtomicBool`… (`std::sync::atomic`) |
| Una colección que se lee mucho y se escribe poco | `std::sync::RwLock` |
| Escrituras cortas y sencillas | `std::sync::Mutex` |
| Un bloqueo mantenido durante un `.await` | `tokio::sync::Mutex` o `tokio::sync::RwLock` |

### Atómicos

La forma más rápida de contar:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

app.state(AtomicU64::new(0));
app.get("/visits", |req: Request| async move {
    let n = req.state::<AtomicU64>().fetch_add(1, Ordering::Relaxed) + 1;
    format!("visita n.º {n}")
});
```

### `Mutex` y `RwLock`

Un almacén en memoria, compartido por todas las peticiones:

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
> Nunca mantengas el guard de un `std::sync::Mutex` o de un `RwLock` durante un `.await`: el handler dejaría de compilar (`future cannot be sent between threads safely`), y es mejor así, porque el bloqueo frenaría a las demás peticiones. Lee el cuerpo (`req.json().await?`) *antes* de tomar el bloqueo, como arriba. Si de verdad necesitas mantener un bloqueo mientras esperas, usa `tokio::sync::Mutex`, cuyo `lock()` también se espera: `let mut guard = req.state::<MyState>().items.lock().await;`.

## Una estructura para toda la aplicación

En una aplicación real, agrúpalo todo en una sola estructura: una sola llamada a `app.state(...)` y un solo tipo que pedir.

```rust
struct AppState {
    config: Config,
    visits: AtomicU64,
    store: Store,
}

app.state(AppState {
    config: Config { app_name: "Mi API".into(), max_items: 50 },
    visits: AtomicU64::new(0),
    store: Store::default(),
});

app.get("/about", |req: Request| async move {
    let state = req.state::<AppState>();
    state.visits.fetch_add(1, Ordering::Relaxed);
    state.config.app_name.clone()
});
```

## Compartir el estado fuera del servidor

`app.state(valor)` mueve el valor dentro de la aplicación. Si otro código también lo necesita (una tarea en segundo plano, por ejemplo), envuélvelo en un `Arc`, registra un clon y léelo con `req.state::<Arc<T>>()`:

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

    // Una tarea en segundo plano que lee el mismo estado.
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            println!("{} peticiones hasta ahora", stats.requests.load(Ordering::Relaxed));
        }
    });

    app.listen(3000).await
}
```

Aquí el servidor arranca con `app.listen(...).await` dentro de `#[tokio::main]` para que la tarea en segundo plano y el servidor compartan el mismo runtime de tokio (ver [Servidor](server.md)).

También puedes capturar valores en una closure, como harías en JavaScript. Un handler puede llamarse muchas veces, así que clona lo que capturas para cada petición:

```rust
let greeting = Arc::new(String::from("Hola"));
app.get("/hello", move |_| {
    let greeting = greeting.clone();
    async move { format!("¡{greeting}!") }
});
```

## Pools de conexiones a la base de datos

Un pool de conexiones es el ejemplo típico de estado: créalo al arrancar, regístralo y úsalo desde los handlers. Aquí tienes una ilustración con [sqlx](https://docs.rs/sqlx) y PostgreSQL (sqlx es un crate externo, y este fragmento es un esbozo que debes adaptar, no código probado); el patrón es el mismo con deadpool, diesel-async o un cliente de Redis.

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
            .await?; // un error de la base de datos se convierte en un 500
        let name = name.ok_or_else(|| Error::not_found("usuario no encontrado"))?;
        Ok::<_, Error>(Json(json!({ "id": id, "name": name })))
    });

    app.listen(3000).await?;
    Ok(())
}
```

> [!TIP]
> Crear un pool es asíncrono: hazlo dentro de `#[tokio::main]` y luego arranca el servidor con `app.listen(...).await` en ese mismo runtime, en lugar de `app.run(...)`, que crea sus propios runtimes.

## Datos propios de una petición

`app.state` lo comparten *todas* las peticiones. Para un dato que pertenece a *una sola* petición (el usuario autenticado, un identificador de petición…), un middleware lo adjunta con `req.set(valor)` y los handlers lo leen con `req.get::<T>()`, el equivalente de `res.locals` o `req.user` en Express:

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
        Some(user) => format!("usuario {}", user.id),
        None => "anónimo".to_string(),
    }
});
```

Los valores se guardan por tipo, como el estado, y deben implementar `Clone + Send + Sync`. Para un acceso de más bajo nivel, `req.extensions()` y `req.extensions_mut()` exponen las `http::Extensions` subyacentes.

| | `app.state(valor)` | `req.set(valor)` |
|---|---|---|
| Duración | toda la aplicación | una petición |
| Lo define | tu código de arranque | un middleware |
| Se lee con | `req.state::<T>()` / `req.try_state::<T>()` | `req.get::<T>()` |
| Equivalente en Express | `app.locals` | `res.locals`, `req.user` |
