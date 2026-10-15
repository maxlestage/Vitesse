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

Un `Router` tiene los mismos métodos de enrutamiento que `App`: `get`, `post`, `put`, `patch`, `delete`, `head`, `options`, `all`, `route(método, ruta, handler)`, además de `ws`, `mount`, `static_dir` y `serve_dir`. Igual que en `App`, devuelven un `&mut Router`, así que puedes encadenarlos:

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

Los middlewares de un router también se ejecutan para las peticiones bajo su prefijo a las que no responde ninguna de sus rutas: el `404` (el [fallback](#el-fallback-de-un-router) propio del router si lo tiene, o si no el de la aplicación), el `405 Method Not Allowed` y la respuesta automática a `OPTIONS` (`204` con `Allow`). En el ejemplo anterior, `GET /admin/desconocido` también pasa por `require_token`: sin token, el visitante recibe un `401` y ni siquiera sabe si la página existe.

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
- Un prefijo puede contener parámetros: un router montado en `/users/:user_id/posts` cubre `/users/42/posts/...`, sea cual sea el valor de `:user_id`. Las restricciones del prefijo también se comprueban para estas peticiones: un router montado en `/docs/:lang(en|fr|es)` no cubre `/docs/de/...`, cuyo `404` viene de la aplicación, sin los middlewares ni el fallback del router.

## El fallback de un router

Por defecto, una ruta desconocida bajo el prefijo de un router recibe el `404` de la aplicación: tu `app.fallback`, o el `{"error":"Cannot GET /nope"}` por defecto. `router.fallback(handler)` responde a esas peticiones en su lugar. Así, una API puede mantener sus errores en JSON mientras el resto del sitio muestra una página HTML:

```rust
let mut api = Router::new();
api.get("/users", |_| async { Json(vec!["ada", "grace"]) });
api.fallback(|req: Request| async move {
    (404, Json(json!({ "error": format!("ninguna ruta de la API para {}", req.path()) })))
});

let mut app = App::new();
app.mount("/api", api);
app.fallback(|_| async { (404, Html("<h1>Página no encontrada</h1>")) });
```

| Petición | Respuesta |
|---|---|
| `GET /api/users` | la ruta |
| `GET /api/nope` | el fallback del router: `404 {"error":"ninguna ruta de la API para /api/nope"}` |
| `POST /api/users` | `405 Method Not Allowed`: la ruta existe, así que no se llama a ningún fallback |
| `GET /nope` | el fallback de la aplicación: la página HTML |

- El fallback se ejecuta después de los middlewares del router, como una de sus rutas: un router protegido por autenticación protege también su fallback, y las cabeceras que añaden sus middlewares aparecen en las respuestas del fallback.
- Con routers anidados, gana el router más profundo que tenga fallback: un fallback en el router montado en `/api/admin` responde a `/api/admin/nope`, y el de `/api`, al resto de `/api`. Un router sin fallback usa el de su padre y, en último término, el de la aplicación.
- Como `app.fallback`, puede responder cualquier cosa, no solo un `404`: una redirección, una página HTML o el `index.html` de una aplicación de una sola página que vive bajo el prefijo.

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

## Grupos de rutas: `app.group`

`app.group(prefijo, |grupo| { … })` crea un router, te deja llenarlo y lo monta: el `Route::prefix()->group()` de Laravel, el `app.route()` de Hono. Es la forma más corta de dar a unas cuantas rutas un prefijo y middlewares propios:

```rust
app.group("/admin", |admin| {
    admin.middleware(middleware::basic_auth(("admin", "s3cret")));
    admin.get("/stats", stats);        // GET /admin/stats, protegida
    admin.group("/users", |users| {    // los grupos se anidan
        users.get("/:id", mostrar);    // GET /admin/users/:id, también protegida
    });
});
```

La closure recibe un `&mut Router`: una función que lo reciba también sirve, y un módulo puede exportar sus rutas como una sola función, montable bajo cualquier prefijo.

```rust
// src/routes/users.rs
pub fn routes(r: &mut Router) {
    r.get("/", listar).post("/", crear).get("/:id", mostrar);
}

// src/main.rs
app.group("/api/users", routes::users::routes);
```

Todo lo que esta página dice de los routers vale para los grupos: middlewares que cubren todo el prefijo, fallback, nombres, etc.

## Parámetros en el prefijo

Un prefijo de montaje puede contener parámetros, que los handlers leen como cualquier otro:

```rust
let mut posts = Router::new();
posts.get("/", |req: Request| async move {
    format!("artículos del usuario {}", req.param("user_id").unwrap_or("?"))
});

app.mount("/users/:user_id/posts", posts); // GET /users/42/posts
```

Todo lo que ofrece la [sintaxis de las rutas](routing.md#parámetros-de-ruta) funciona también en un prefijo: varios parámetros en un segmento (`/api/v:version`), [restricciones](routing.md#restricciones-con-expresiones-regulares) y [parámetros opcionales](routing.md#parámetros-opcionales). Aquí, un solo router sirve la documentación en tres idiomas, con una página de inicio por idioma:

```rust
let mut docs = Router::new();
docs.get("/:page?", |req: Request| async move {
    let lang = req.param("lang").unwrap_or("en");
    let page = req.param("page").unwrap_or("index");
    format!("página {page} en {lang}")
});

app.mount("/docs/:lang(en|fr|es)", docs);
```

| Petición | Respuesta |
|---|---|
| `GET /docs/fr` | `página index en fr` |
| `GET /docs/es/routing` | `página routing en es` |
| `GET /docs/de/routing` | `404`: `de` no cumple la restricción |

`req.route()` da el patrón completo, prefijo incluido: `/docs/:lang(en|fr|es)/:page?`. Los parámetros opcionales tienen que quedar al final de la ruta completa, prefijo y ruta incluidos: con un prefijo como `/:lang?`, cualquier ruta que no sea la `/` del router hace que `mount` entre en pánico. Pon el parámetro opcional en las rutas del router, como arriba.

## Rutas WebSocket

`router.ws(ruta, handler)` declara una ruta [WebSocket](websocket.md), exactamente igual que `app.ws`. Se aplica el prefijo del router, y sus middlewares se ejecutan sobre la petición del handshake, antes de aceptar la conexión: un middleware que responde `401` rechaza la conexión.

```rust
let mut live = Router::new();
live.middleware(require_token); // también comprueba el handshake WebSocket
live.ws("/feed", |_req, mut socket| async move {
    let _ = socket.send("bienvenido al directo").await;
});

app.mount("/live", live); // ws://example.com/live/feed
```

## Rutas con nombre en los routers

Una ruta declarada en un router puede recibir un [nombre](routing.md#rutas-con-nombre-y-generación-de-url) con `.name(...)`, exactamente igual que en `App`. El nombre pertenece a toda la aplicación, y la ruta que construye incluye el prefijo de montaje:

```rust
let mut api = Router::new();
api.get(r"/items/:id(\d+)", show_item).name("item");
api.post("/items", |req: Request| async move {
    // Los parámetros del prefijo van primero: ":version", luego ":id".
    let link = req.path_for("item", &[&"v1", &7])?; // "/api/v1/items/7"
    Ok::<_, Error>((201, Json(json!({ "link": link }))))
});

app.mount("/api/:version", api);
```

- Cualquier handler puede usar el nombre, dentro del router o en otra parte de la aplicación: `req.path_for("item", …)` también funciona desde `app.get("/", …)`.
- Los valores siguen el orden de los parámetros en la ruta completa: primero los del prefijo, luego los de la ruta.
- Los nombres tienen que ser únicos en toda la aplicación: dos routers que llaman `"show"` a una ruta hacen que el segundo `mount` entre en pánico al arrancar. Ponles un prefijo (`"users.show"`, `"posts.show"`).
- `name` va detrás de una ruta: llamarlo justo después de `mount` provoca un pánico, ya que `mount` añade rutas pero no es una.

## Cómo funciona el montaje

`mount` copia las rutas del router en el árbol de enrutamiento de la aplicación, una sola vez, al arrancar. Algunas consecuencias:

- **Coste cero por petición**: una ruta montada es exactamente igual de rápida que una ruta declarada directamente en `App`.
- **`mount` consume el router**. Para montar dos veces las mismas rutas (`/api/v1` y `/api/latest`, por ejemplo), construye el router en una función y llámala dos veces.
- **Los conflictos se detectan al arrancar**: si el mismo método y la misma ruta se definen dos veces, incluso en routers distintos, el programa entra en pánico nada más iniciarse, con un mensaje como `duplicate route: GET /api/x is already defined`.
- **La prioridad de las rutas** es global: un segmento estático gana a uno con restricción o compuesto, que gana a un parámetro simple, que gana a un comodín (`/users/new`, luego `/users/:id(\d+)`, luego `/users/:name`), venga del router que venga. Consulta [Cómo se eligen las rutas](routing.md#cómo-se-eligen-las-rutas).
- **Los nombres de las rutas** también son globales: consulta [Rutas con nombre en los routers](#rutas-con-nombre-en-los-routers).
- El manejador de errores (`app.on_error`) y el estado (`app.state`) pertenecen a la aplicación, no a los routers. Un router sí puede tener su propio [fallback](#el-fallback-de-un-router), que responde antes que `app.fallback`.

## Plugins: `app.register`

Un plugin es una pieza de aplicación reutilizable: rutas, middlewares, estado, hooks… añadidos en una llamada, como el `register` de Fastify. Cualquier función que reciba un `&mut App` es un plugin, y una estructura que implemente `vitesse::Plugin` puede llevar opciones:

```rust
use vitesse::Plugin;

fn salud(app: &mut App) {
    app.get("/healthz", |_| async { "ok" });
}

struct Metricas {
    ruta: &'static str,
}

impl Plugin for Metricas {
    fn register(self, app: &mut App) {
        app.state(Contadores::default());
        app.on_response(|res: &mut Response| { /* contar los estados */ });
        app.get(self.ruta, mostrar_metricas);
    }
}

app.register(salud).register(Metricas { ruta: "/metrics" });
```

Un plugin actúa sobre toda la aplicación (middlewares globales, hooks, estado). Para rutas encapsuladas bajo un prefijo, con middlewares propios, usa un [grupo](#grupos-de-rutas-appgroup).

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
