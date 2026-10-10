# Routers

Un `Router` agrupa rutas relacionadas, con sus propios middlewares, y se monta en la aplicación bajo un prefijo: es el equivalente de `express.Router()`. Los routers mantienen ordenado un proyecto que crece, con un módulo por recurso.

## Crear y montar un router

```js
const users = express.Router();
users.get('/', listUsers);
users.get('/:id', showUser);
app.use('/users', users);
```

```rust
use vitesse::prelude::*;

let mut users = Router::new();
users.get("/", |_| async { "lista de usuarios" });
users.get("/:id", |req: Request| async move {
    format!("usuario {}", req.param("id").unwrap_or("?"))
});

let mut app = App::new();
app.mount("/users", users); // GET /users, GET /users/:id
```

Un `Router` tiene los mismos métodos de enrutamiento que `App`: `get`, `post`, `put`, `patch`, `delete`, `head`, `options`, `all`, `route(método, ruta, handler)`, además de `mount`, `static_dir` y `serve_dir`. Igual que en `App`, devuelven un `&mut Router`, así que puedes encadenarlos:

```rust
let mut users = Router::new();
users
    .get("/", list_users)
    .post("/", create_user)
    .get("/:id", show_user)
    .delete("/:id", delete_user);
```

Las rutas se combinan como esperas: la `/` del router se convierte en `/users`, `/:id` en `/users/:id`, y `/users/` equivale a `/users`.

> [!WARNING]
> Declara el router con `let mut users = Router::new();` y luego llama a sus métodos en instrucciones separadas. `let users = Router::new().get(...);` no compila: los métodos devuelven un `&mut Router` prestado de un valor temporal, no el `Router` en sí.

## Los middlewares de un router

`router.middleware(...)` añade un middleware a **todas las rutas de ese router** y, como `router.use()` en Express, a cualquier otra petición bajo su prefijo:

```rust
async fn require_token(req: Request, next: Next) -> Response {
    match req.header("authorization") {
        Some("Bearer secret") => next.run(req).await,
        _ => Error::unauthorized("falta el token").into_response(),
    }
}

let mut admin = Router::new();
admin.middleware(require_token);
admin.get("/stats", |_| async { "estadísticas secretas" });

app.mount("/admin", admin);                         // protegido
app.get("/", |_| async { "página de inicio pública" }); // no afectado
```

Para una petición, el orden es: middlewares globales (`app.middleware`), luego los middlewares del router en el orden en que se añadieron, luego los de la ruta (`.with`) y por último el handler. El middleware de un router se aplica a todas sus rutas, se haya añadido antes o después de ellas, siempre que se añada antes de `mount`.

### Peticiones a las que no responde ninguna ruta

Los middlewares de un router también se ejecutan para las peticiones bajo su prefijo a las que no responde ninguna de sus rutas: el `404` (o tu `app.fallback`), el `405 Method Not Allowed` y la respuesta automática a `OPTIONS` (`204` con `Allow`). En el ejemplo anterior, `GET /admin/desconocido` también pasa por `require_token`: sin token, el visitante recibe un `401` y ni siquiera sabe si la página existe.

Gracias a esto, un logger o CORS en un router funcionan como cabe esperar:

```rust
let mut api = Router::new();
api.middleware(middleware::logger()); // registra también los 404 bajo /api
api.middleware(middleware::cors());   // responde también a las peticiones preliminares de /api/users
api.post("/users", |_| async { (201, "creado") });

app.mount("/api", api);
```

Antes de un `POST /api/users` desde otro origen, el navegador envía una petición preliminar `OPTIONS /api/users`: `cors()` la responde con un `204` y las cabeceras CORS. Un `GET /api/nope` se registra y luego recibe su `404`.

Algunos detalles:

- El prefijo se compara segmento a segmento: un router montado en `/api` cubre `/api` y `/api/...`, pero no `/apix`.
- Los middlewares globales (`app.middleware`) siguen ejecutándose primero, para todas las peticiones. Con routers anidados, vienen después los middlewares del router exterior y luego los del interior.
- Si un middleware del router reescribe la ruta de una de estas peticiones con `req.set_uri(...)`, el enrutamiento se repite con la nueva ruta.
- Un prefijo puede contener parámetros: un router montado en `/users/:user_id/posts` cubre `/users/42/posts/...`, sea cual sea el valor de `:user_id`.

## Anidar routers

Un router puede montar otros routers. Los middlewares del router padre envuelven también las rutas de sus hijos:

```rust
let mut users = Router::new();
users.get("/", |_| async { "usuarios" });

let mut posts = Router::new();
posts.get("/", |_| async { "artículos" });

let mut api = Router::new();
api.middleware(require_token); // protege también /users y /posts
api.mount("/users", users);
api.mount("/posts", posts);

app.mount("/api/v1", api); // GET /api/v1/users, GET /api/v1/posts
```

## Parámetros en el prefijo

Un prefijo de montaje puede contener parámetros, que los handlers leen como cualquier otro:

```rust
let mut posts = Router::new();
posts.get("/", |req: Request| async move {
    format!("artículos del usuario {}", req.param("user_id").unwrap_or("?"))
});

app.mount("/users/:user_id/posts", posts); // GET /users/42/posts
```

## Cómo funciona el montaje

`mount` copia las rutas del router en el árbol de enrutamiento de la aplicación, una sola vez, al arrancar. Algunas consecuencias:

- **Coste cero por petición**: una ruta montada es exactamente igual de rápida que una ruta declarada directamente en `App`.
- **`mount` consume el router**. Para montar dos veces las mismas rutas (`/api/v1` y `/api/latest`, por ejemplo), construye el router en una función y llámala dos veces.
- **Los conflictos se detectan al arrancar**: si el mismo método y la misma ruta se definen dos veces, incluso en routers distintos, el programa entra en pánico nada más iniciarse, con un mensaje como `duplicate route: GET /api/x is already defined`.
- **La prioridad de las rutas** es global: un segmento estático gana a un parámetro, que gana a un comodín (`/users/new` antes que `/users/:id`), venga del router que venga.
- El handler de respaldo (`app.fallback`), el manejador de errores (`app.on_error`) y el estado (`app.state`) pertenecen a la aplicación, no a los routers.

## Organizar un proyecto

Los routers facilitan dividir una aplicación en módulos, uno por recurso. Una organización típica:

```text
mi-app/
├── Cargo.toml
└── src/
    ├── main.rs        // construye la App y arranca el servidor
    └── routes/
        ├── mod.rs     // ensambla la API
        ├── users.rs   // /api/users
        └── posts.rs   // /api/users/:user_id/posts
```

Cada módulo expone una función que devuelve su `Router` (`posts.rs` sigue el mismo patrón que `users.rs`):

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
    (StatusCode::CREATED, "creado")
}

async fn show(req: Request) -> vitesse::Result<String> {
    let id: u32 = req.param_as("id")?;
    Ok(format!("usuario {id}"))
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

Mantener una función `build_app()` separada de `main` permite que tus tests construyan exactamente la misma aplicación y la llamen sin red: ver [Tests](testing.md).
