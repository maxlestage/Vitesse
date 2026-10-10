# Leer la petición

Cada handler recibe una `Request`. De ella lees el método, la ruta, los parámetros de ruta, la query string, las cabeceras, las cookies, el cuerpo y la dirección del cliente. Esta página repasa todo eso, con el equivalente de Express cuando existe.

## El objeto `Request`

Un handler recibe la petición por valor. Casi todos los métodos reciben `&self`, incluida la lectura del cuerpo, así que puedes conservar un parámetro mientras esperas el cuerpo:

```rust
app.put("/users/:id", |req: Request| async move {
    let id: u64 = req.param_as("id")?;
    let agent = req.header("user-agent").unwrap_or("desconocido");
    let body: vitesse::serde_json::Value = req.json().await?;
    Ok::<_, Error>(format!("usuario {id} modificado por {agent}: {body}"))
});
```

Los pocos métodos que modifican la petición (`set`, `set_body`, `set_uri`, `headers_mut`, `extensions_mut`) necesitan un `mut req`. Los vas a usar sobre todo en [middlewares](middleware.md).

## Método, ruta y URL

```rust
app.all("/debug", |req: Request| async move {
    format!(
        "{} {} query={:?} version={:?} host={:?}",
        req.method(),       // GET
        req.path(),         // /debug
        req.query_string(), // Some("a=1&b=2")
        req.version(),      // HTTP/1.1
        req.hostname(),     // Some("localhost")
    )
});
```

- `req.method()` devuelve un `&Method`. Compáralo con `*req.method() == Method::POST`.
- `req.path()` devuelve la ruta sin la query string, tal como llegó (sin decodificar), como `req.path` en Express.
- `req.uri()` devuelve la `http::Uri` completa, ruta y query string, como `req.originalUrl`.
- `req.hostname()` devuelve la cabecera `Host` sin el puerto: `example.com` para `example.com:8080`.
- `req.version()` devuelve la versión de HTTP (`vitesse::http::Version`).

### Reescribir la URL

`req.set_uri(uri)` reemplaza la ruta y la query string. Los middlewares globales se ejecutan antes del enrutamiento, así que una reescritura en ese punto cambia la ruta que responde:

```rust
app.middleware(|mut req: Request, next: Next| async move {
    if req.path() == "/old-page" {
        req.set_uri(vitesse::http::Uri::from_static("/new-page"));
    }
    next.run(req).await
});
```

El navegador no ve esta reescritura. Para enviarlo a la nueva dirección, devuelve un `Redirect::to(...)` (consulta [Enviar la respuesta](responses.md)).

## Parámetros de ruta

Están explicados en detalle en [Enrutamiento](routing.md). En resumen:

```rust
app.get("/users/:id/posts/:post", |req: Request| async move {
    let user: u64 = req.param_as("id")?;              // tipado, 400 si no es válido
    let post = req.param("post").unwrap_or_default(); // Option<&str>
    let all: Vec<String> = req.params().map(|(k, v)| format!("{k}={v}")).collect();
    Ok::<_, Error>(format!("{user} {post} {}", all.join("&")))
});
```

## Query string

```rust
app.get("/search", |req: Request| async move {
    let q = req.query("q").unwrap_or_default();
    let page: u32 = req.query("page").and_then(|p| p.parse().ok()).unwrap_or(1);
    format!("buscando {q:?}, página {page}")
});
```

`GET /search?q=caf%C3%A9+cr%C3%A8me&page=2` responde `buscando "café crème", página 2`.

- `req.query(nombre)` devuelve el valor decodificado del primer parámetro con ese nombre, como `Option<Cow<str>>`. Un `+` se convierte en espacio. Un `Cow<str>` se usa como un `&str` (y `.into_owned()` te da un `String`). Solo reserva memoria cuando el valor contenía caracteres codificados.
- `req.query_pairs()` recorre todos los pares `(nombre, valor)`, incluidos los nombres repetidos. Así se lee `?tag=a&tag=b`:

