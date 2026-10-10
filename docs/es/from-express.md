# Viniendo de Express

Si conoces Express, ya conoces casi todo Vitesse: las mismas rutas, el mismo `req`, los mismos middlewares con `next`, los mismos routers. Esta guía relaciona cada concepto de Express con su equivalente en Vitesse, muestra la misma API CRUD escrita con ambos y repasa las diferencias que suelen sorprender al principio.

## Cinco ideas que conviene tener presentes

1. **Un handler devuelve su respuesta.** No hay ningún objeto `res` que modificar: devuelves un texto, un `Json(...)`, un código de estado o una `Response` que has construido.
2. **Todo es asíncrono.** Un handler es una función `async` (o un closure `async`) que recibe la `Request`.
3. **Los datos tienen tipos.** Los cuerpos JSON y las query strings se deserializan en structs de Rust con [serde](https://serde.rs); una entrada no válida se rechaza con un `400` antes incluso de que se ejecute tu código.
4. **Los errores se propagan con `?`.** Un handler puede devolver un `Result`: `?` se detiene en el primer error y lo convierte en una respuesta HTTP.
5. **Los fallos se detectan pronto.** El compilador comprueba los tipos, y una ruta no válida o duplicada detiene el programa al arrancar, señalando la línea culpable.

## Concepto por concepto

### Aplicación y servidor

| Express | Vitesse |
|---|---|
| `const app = express()` | `let mut app = App::new();` |
| `app.listen(3000, callback)` | `app.run(3000)` (bloquea: muestra tu mensaje antes) |
| `app.listen(3000)` dentro de código ya asíncrono | `app.listen(3000).await` |
| `process.env.PORT` | `std::env::var("PORT")` |
| `app.locals.db = db` | `app.state(db)`, que se lee con `req.state::<Db>()` |
| `app.use((req, res) => res.status(404)…)` | `app.fallback(handler)` |

### Enrutamiento

| Express | Vitesse |
|---|---|
| `app.get('/users/:id', handler)` | `app.get("/users/:id", handler)` |
| `app.post` / `put` / `patch` / `delete` / `all` | Los mismos nombres |
| `app.route('/todos').get(a).post(b)` | `app.get("/todos", a).post("/todos", b)` (las llamadas se encadenan) |
| Cualquier otro método | `app.route(Method::from_bytes(b"PURGE").unwrap(), "/cache", handler)` |
| `/files/*path` (Express 5) | `/files/*path`, que se lee con `req.param("path")` |
| `express.Router()` | `Router::new()` |
| `app.use('/api', router)` | `app.mount("/api", router)` |
| `router.use(mw)` | `router.middleware(mw)`, que también se ejecuta para los `404` bajo el prefijo del router |
| `app.ws('/echo', (ws, req) => …)` (express-ws) | `app.ws("/echo", \|req, socket\| async move { … })`: la petición va primero, y un bucle sustituye a `ws.on('message')` (consulta [WebSocket](websocket.md)) |

### Petición

| Express | Vitesse |
|---|---|
| `req.params.id` | `req.param("id")` (un `Option<&str>`), o `req.param_as::<u64>("id")?` |
| `req.query.page` | `req.query("page")`, o un struct completo con `req.query_as::<T>()?` |
| `express.json()` + `req.body` | `req.json::<T>().await?` |
| `express.urlencoded()` + `req.body` | `req.form::<T>().await?` |
| `express.text()` / `express.raw()` | `req.text().await?` / `req.bytes().await?` |
| `req.get('host')` | `req.header("host")` |
| `req.cookies.session` (cookie-parser) | `req.cookie("session")`, integrado |
| `req.ip` / `req.hostname` | `req.ip()` / `req.hostname()` |
| `req.method` / `req.path` / `req.originalUrl` | `req.method()` / `req.path()` / `req.uri()` |
| `req.is('json')` | `req.is("json")` |
| `res.locals.user = user` | `req.set(user)`, que se lee con `req.get::<User>()` |

### Respuesta

| Express | Vitesse |
|---|---|
| `res.send('texto')` | Devolver `"texto"` (o un `String`) |
| `res.json(obj)` | Devolver `Json(obj)` o `json!({ ... })` |
| `res.status(201).json(obj)` | Devolver `(201, Json(obj))` o `res::status(201).json(obj)` |
| `res.sendStatus(204)` | Devolver `StatusCode::NO_CONTENT` |
| `res.redirect('/login')` | Devolver `Redirect::to("/login")` (302) |
| `res.redirect(301, '/new')` | Devolver `Redirect::permanent("/new")` |
| `res.set('X-Foo', 'bar')` | `Response::new().header("x-foo", "bar")` |
| `res.type('text/csv')` | `.content_type("text/csv")` |
| `res.cookie('theme', 'dark', { httpOnly: true })` | `.cookie(Cookie::new("theme", "dark").http_only(true))` |
| `res.clearCookie('theme')` | `.clear_cookie("theme")` |
| `res.attachment('informe.csv')` | `.attachment("informe.csv")` |
| `res.sendFile(path)` | `res::file(path).await` |
| `res.download(path, 'informe.pdf')` | `res::download(path, "informe.pdf").await` |

### Middlewares y errores

| Express | Vitesse |
|---|---|
| `app.use(fn)` | `app.middleware(fn)` |
| `function mw(req, res, next)` | `async fn mw(req: Request, next: Next) -> Response` |
| `next()` | `next.run(req).await` |
| `app.get('/admin', auth, handler)` | `app.get("/admin", handler.with(auth))` |
| `next(err)` / `throw err` | `return Err(Error::bad_request("…"))`, o `?` |
| `app.use((err, req, res, next) => …)` | `app.on_error(f)`, con `f` una función `Fn(Error) -> impl IntoResponse` |
| `morgan('dev')` | `middleware::logger()` |
| `cors()` | `middleware::cors()` |
| `helmet()` | `middleware::helmet()` |
| `connect-timeout` | `middleware::timeout(duración)` |
| `express.static('public')` | `app.middleware(ServeDir::new("public"))` |
| `app.use('/static', express.static('public'))` | `app.static_dir("/static", "public")` |

### Pruebas

| supertest | Vitesse |
|---|---|
| `request(app).get('/')` | `TestClient::new(app()).get("/").await` |
| `.set('authorization', 'Bearer x')` | `.header("authorization", "Bearer x")` |
| `.send({ title: 'x' })` | `.json(&json!({ "title": "x" }))` |
| `expect(res.status).toBe(200)` | `assert_eq!(res.status(), 200)` |
| `res.body` | `res.json::<T>()` |

## Una API CRUD, lado a lado

La misma API de tareas en memoria, primero con Express:

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
  if (!todo) return res.status(404).json({ error: 'tarea no encontrada' });
  res.json(todo);
});

