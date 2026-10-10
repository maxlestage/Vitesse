# Tu primera aplicación

En este tutorial vas a construir una pequeña API JSON paso a paso: un Hello World, una ruta JSON, una ruta con un parámetro, una ruta `POST` que lee un cuerpo JSON y el registro de las peticiones. Te llevará unos diez minutos.

Primero necesitas tener Rust instalado. Consulta [Instalación](installation.md).

## 1. Crear el proyecto

```sh
cargo new hello-vitesse
cd hello-vitesse
cargo add vitesse
cargo add serde --features derive
```

Si Vitesse todavía no está en crates.io, usa `cargo add vitesse --git https://github.com/maxlestage/Vitesse` (consulta [Instalación](installation.md#agregar-vitesse)).

## 2. Hello World

Reemplaza `src/main.rs` por:

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    println!("Vitesse escuchando en http://localhost:3000");
    app.run(3000)
}
```

Arráncalo con `cargo run` y, en otra terminal:

```sh
curl http://localhost:3000
```

```text
Hello World!
```

Qué hace cada línea:

- `use vitesse::prelude::*;` importa los tipos que vas a usar todo el tiempo: `App`, `Request`, `Json`, `Error`, `StatusCode`, etc.
- `App::new()` equivale a `express()`. `app` se declara `mut` porque agregar rutas la modifica.
- `app.get(ruta, handler)` registra una ruta. El handler es una closure asíncrona. Recibe la petición (aquí ignorada con `_`) y devuelve cualquier cosa que se pueda convertir en una respuesta. Un `&str` se convierte en un `200` con `Content-Type: text/plain`.
- `app.run(3000)` arranca el servidor en el puerto 3000, en todas las interfaces de red y con todos los núcleos de la CPU. Se queda bloqueado hasta que pulsas `Ctrl+C`. Devuelve un `std::io::Result<()>`, por eso `main` devuelve ese mismo tipo. Si el puerto ya está ocupado, el programa termina con el error.

Con Express, lo mismo sería:

```js
const app = express();
app.get('/', (req, res) => res.send('Hello World!'));
app.listen(3000);
```

Antes de cada uno de los siguientes pasos, detén el servidor con `Ctrl+C` y vuelve a arrancarlo con `cargo run`.

## 3. Una ruta JSON

Agrega esta ruta debajo de la primera:

```rust
app.get("/api/status", |_| async {
    json!({ "status": "ok", "framework": "Vitesse" })
});
```

`json!` construye un valor JSON con una sintaxis parecida a la de JavaScript. Devolverlo envía un `200` con `Content-Type: application/json`.

```sh
curl http://localhost:3000/api/status
```

```json
{"framework":"Vitesse","status":"ok"}
```

> [!NOTE]
> Las claves llegan ordenadas alfabéticamente porque `json!` guarda los objetos en un mapa ordenado. Cuando serializas tu propia estructura con `Json(...)` (paso 5), los campos conservan su orden de declaración.

## 4. Una ruta con un parámetro

```rust
app.get("/hello/:name", |req: Request| async move {
    let name = req.param("name").unwrap_or("desconocido");
    format!("¡Hola, {name}!")
});
```

- `:name` es un parámetro de ruta, como en Express. `req.param("name")` devuelve un `Option<&str>`, que vale `Some("Ada")` para `/hello/Ada`. El valor ya viene decodificado: `/hello/Fran%C3%A7ois` da `François`.
- Cuando el handler usa la petición, escribe `|req: Request| async move { ... }`. `move` mueve la petición dentro del bloque asíncrono para que pueda usarla. Si no la usa, basta con `|_| async { ... }`.
- `format!` construye un `String`, que también es una respuesta `text/plain` válida.

```sh
curl http://localhost:3000/hello/Ada
```

```text
¡Hola, Ada!
```

¿Necesitas un número? `req.param_as::<u64>("id")?` convierte el parámetro y responde `400 Bad Request` por sí solo si no es un número. Consulta [Enrutamiento](routing.md).

## 5. Leer un cuerpo JSON

Ahora agrega una ruta `POST /users`. Recibe `{"name": "Ada"}` y responde `201 Created` con el nuevo usuario. Encima de `main`, agrega los tipos y el handler:

```rust
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

#[derive(Serialize)]
struct User {
    id: u64,
    name: String,
}

async fn create_user(req: Request) -> vitesse::Result<(StatusCode, Json<User>)> {
    let input: NewUser = req.json().await?;
    if input.name.trim().is_empty() {
        return Err(Error::bad_request("el nombre no puede estar vacío"));
    }
    let user = User { id: 1, name: input.name };
    Ok((StatusCode::CREATED, Json(user)))
}
```

Luego regístralo en `main`:

```rust
app.post("/users", create_user);
```

- Un handler puede ser una `async fn` con nombre que recibe una `Request`. Se lee mejor en cuanto el handler pasa de unas pocas líneas.
- `req.json().await` lee el cuerpo y lo deserializa en el tipo que pediste (`NewUser`). No hay que instalar nada parecido a `express.json()`: el cuerpo se analiza en el momento en que lo pides. Si el JSON no es válido o falta un campo, `?` devuelve un `400 Bad Request`. Si el cuerpo supera el límite (1 MiB por defecto), devuelve `413 Payload Too Large`.
- `vitesse::Result<T>` es una forma corta de `Result<T, vitesse::Error>`. `Error::bad_request(...)` crea un error `400`, y su mensaje se envía al cliente como `{"error": "..."}`.
- `(StatusCode::CREATED, Json(user))` envía la estructura como JSON con el estado `201`. Es el equivalente de `res.status(201).json(user)`.

Prueba una petición válida, un nombre vacío y un campo que falta:

```sh
curl -i -X POST http://localhost:3000/users \
  -H 'content-type: application/json' -d '{"name":"Ada"}'
```

```http
HTTP/1.1 201 Created
content-type: application/json
date: Sat, 10 Oct 2026 09:00:00 GMT
content-length: 21

{"id":1,"name":"Ada"}
```

```sh
curl -X POST http://localhost:3000/users \
  -H 'content-type: application/json' -d '{"name":""}'
# {"error":"el nombre no puede estar vacío"}

curl -X POST http://localhost:3000/users \
  -H 'content-type: application/json' -d '{"nombre":"Ada"}'
# {"error":"invalid JSON: missing field `name` at line 1 column 16"}
```

> [!TIP]
> También puedes escribir este handler como closure. Una closure no tiene tipo de retorno declarado, así que tienes que indicar el tipo de error en el `Ok`, con `Ok::<_, Error>((StatusCode::CREATED, Json(user)))`. Una función con nombre te lo ahorra.

## 6. Registrar cada petición

Agrega esta línea justo después de `App::new()`:

```rust
app.middleware(middleware::logger());
```

`app.middleware(...)` es el `app.use(fn)` de Express. Un middleware global se ejecuta en cada petición, incluidas las 404. `middleware::logger()` funciona como `morgan('dev')` y muestra una línea por petición:

```text
GET /hello/Ada 200 0.007 ms
POST /users 201 0.008 ms
```

Los middlewares globales se ejecutan en el orden en que los agregas. Escribir el tuyo lleva unas pocas líneas: consulta [Middlewares](middleware.md).

## 7. Leer el puerto del entorno

Muchas plataformas de hosting (Heroku, Render, Railway y otras) le indican a tu aplicación en qué puerto escuchar mediante la variable de entorno `PORT`. Reemplaza las dos últimas líneas de `main` por:

```rust
let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
println!("Vitesse escuchando en http://localhost:{port}");
app.run(port)
```

Es el equivalente de `app.listen(process.env.PORT || 3000)`. `app.run` acepta un número de puerto, una cadena como `"3000"` o `"127.0.0.1:8080"`, un `SocketAddr` y más, así que puedes pasarle tal cual el `String` leído del entorno.

## El programa completo

```rust
use serde::{Deserialize, Serialize};
use vitesse::prelude::*;

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

#[derive(Serialize)]
struct User {
    id: u64,
    name: String,
}

async fn create_user(req: Request) -> vitesse::Result<(StatusCode, Json<User>)> {
    let input: NewUser = req.json().await?;
    if input.name.trim().is_empty() {
        return Err(Error::bad_request("el nombre no puede estar vacío"));
    }
    let user = User { id: 1, name: input.name };
    Ok((StatusCode::CREATED, Json(user)))
}

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.middleware(middleware::logger());

    app.get("/", |_| async { "Hello World!" });

    app.get("/api/status", |_| async {
        json!({ "status": "ok", "framework": "Vitesse" })
    });

    app.get("/hello/:name", |req: Request| async move {
        let name = req.param("name").unwrap_or("desconocido");
        format!("¡Hola, {name}!")
    });

    app.post("/users", create_user);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
    println!("Vitesse escuchando en http://localhost:{port}");
    app.run(port)
}
```

## Ejecutar en modo release

```sh
cargo run --release
```

La primera compilación release tarda un poco más, pero después el servidor es mucho más rápido (agrega también el [perfil de release recomendado](installation.md#perfil-de-release-recomendado)). Para usar otro puerto:

```sh
PORT=8080 cargo run --release
```

Vitesse también se encarga de las rutas desconocidas y de los métodos incorrectos:

```sh
curl -i http://localhost:3000/nope
```

```http
HTTP/1.1 404 Not Found
content-type: application/json
date: Sat, 10 Oct 2026 09:00:00 GMT
content-length: 28

{"error":"Cannot GET /nope"}
```

```sh
curl -i -X DELETE http://localhost:3000/users
```

```http
HTTP/1.1 405 Method Not Allowed
content-type: application/json
allow: POST, OPTIONS
date: Sat, 10 Oct 2026 09:00:00 GMT
content-length: 30

{"error":"Method Not Allowed"}
```

## ¿Y ahora qué?

- [Enrutamiento](routing.md): comodines, parámetros tipados, prioridad de rutas, 404 y 405.
- [Leer la petición](requests.md): query string, cabeceras, cookies, formularios y subida de archivos.
- [Enviar la respuesta](responses.md): códigos de estado, cabeceras, cookies, redirecciones y archivos.
- [Estado compartido](state.md): reemplaza el `id: 1` fijo por un almacén en memoria o un pool de base de datos.
- [Gestión de errores](errors.md), [Pruebas](testing.md) y, por último, despliega con [Docker](docker.md).