```rust
app.get("/tags", |req: Request| async move {
    let tags: Vec<String> = req
        .query_pairs()
        .filter(|(key, _)| key == "tag")
        .map(|(_, value)| value.into_owned())
        .collect();
    format!("etiquetas: {}", tags.join(", "))
});
```

- `req.query_string()` devuelve la cadena en bruto (`q=caf%C3%A9+cr%C3%A8me&page=2`), o `None` si no hay query string.

### Query string tipada con `query_as`

En cuanto tengas más de un par de parámetros, deserializa toda la query string en una estructura con `req.query_as::<T>()`. Usa `Option` para los parámetros opcionales y `#[serde(default)]` para los valores por defecto:

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Search {
    q: String,
    page: Option<u32>,
    #[serde(default)]
    exact: bool,
}

async fn search(req: Request) -> vitesse::Result<String> {
    let s: Search = req.query_as()?;
    Ok(format!("{} (página {}, exacta: {})", s.q, s.page.unwrap_or(1), s.exact))
}
```

Si falta un campo obligatorio o un valor no se puede convertir, `query_as` devuelve un `400 Bad Request`:

```text
GET /search               → 400 {"error":"invalid query string: missing field `q`"}
GET /search?q=x&page=abc  → 400 {"error":"invalid query string: invalid digit found in string"}
```

> [!NOTE]
> `query_as` no admite claves repetidas: `?q=a&q=b` se rechaza con un `400` (`duplicate field`). Para esos casos, usa `req.query_pairs()`.

## Cabeceras

```rust
app.get("/whoami", |req: Request| async move {
    let agent = req.header("user-agent").unwrap_or("desconocido");
    format!("Estás usando {agent}")
});
```

- `req.header(nombre)` devuelve el valor como `Option<&str>`, como `req.get('User-Agent')` en Express. El nombre no distingue mayúsculas de minúsculas. También puedes pasar una constante de `vitesse::header`, por ejemplo `req.header(header::AUTHORIZATION)`.
- `req.header_all(nombre)` recorre todos los valores de una cabecera repetida.
- `req.headers()` devuelve el `HeaderMap` completo, por ejemplo para recorrer todas las cabeceras. Se construye la primera vez que lo llamas, así que si solo necesitas unas pocas cabeceras, `req.header(...)` sale más barato: lee directamente la petición en bruto.
- `req.headers_mut()` permite que un middleware agregue, modifique o quite cabeceras antes de que el handler las vea.

> [!NOTE]
> Un valor de cabecera que no sea ASCII visible simple (con letras acentuadas, por ejemplo) no siempre se puede devolver como `&str`. En ese caso, lee los bytes en bruto con `req.headers().get("x-name").map(|v| v.as_bytes())`.

### Tipo de contenido

- `req.content_type()` es un atajo de `req.header("content-type")`.
- `req.is(tipo)` comprueba el tipo del cuerpo, como `req.is()` en Express, pero devuelve un `bool`. Acepta un tipo completo (`"application/json"`), un subtipo (`"json"`, que también reconoce sufijos como `application/ld+json`) o un comodín (`"text/*"`). Los parámetros como `; charset=utf-8` se ignoran.

```rust
app.post("/import", |req: Request| async move {
    if !req.is("json") {
        return Err(Error::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, "se esperaba un cuerpo JSON"));
    }
    let items: Vec<vitesse::serde_json::Value> = req.json().await?;
    Ok(format!("{} elementos importados", items.len()))
});
```

## Cookies

`req.cookie(nombre)` lee una cookie de la cabecera `Cookie`. En Express necesitarías `cookie-parser` para `req.cookies.nombre`; aquí no hay que instalar nada:

```rust
app.get("/", |req: Request| async move {
    match req.cookie("session") {
        Some(id) => format!("Bienvenido de nuevo (sesión {id})"),
        None => "Hola, desconocido".to_string(),
    }
});
```

El valor se devuelve tal como lo envió el navegador (sin las comillas que lo rodean). No se decodifica ni se verifica ninguna firma. Para crear o borrar cookies, consulta [Enviar la respuesta](responses.md).

## Cuerpo

Vitesse no tiene un middleware para analizar el cuerpo. Lees el cuerpo llamando a un método, que lo recoge y lo analiza en ese momento. Todos estos métodos son `async` y devuelven un `vitesse::Result`, así que `?` convierte un cuerpo incorrecto en el error HTTP adecuado:

| Método | Equivalente en Express | Obtienes | Error |
|---|---|---|---|
| `req.json::<T>().await` | `express.json()` | `T` (deserializado) | `400` si el JSON no es válido |
| `req.form::<T>().await` | `express.urlencoded()` | `T` (deserializado) | `400` si el formulario no es válido |
| `req.text().await` | `express.text()` | `String` | `400` si no es UTF-8 |
| `req.bytes().await` | `express.raw()` | `Bytes` | |
| `req.take_body()` | leer `req` como flujo | `Body` (un flujo) | |

Los cuatro primeros leen todo el cuerpo en memoria, dentro del límite de tamaño (ver más abajo).

### JSON

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct NewPost {
    title: String,
    tags: Vec<String>,
    draft: Option<bool>,
}

async fn create_post(req: Request) -> vitesse::Result<String> {
    let post: NewPost = req.json().await?;
    Ok(format!(
        "{} ({} etiquetas, borrador: {})",
        post.title,
        post.tags.len(),
        post.draft.unwrap_or(false)
    ))
}
```