app.post('/todos', (req, res) => {
  const { title } = req.body ?? {};
  if (!title || !title.trim()) {
    return res.status(422).json({ error: 'el título es obligatorio' });
  }
  const todo = { id: ++nextId, title, done: false };
  todos.set(todo.id, todo);
  res.status(201).json(todo);
});

app.patch('/todos/:id', (req, res) => {
  const todo = todos.get(Number(req.params.id));
  if (!todo) return res.status(404).json({ error: 'tarea no encontrada' });
  const { title, done } = req.body ?? {};
  if (title !== undefined) todo.title = title;
  if (done !== undefined) todo.done = done;
  res.json(todo);
});

app.delete('/todos/:id', (req, res) => {
  if (!todos.delete(Number(req.params.id))) {
    return res.status(404).json({ error: 'tarea no encontrada' });
  }
  res.sendStatus(204);
});

app.listen(3000, () => console.log('Escuchando en http://localhost:3000'));
```

Y ahora con Vitesse:

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

/// El almacén en memoria, compartido por todas las peticiones (el `Mutex` lo hace seguro entre hilos).
#[derive(Default)]
struct Store {
    next_id: u64,
    todos: BTreeMap<u64, Todo>,
}

type Db = Mutex<Store>;

fn not_found() -> Error {
    Error::not_found("tarea no encontrada")
}

async fn list(req: Request) -> Json<Vec<Todo>> {
    let store = req.state::<Db>().lock().unwrap();
    Json(store.todos.values().cloned().collect())
}

async fn show(req: Request) -> vitesse::Result<Json<Todo>> {
    let id: u64 = req.param_as("id")?; // 400 si no es un número
    let store = req.state::<Db>().lock().unwrap();
    let todo = store.todos.get(&id).ok_or_else(not_found)?;
    Ok(Json(todo.clone()))
}

async fn create(req: Request) -> vitesse::Result<(StatusCode, Json<Todo>)> {
    let new: NewTodo = req.json().await?; // 400 si el JSON no es válido o falta `title`
    if new.title.trim().is_empty() {
        return Err(Error::unprocessable("el título es obligatorio"));
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
    println!("Escuchando en http://localhost:3000");
    app().run(3000)
}
```

Qué ha cambiado:

- **La forma de los datos se declara una sola vez.** `NewTodo` dice que `title` es un texto obligatorio: `{}` o `{"title": 42}` se rechazan con un `400` y un mensaje que explica qué falla. En Express, esa validación corre de tu cuenta.
- **Los parámetros se analizan, no solo se leen.** `Number('abc')` da `NaN` en silencio (y aquí un 404); `req.param_as::<u64>("id")?` responde `400` directamente.
- **«No encontrado» es un valor.** `ok_or_else(not_found)?` convierte una entrada que falta en un `404` y sale de la función, en lugar de un `return res.status(404)…` anticipado.
- **El `Map` global pasa a ser estado.** Las peticiones se ejecutan en paralelo en varios hilos, así que el almacén se registra con `app.state` y se protege con un `Mutex`.
- **Los estados están en el tipo de retorno.** `(StatusCode::CREATED, Json(todo))` para un 201, `StatusCode::NO_CONTENT` para un 204: no puedes olvidarte de enviar una respuesta.

## Middlewares, lado a lado

```js
// Registra cada petición con su duración.
app.use((req, res, next) => {
  const start = Date.now();
  res.on('finish', () => console.log(`${req.method} ${req.path} ${Date.now() - start} ms`));
  next();
});

// Protege una ruta.
function auth(req, res, next) {
  if (req.get('authorization') !== 'Bearer secret') {
    return res.status(401).json({ error: 'inicia sesión' });
  }
  res.locals.user = { name: 'ada' };
  next();
}

app.get('/admin', auth, (req, res) => res.send(`Bienvenida, ${res.locals.user.name}`));
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
        return Error::unauthorized("inicia sesión").into_response();
    }
    req.set(User { name: "ada".into() }); // res.locals.user = …
    next.run(req).await                    // next()
}

async fn admin(req: Request) -> String {
    let user = req.get::<User>().unwrap(); // lo ha puesto `auth`
    format!("Bienvenida, {}", user.name)
}

fn app() -> App {
    let mut app = App::new();

    // Registra cada petición con su duración.
    app.middleware(|req: Request, next: Next| async move {
        let start = Instant::now();
        let line = format!("{} {}", req.method(), req.path());
        let res = next.run(req).await;
        println!("{line} {:?}", start.elapsed());
        res
    });

    // Protege una ruta.
    app.get("/admin", admin.with(auth));
    app
}
```

No hay `res.on('finish')`: el código que va después de `next.run(req).await` se ejecuta cuando el resto de la cadena ya ha producido la respuesta, e incluso puede modificarla (`res.header(...)`) antes de devolverla. Un middleware que quiere detener la petición simplemente devuelve una respuesta sin llamar a `next`. Consulta [Middlewares](middleware.md).

## Errores, lado a lado

```js
app.get('/users/:id', async (req, res) => {
  const user = await findUser(req.params.id);
  if (!user) return res.status(404).json({ error: 'usuario no encontrado' });
  res.json(user);
});

app.use((err, req, res, next) => {
  console.error(err);
  res.status(err.status || 500).json({ message: err.message });
});
```

```rust
// `find_user` es tu llamada a la base de datos: devuelve `Result<Option<User>, _>`.
async fn show_user(req: Request) -> vitesse::Result<Json<User>> {
    let id: u64 = req.param_as("id")?;
    let user = find_user(id).await?.ok_or_else(|| Error::not_found("usuario no encontrado"))?;
    Ok(Json(user))
}

app.get("/users/:id", show_user);

app.on_error(|err: Error| {
    res::status(err.status()).json(json!({ "message": err.message() }))
});
```

`Error` lleva un estado HTTP y un mensaje para el cliente. Cualquier otro error (base de datos, E/S, parseo…) se convierte automáticamente con `?` en un `500` cuyos detalles solo se registran en el log, nunca se envían al cliente. `app.on_error` se aplica a todas las respuestas de error, da igual dónde se registre: tus errores, los 404, los 405, los cuerpos no válidos y los pánicos. Consulta [Errores](errors.md).

## Pruebas, lado a lado