- Un JSON no válido, un campo que falta o un tipo incorrecto dan un `400` que explica el motivo (ejemplo abajo). Un cuerpo vacío también es un JSON no válido.
- Los campos desconocidos se ignoran. Agrega `#[serde(deny_unknown_fields)]` a la estructura para rechazarlos.
- Para JSON sin forma fija, pide un `vitesse::serde_json::Value`.
- A diferencia de `express.json()`, `req.json()` no mira el `Content-Type`: analiza lo que se haya enviado. Si quieres exigirlo, comprueba primero `req.is("json")` (ejemplo más arriba).

Por ejemplo, enviar `{"tags": []}` a este handler da:

```json
{"error":"invalid JSON: missing field `title` at line 1 column 12"}
```

### Formularios

Los formularios HTML (`application/x-www-form-urlencoded`) se leen igual, con `req.form()`:

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Login {
    email: String,
    password: String,
    // Una casilla sin marcar no se envía: `false` por defecto
    #[serde(default)]
    remember: bool,
}

async fn login(req: Request) -> vitesse::Result<Redirect> {
    let form: Login = req.form().await?;
    if form.email.is_empty() || form.password.is_empty() {
        return Err(Error::bad_request("el correo y la contraseña son obligatorios"));
    }
    // … comprobar las credenciales, abrir una sesión…
    Ok(Redirect::see_other("/dashboard"))
}
```

Vitesse no analiza `multipart/form-data` (formularios con subida de archivos). Lee el flujo en bruto con [`take_body()`](#streaming-con-take_body) y pásalo a un crate que analice multipart.

### Texto y bytes en bruto

```rust
app.post("/shout", |req: Request| async move {
    let text = req.text().await?; // String, 400 si no es UTF-8 válido
    Ok::<_, Error>(text.to_uppercase())
});

app.post("/size", |req: Request| async move {
    let bytes = req.bytes().await?; // vitesse::Bytes
    Ok::<_, Error>(format!("{} bytes recibidos", bytes.len()))
});
```

El cuerpo se lee una sola vez y luego se guarda en memoria, así que puedes llamar a estos métodos varias veces. Por ejemplo, un webhook puede verificar una firma sobre los bytes en bruto y luego analizar el mismo cuerpo como JSON:

```rust
app.post("/webhook", |req: Request| async move {
    let raw = req.bytes().await?;          // verificar una firma sobre los bytes en bruto…
    let event: vitesse::serde_json::Value = req.json().await?; // …y luego analizarlos
    Ok::<_, Error>(StatusCode::NO_CONTENT)
});
```

### Límite de tamaño del cuerpo y 413

`json`, `form`, `text` y `bytes` leen todo el cuerpo en memoria, así que su tamaño está limitado: **1 MiB** por defecto (`vitesse::DEFAULT_BODY_LIMIT`). Un cuerpo más grande recibe un `413 Payload Too Large`. Si el cliente anunció el tamaño con `Content-Length`, la petición se rechaza sin llegar a leer el cuerpo. Para cambiar el límite:

```rust
let mut app = App::new();
app.body_limit(10 * 1024 * 1024); // 10 MiB
```

- El límite vale para toda la aplicación, como `express.json({ limit: '10mb' })`.
- Solo entra en juego cuando lees el cuerpo. Un handler que nunca lo lee nunca produce un `413`.
- Si solo una ruta necesita más (una subida de archivos, por ejemplo), mantén un límite global bajo y lee el cuerpo de esa ruta como flujo con `take_body()`, aplicando tu propio límite.

### Streaming con `take_body`

Para subidas grandes, o para reenviar un cuerpo a otro sitio, `req.take_body()` te da el cuerpo en bruto como un flujo (`vitesse::Body`), sin cargarlo en memoria. Los datos llegan a medida que el cliente los envía.

> [!WARNING]
> `take_body()` se salta `app.body_limit`, así que limitar el tamaño es cosa tuya. Después, `json()`, `text()` y los demás métodos de lectura ya no pueden leer el cuerpo: fallan con un `500`.

`Body` implementa el trait estándar `http_body::Body`. La forma más sencilla de leerlo trozo a trozo es el método `frame()` de [http-body-util](https://docs.rs/http-body-util) (`cargo add http-body-util`). Este ejemplo guarda una subida en un archivo, con un tope de 100 MiB:

```rust
use http_body_util::BodyExt; // para .frame()
use vitesse::prelude::*;
use vitesse::tokio::{fs::File, io::AsyncWriteExt};

const MAX_UPLOAD: usize = 100 * 1024 * 1024; // 100 MiB

async fn upload(req: Request) -> vitesse::Result<String> {
    let mut body = req.take_body();
    let mut file = File::create("upload.bin").await?;
    let mut total = 0;

    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|e| Error::bad_request("subida interrumpida").with_source(e))?;
        if let Ok(chunk) = frame.into_data() {
            total += chunk.len();
            if total > MAX_UPLOAD {
                return Err(Error::payload_too_large());
            }
            file.write_all(&chunk).await?;
        }
    }
    file.flush().await?;
    Ok(format!("{total} bytes guardados"))
}
```

`?` convierte los errores de archivo (`std::io::Error`) en un `500`. En cambio, un error de lectura del flujo es un `BoxError` genérico que `?` no sabe convertir por sí solo; por eso el ejemplo lo envuelve con `map_err`.

Una respuesta también puede ser un `Body`, así que devolver el flujo tal cual da un eco en streaming:

```rust
app.post("/echo", |req: Request| async move { req.take_body() });
```

### Reemplazar el cuerpo

`req.set_body(cuerpo)` reemplaza el cuerpo antes de que el handler lo lea. Un middleware puede usarlo para descomprimir o descifrar el cuerpo. Acepta cualquier cosa que se convierta en `Body`: `String`, `Vec<u8>`, `Bytes`, `&'static str`, etc. El nuevo cuerpo está sujeto a `body_limit` igual que el original.

## Dirección del cliente

```rust
app.get("/ip", |req: Request| async move {
    match req.ip() {
        Some(ip) => format!("Tu IP: {ip}"),
        None => "desconocida".to_string(),
    }
});
```

- `req.ip()` devuelve la dirección IP del cliente conectado como `Option<IpAddr>`, como `req.ip`. `req.remote_addr()` devuelve la IP y el puerto como `Option<SocketAddr>`.
- Detrás de un proxy inverso o un balanceador de carga (Nginx, Heroku, el balanceador de una nube, etc.), esa dirección es la del proxy. El cliente original suele aparecer en la cabecera `X-Forwarded-For`. Vitesse no tiene un equivalente del ajuste `trust proxy` de Express, así que lee la cabecera tú mismo (consulta también [Puesta en producción](production.md)):