```js
const request = require('supertest');
const app = require('./app');

test('crea una tarea', async () => {
  const res = await request(app).post('/todos').send({ title: 'Aprender Rust' });
  expect(res.status).toBe(201);
  expect(res.body.title).toBe('Aprender Rust');
});
```

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use vitesse::serde_json::Value;
    use vitesse::test::TestClient;

    #[tokio::test]
    async fn crea_una_tarea() {
        let client = TestClient::new(app());

        let res = client.post("/todos").json(&json!({ "title": "Aprender Rust" })).await;

        assert_eq!(res.status(), 201);
        let body: Value = res.json();
        assert_eq!(body["title"], "Aprender Rust");
    }
}
```

`TestClient` llama a la aplicación en memoria, sin abrir ningún puerto. Lo tienes todo en [Pruebas](testing.md).

## Diferencias clave

### `async move` y los closures

```rust
app.get("/", |_| async { "¡Hola!" });                   // la petición no se usa
app.get("/hello/:name", |req: Request| async move {     // la petición se usa
    format!("¡Hola, {}!", req.param("name").unwrap_or("desconocido"))
});
```

Un closure recibe la petición y devuelve un bloque `async`. Cuando el bloque usa `req`, escribe `async move` para que se quede con la petición, e indica el tipo (`req: Request`) para que el compilador lo conozca. Cuando un handler crece, una `async fn` con nombre suele leerse mejor que un closure.

### JSON con tipos gracias a serde

Deriva `Deserialize` para lo que recibes y `Serialize` para lo que envías. Los campos opcionales son `Option<T>`; los campos desconocidos se ignoran por defecto. Cuando no conoces la estructura de antemano, usa `serde_json::Value`, el equivalente a un objeto JavaScript cualquiera (`vitesse::serde_json` está reexportado).

### Errores con `?`

`?` sustituye a la vez a `try/catch` y a `next(err)`. Las funciones de Vitesse que pueden fallar (`req.json()`, `req.param_as()`, `req.query_as()`…) ya devuelven un `vitesse::Error` con el estado adecuado. En un closure que usa `?`, indícale al compilador el tipo de error con `Ok::<_, Error>(valor)` como última expresión, o usa una función con nombre que devuelva `vitesse::Result<T>`.

### Comprobado al compilar, y al arrancar

Un handler que devuelve algo que no es una respuesta, una errata en el nombre de un método, un `.await` olvidado: el compilador se niega a construir el programa. Las rutas se comprueban en cuanto se añaden: un patrón no válido o la misma ruta definida dos veces hace que el programa entre en pánico al arrancar, indicando la línea culpable.

### El estado compartido debe ser seguro entre hilos

Las peticiones se atienden en paralelo en varios hilos, así que no hay variables globales modificables: registra tus datos con `app.state(...)` y protege lo que cambia con un atómico, un `Mutex` o un `RwLock`. Leer el estado no cuesta nada (`req.state::<T>()` devuelve una simple referencia). Consulta [Estado compartido](state.md).

## Trampas habituales

- **Los middlewares globales se ejecutan antes del enrutamiento, en todas las peticiones.** Da igual que llames a `app.middleware(...)` antes o después de tus rutas: todos los middlewares globales se ejecutan, en el orden en que se añadieron, antes del enrutamiento, incluidos los 404. Para afectar solo a algunas rutas, usa un [router](routers.md) (cuyos middlewares cubren todo su prefijo, como `router.use`) o `handler.with(mw)`.
- **Las rutas distinguen mayúsculas y minúsculas.** `/Users` no coincide con `/users` (Express las ignora por defecto). La barra final se ignora: `/users/` coincide con `/users`, como en Express.
- **Un método equivocado responde `405`, no `404`,** con una cabecera `Allow`. `HEAD` usa la ruta `GET` y `OPTIONS` responde `204` automáticamente.
- **Un solo handler por método y ruta.** Definir dos veces la misma ruta provoca un pánico al arrancar; la costumbre de Express de encadenar varios handlers en una ruta con `next()` se convierte en un middleware.
- **`req.json()` no comprueba el `Content-Type`.** `express.json()` ignora los cuerpos que no son JSON; Vitesse analiza lo que recibe. Para exigir la cabecera, comprueba `req.is("json")` y devuelve un `415`.
- **Las query strings son planas.** `?user[name]=ada` no se convierte en un objeto anidado. Para una clave repetida como `?tag=a&tag=b`, recorre `req.query_pairs()`.
- **No hay parámetros opcionales ni expresiones regulares,** ni parámetros parciales dentro de un segmento (`/:id?`, `/:id(\d+)`, `/vuelos/:desde-:hasta`): declara rutas separadas o analiza tú mismo el segmento.
- **Los errores son JSON por defecto** (`{"error": "..."}`), y los detalles internos nunca llegan al cliente, ni siquiera en desarrollo. Usa `app.on_error` para otro formato.
- **`req.set(valor)` exige `Clone`.** Añade `#[derive(Clone)]` a los tipos que adjuntas a una petición.
- **El cuerpo se lee bajo demanda, y una sola vez desde la red.** El resultado se guarda en caché: llamar a `req.text()` y después a `req.bytes()` funciona.
- **Recargar con cada cambio.** `cargo run` recompila lo que ha cambiado; para un bucle al estilo de `nodemon`, usa una herramienta como [cargo-watch](https://github.com/watchexec/cargo-watch) (`cargo watch -x run`). Compila con `cargo build --release` para producción.

Para ir más allá, la [chuleta](cheatsheet.md) recoge toda la API en una sola página.