```rust
fn client_ip(req: &Request) -> Option<String> {
    req.header("x-forwarded-for")
        .and_then(|list| list.split(',').next())
        .map(|ip| ip.trim().to_string())
        .or_else(|| req.ip().map(|ip| ip.to_string()))
}
```

> [!WARNING]
> Un cliente puede enviar `X-Forwarded-For` por su cuenta. Confía en ella solo si la pone tu proxy, y nunca tomes decisiones de seguridad basándote en ella.

## Estado y datos por petición

- `req.state::<T>()` devuelve el estado global registrado con `app.state(valor)`, el equivalente de `app.locals`. Entra en pánico (y por tanto responde `500`) si no existe un estado de ese tipo. `req.try_state::<T>()` devuelve un `Option` en su lugar. Consulta [Estado compartido](state.md).
- `req.set(valor)` adjunta un valor a esta petición y `req.get::<T>()` lo vuelve a leer, el equivalente de `res.locals` o `req.user`. El tipo tiene que implementar `Clone`. Así es como un middleware le pasa el usuario autenticado al handler (consulta [Middlewares](middleware.md)).
- `req.extensions()` y `req.extensions_mut()` dan acceso directo al almacenamiento `http::Extensions` subyacente.

```rust
#[derive(Clone)]
struct User {
    name: String,
}

app.middleware(|mut req: Request, next: Next| async move {
    if req.header("authorization") == Some("Bearer secret") {
        req.set(User { name: "ada".into() });
    }
    next.run(req).await
});

app.get("/me", |req: Request| async move {
    match req.get::<User>() {
        Some(user) => format!("Hola, {}", user.name),
        None => "No has iniciado sesión".to_string(),
    }
});
```

## Referencia de métodos

| Método | Express | Devuelve |
|---|---|---|
| `method()` | `req.method` | `&Method` |
| `path()` | `req.path` | `&str` |
| `uri()` | `req.originalUrl` | `&Uri` |
| `version()` | `req.httpVersion` | `Version` |
| `hostname()` | `req.hostname` | `Option<&str>` |
| `set_uri(uri)` | asignar `req.url` | |
| `param(nombre)` | `req.params.nombre` | `Option<&str>` |
| `param_as::<T>(nombre)` | | `Result<T, Error>` (`400`) |
| `params()` | `req.params` | iterador de `(&str, &str)` |
| `query(nombre)` | `req.query.nombre` | `Option<Cow<str>>` |
| `query_as::<T>()` | | `Result<T, Error>` (`400`) |
| `query_pairs()` | | iterador de `(Cow<str>, Cow<str>)` |
| `query_string()` | | `Option<&str>` |
| `header(nombre)` | `req.get(nombre)` | `Option<&str>` |
| `header_all(nombre)` | | iterador de `&str` |
| `headers()` / `headers_mut()` | `req.headers` | `&HeaderMap` / `&mut HeaderMap` |
| `content_type()` | `req.get('content-type')` | `Option<&str>` |
| `is(tipo)` | `req.is(tipo)` | `bool` |
| `cookie(nombre)` | `req.cookies.nombre` | `Option<&str>` |
| `json::<T>().await` | `req.body` + `express.json()` | `Result<T, Error>` |
| `form::<T>().await` | `req.body` + `express.urlencoded()` | `Result<T, Error>` |
| `text().await` | `req.body` + `express.text()` | `Result<String, Error>` |
| `bytes().await` | `req.body` + `express.raw()` | `Result<Bytes, Error>` |
| `take_body()` | `req` (flujo) | `Body` |
| `set_body(cuerpo)` | | |
| `ip()` | `req.ip` | `Option<IpAddr>` |
| `remote_addr()` | `req.socket.remoteAddress` | `Option<SocketAddr>` |
| `state::<T>()` / `try_state::<T>()` | `req.app.locals` | `&T` / `Option<&T>` |
| `get::<T>()` / `set(valor)` | `res.locals` | `Option<&T>` / `Option<T>` (el valor anterior) |
| `extensions()` / `extensions_mut()` | | `&Extensions` / `&mut Extensions` |
