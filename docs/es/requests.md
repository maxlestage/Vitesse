# Leer la petición

Cada handler recibe una `Request`. De ella lees el método, la ruta, los parámetros de ruta, la query string, las cabeceras, las cookies y la sesión, el cuerpo, los archivos subidos y la dirección del cliente, averiguas qué formato prefiere el cliente y construyes las URL de tus rutas. Esta página repasa todo eso, con el equivalente de Express cuando existe.

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

Los pocos métodos que modifican la petición (`set`, `set_body`, `set_uri`, `set_method`, `set_body_limit`, `headers_mut`, `extensions_mut`) necesitan un `mut req`. Los vas a usar sobre todo en [middlewares](middleware.md).

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
- `req.hostname()` devuelve el host solicitado sin el puerto: `example.com` para `example.com:8080`. `req.host()` conserva el puerto (consulta [Host y protocolo](#host-y-protocolo)).
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

### Sobrescribir el método

`req.set_method(método)` reemplaza el método. Igual que `set_uri`, cambia la ruta que responde cuando lo llamas en un middleware global. Así se escribe el equivalente del paquete `method-override` de Express: los formularios HTML solo pueden enviar `GET` y `POST`, así que el formulario pasa en la query string el método que quiere:

```html
<form method="post" action="/posts/42?_method=DELETE">
  <button>Borrar</button>
</form>
```

```rust
app.middleware(|mut req: Request, next: Next| async move {
    if *req.method() == Method::POST {
        let wanted = req
            .query("_method")
            .and_then(|m| m.to_ascii_uppercase().parse::<Method>().ok())
            .filter(|m| [Method::PUT, Method::PATCH, Method::DELETE].contains(m));
        if let Some(method) = wanted {
            req.set_method(method);
        }
    }
    next.run(req).await
});

app.delete("/posts/:id", |_| async { "artículo borrado" }); // el formulario también llega aquí
```

Solo se reescriben las peticiones `POST`, y solo a `PUT`, `PATCH` o `DELETE`, como hace `method-override` por defecto: un enlace (`GET`) no puede borrar nada.

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

### La ruta elegida

`req.route()` devuelve el patrón de la ruta que responde a la petición, prefijo de montaje incluido: `Some("/users/:id/posts/:post")` para `/users/42/posts/7`. Es el `req.route.path` de Express, y es lo que quieres en los registros y las métricas, donde la ruta en bruto daría una línea por usuario. Consulta [Enrutamiento](routing.md#la-ruta-elegida-reqroute).

### Construir URL: `path_for` y `url_for`

Dale un nombre a una ruta con `.name(...)`, y cualquier handler puede construir su URL en lugar de escribirla a mano (el `url_for` de actix-web; Express no tiene equivalente):

```rust
app.get("/users/:id", show_user).name("user");

app.post("/users", |req: Request| async move {
    let id = 42; // el id del usuario que acabas de crear
    let location = req.url_for("user", &[&id])?; // "https://example.com/users/42"
    Ok::<_, Error>(res::status(201).header("location", location).json(json!({ "id": id })))
});
```

- `req.path_for(nombre, &[...])` devuelve la ruta, `"/users/42"`: los valores rellenan los parámetros de la ruta en orden, y se codifican (`"a b"` da `a%20b`). Pasa `&[]` para una ruta sin parámetros, y `&[&"LAX", &"SFO"]` para varios.
- `req.url_for(nombre, &[...])` añade el protocolo y el host de la petición actual (`req.protocol()` y `req.host()`, así que sigue a [`trust_proxy`](#detrás-de-un-proxy-trust_proxy)). Para los enlaces que salen del sitio, en un correo por ejemplo, prefiere `path_for` precedido de una URL base sacada de tu configuración (consulta [Host y protocolo](#host-y-protocolo)).
- Los dos devuelven un `Error` para un nombre desconocido, un valor que falta o que sobra, o un valor que no cumple la restricción del parámetro (y `url_for`, para una petición sin host). Son errores de programación, no del cliente: el cliente recibe un simple `500 {"error":"Internal Server Error"}`, y la causa se escribe en la salida de error.

Las rutas con nombre, los parámetros opcionales y las restricciones se explican en [Enrutamiento](routing.md#rutas-con-nombre-y-generación-de-url).

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

## Negociación de contenido

Algunas rutas pueden responder en varios formatos: HTML para un navegador, JSON para un script. `req.accepts(&[...])` lee la cabecera `Accept` y devuelve el formato que el cliente prefiere entre los que ofreces, como `req.accepts()` en Express:

```rust
async fn show_user(req: Request) -> Response {
    match req.accepts(&["html", "json"]) {
        Some("html") => res::html("<h1>Ada Lovelace</h1>"),
        Some(_) => res::json(json!({ "name": "Ada Lovelace" })),
        None => res::send_status(406), // Not Acceptable
    }
}
```

| `Accept` enviado por el cliente | `req.accepts(&["html", "json"])` |
|---|---|
| ninguno | `Some("html")`: tu primera oferta |
| `text/html,application/xhtml+xml,…,*/*;q=0.8` (un navegador) | `Some("html")` |
| `application/json` | `Some("json")` |
| `*/*` | `Some("html")` |
| `image/png` | `None` |

- Las ofertas son extensiones de archivo (`"html"`, `"json"`, `"csv"`, `"txt"`…) o tipos MIME (`"application/json"`, `"image/*"`). Recibes la oferta tal como la escribiste, así que puedes hacer `match` sobre ella.
- Gana la calidad más alta (`q=`) y después la coincidencia más específica (`text/html` antes que `text/*`, y este antes que `*/*`). Un tipo enviado con `q=0` se rechaza.
- Sin cabecera `Accept`, el cliente acepta cualquier cosa: recibes tu primera oferta.
- `None` significa que nada de lo que ofreces es aceptable: responde `406 Not Acceptable`, o recurre a un formato por defecto.

> [!TIP]
> Cuando la respuesta depende de `Accept`, añádele `.header("vary", "accept")`, para que las cachés (la del navegador, una CDN) no sirvan la versión JSON a un navegador.

`req.accepts_language(&[...])` hace lo mismo con `Accept-Language`, como `req.acceptsLanguages()`. Un idioma coincide con sus variantes regionales y viceversa: `"fr"` coincide con `fr-FR`, y `"en-GB"` con `en`. Sin la cabecera recibes el primer idioma, y `None` cuando ningún idioma coincide, así que indica uno por defecto:

```rust
app.get("/", |req: Request| async move {
    let lang = req.accepts_language(&["en", "fr", "es"]).unwrap_or("en");
    Redirect::to(format!("/{lang}/"))
});
```

Un navegador en francés (`fr-FR,fr;q=0.9,en;q=0.8`) va a `/fr/`, y uno en alemán (`de`), a `/en/`.

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

El valor se devuelve tal como lo envió el navegador (sin las comillas que lo rodean), sin decodificar. Para crear o borrar cookies, consulta [Enviar la respuesta](responses.md#cookies).

### Cookies firmadas y la sesión

Con la feature `session`, de las cookies salen dos cosas más. `req.signed_cookie(nombre)` lee una cookie firmada con el secreto de la aplicación (`app.cookie_secret(...)`), como `req.signedCookies.nombre` en Express: obtienes el valor, o `None` si la cookie falta o si el visitante la modificó. Y con el middleware `session`, `req.session()` devuelve la sesión del visitante, como `req.session` con express-session:

```rust
app.get("/", |req: Request| async move {
    let theme = req.signed_cookie("theme").unwrap_or_else(|| "light".into());
    let user: Option<String> = req.session().get("user");
    format!("tema: {theme}, usuario: {user:?}")
});
```

La firma, las sesiones, el inicio y el cierre de sesión y los almacenes de sesiones se explican en [Cookies firmadas y sesiones](sessions.md).

## Cuerpo

Vitesse no tiene un middleware para analizar el cuerpo. Lees el cuerpo llamando a un método, que lo recoge y lo analiza en ese momento. Todos estos métodos son `async` y devuelven un `vitesse::Result`, así que `?` convierte un cuerpo incorrecto en el error HTTP adecuado:

| Método | Equivalente en Express | Obtienes | Error |
|---|---|---|---|
| `req.json::<T>().await` | `express.json()` | `T` (deserializado) | `400` si el JSON no es válido |
| `req.form::<T>().await` | `express.urlencoded()` | `T` (deserializado) | `400` si el formulario no es válido |
| `req.text().await` | `express.text()` | `String` | `400` si no es UTF-8 |
| `req.bytes().await` | `express.raw()` | `Bytes` | |
| `req.multipart()?` | `multer` | los campos de un formulario de subida, uno a uno (consulta [Subida de archivos](#subida-de-archivos-multipart)) | `415` si no es `multipart/form-data` |
| `req.take_body()` | leer `req` como flujo | `Body` (un flujo) | |

Los cuatro primeros leen todo el cuerpo en memoria, dentro del [límite de tamaño](#límite-de-tamaño-del-cuerpo-y-413), que también se aplica a `multipart`.

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

Los formularios que suben archivos (`enctype="multipart/form-data"`) se leen con `req.multipart()`: consulta [Subida de archivos (multipart)](#subida-de-archivos-multipart).

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

`json`, `form`, `text` y `bytes` leen todo el cuerpo en memoria, así que su tamaño está limitado: **1 MiB** por defecto (`vitesse::DEFAULT_BODY_LIMIT`). Un cuerpo más grande recibe un `413 Payload Too Large`. Si el cliente anunció el tamaño con `Content-Length`, la petición se rechaza sin llegar a leer el cuerpo. Para cambiar el límite de toda la aplicación:

```rust
let mut app = App::new();
app.body_limit(10 * 1024 * 1024); // 10 MiB
```

Sin embargo, la mayoría de las rutas solo reciben cuerpos JSON pequeños, y solo unas pocas necesitan más. Mantén bajo el límite global y súbelo donde haga falta con `middleware::body_limit(bytes)`, en una ruta con `.with(...)` o en todo un [router](routers.md). Es el equivalente de `express.json({ limit: '50mb' })` en una sola ruta:

```rust
async fn import(req: Request) -> vitesse::Result<String> {
    let rows: Vec<vitesse::serde_json::Value> = req.json().await?;
    Ok(format!("{} filas importadas", rows.len()))
}

let mut app = App::new(); // 1 MiB por defecto…
app.post("/import", import.with(middleware::body_limit(50 * 1024 * 1024))); // …50 MiB aquí

let mut admin = Router::new();
admin.middleware(middleware::body_limit(200 * 1024 * 1024)); // 200 MiB bajo /admin
admin.post("/restore", restore_backup);
app.mount("/admin", admin);
```

El límite pertenece a cada petición: `req.body_limit()` devuelve el que se aplica, y `req.set_body_limit(bytes)` lo cambia, que es todo lo que hace `middleware::body_limit`. Tu propio middleware puede usarlo para elegir un límite por usuario, por ejemplo (`CurrentUser` lo pone tu middleware de autenticación):

```rust
async fn larger_bodies_for_admins(mut req: Request, next: Next) -> Response {
    if req.get::<CurrentUser>().is_some_and(|user| user.admin) {
        req.set_body_limit(1024 * 1024 * 1024); // 1 GiB
    }
    next.run(req).await
}
```

- El límite solo entra en juego cuando lees el cuerpo. Un handler que nunca lo lee nunca produce un `413`.
- También cubre [`req.multipart()`](#subida-de-archivos-multipart) (el formulario entero) y [`middleware::decompress()`](middleware.md#decompress) (el tamaño comprimido y el descomprimido), pero no `take_body()`.
- Funciona igual sobre HTTP/1.1, [HTTP/2](https.md) y [HTTP/3](http3.md).
- Un cuerpo cortado por el `RequestBodyLimitLayer` de tower-http (consulta [Capas de tower](middleware.md#capas-de-tower)) también da un `413`.

### Streaming con `take_body`

Para una subida grande en bruto (no un formulario), o para reenviar un cuerpo a otro sitio, `req.take_body()` te da el cuerpo en bruto como un flujo (`vitesse::Body`), sin cargarlo en memoria. Los datos llegan a medida que el cliente los envía.

> [!WARNING]
> `take_body()` se salta el límite del cuerpo (`app.body_limit`, `middleware::body_limit`), así que limitar el tamaño es cosa tuya. Después, `json()`, `text()` y los demás métodos de lectura ya no pueden leer el cuerpo: fallan con un `500`.

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

`req.set_body(cuerpo)` reemplaza el cuerpo antes de que el handler lo lea. Un middleware puede usarlo para descifrar o transformar el cuerpo ([`middleware::decompress()`](middleware.md#decompress) lo usa para los cuerpos comprimidos). Acepta cualquier cosa que se convierta en `Body`: `String`, `Vec<u8>`, `Bytes`, `&'static str`, etc. El nuevo cuerpo está sujeto al límite del cuerpo igual que el original.

## Subida de archivos (multipart)

Los formularios que envían archivos usan `enctype="multipart/form-data"`. En Express añadirías `multer`; en Vitesse, activa la feature `multipart`:

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["multipart"] }
```

```html
<form method="post" action="/upload" enctype="multipart/form-data">
  <input name="title">
  <input type="file" name="photos" multiple>
  <button>Enviar</button>
</form>
```

`req.multipart()?` abre el formulario, y `form.next_field().await?` devuelve sus campos uno a uno, en el orden en que los envió el navegador, mientras el cuerpo sigue llegando: un archivo nunca se guarda en memoria salvo que lo pidas.

```rust
use vitesse::prelude::*;

async fn upload(req: Request) -> vitesse::Result<String> {
    let mut form = req.multipart()?; // 415 si el cuerpo no es multipart/form-data
    let mut report = Vec::new();
    while let Some(field) = form.next_field().await? {
        let name = field.name().unwrap_or("").to_owned();
        match field.file_name().map(safe_name) {
            // Un archivo: se escribe en disco trozo a trozo, a medida que llega.
            Some(file_name) => {
                let size = field.save(format!("uploads/{file_name}")).await?;
                report.push(format!("{name}: {file_name} ({size} bytes)"));
            }
            // Un campo de texto.
            None => {
                let value = field.text().await?;
                report.push(format!("{name} = {value}"));
            }
        }
    }
    Ok(report.join("\n"))
}

/// El nombre de archivo del cliente, reducido a letras, cifras, `-`, `_` y `.`
/// (sin `/`, sin `..`, sin archivos ocultos).
fn safe_name(client_name: &str) -> String {
    let name: String = client_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    match name.trim_start_matches('.') {
        "" => "upload".to_owned(),
        name => name.to_owned(),
    }
}

let mut app = App::new();
app.post("/upload", upload.with(middleware::body_limit(100 * 1024 * 1024))); // 100 MiB
```

| Método de `Field` | Te da |
|---|---|
| `field.name()` | El `name` del campo (`Option<&str>`) |
| `field.file_name()` | Para un archivo, su nombre en la máquina del cliente (`Option<&str>`); `None` para un campo de texto |
| `field.content_type()` | El tipo que anuncia el navegador (`image/png`…), si lo hay |
| `field.headers()` | Todas las cabeceras de la parte |
| `field.chunk().await?` | El siguiente trozo del contenido (`Option<Bytes>`), `None` al final |
| `field.bytes().await?` | Todo el contenido, en memoria (`Bytes`) |
| `field.text().await?` | Todo el contenido como `String` (`400` si no es UTF-8) |
| `field.save(path).await?` | Escribe el contenido en un archivo a medida que llega (creado, o vaciado si ya existe) y devuelve su tamaño en bytes (`u64`) |

`bytes`, `text` y `save` consumen el campo. Los campos llegan uno tras otro por el mismo flujo, así que lee (o descarta) cada uno antes de pedir el siguiente. Para procesar un archivo sobre la marcha (calcular su hash, enviarlo a otro sitio), léelo trozo a trozo:

```rust
while let Some(mut field) = form.next_field().await? {
    let mut size = 0;
    while let Some(chunk) = field.chunk().await? {
        size += chunk.len(); // calcular un hash, enviarlo a un almacenamiento de objetos…
    }
    println!("{:?}: {size} bytes", field.name());
}
```

- Todo el cuerpo está sujeto al [límite del cuerpo](#límite-de-tamaño-del-cuerpo-y-413), 1 MiB por defecto: súbelo en las rutas de subida con `middleware::body_limit`, como arriba. Por encima, la lectura falla con un `413`.
- Un cuerpo que no es `multipart/form-data` recibe un `415 Unsupported Media Type`, y un formulario mal formado, un `400`.
- `req.multipart()` se queda con el cuerpo: los demás métodos de lectura ya no pueden leerlo después.
- Si la subida falla a medias (`413`, cliente desaparecido), `save` borra el archivo parcial y devuelve el error.
- El ejemplo `examples/live.rs` del repositorio tiene una página de subida completa (`cargo run --example live --features multipart`).

> [!WARNING]
> `file_name()` viene del cliente: puede ser `../../etc/passwd`, contener cualquier carácter o coincidir con el archivo de otro usuario. Nunca lo uses tal cual como ruta. Límpialo como arriba o, mejor, guarda el archivo con un nombre que elijas tú (un id aleatorio, una clave de base de datos) y conserva el nombre original solo como información. `content_type()` también es lo que afirma el cliente: comprueba el contenido si importa.

## Host y protocolo

```rust
app.get("/where", |req: Request| async move {
    format!(
        "{} {} {:?} {:?}",
        req.protocol(), // "https"
        req.secure(),   // true
        req.host(),     // Some("example.com:8443")
        req.hostname(), // Some("example.com")
    )
});
```

| Método | Express | Devuelve |
|---|---|---|
| `req.protocol()` | `req.protocol` | `"https"` sobre TLS o HTTP/3, `"http"` en los demás casos |
| `req.secure()` | `req.secure` | `true` cuando `protocol()` es `"https"` |
| `req.host()` | `req.host` (Express 5) | La cabecera `Host` con su puerto (`example.com:8443`), o la autoridad de una petición HTTP/2 o HTTP/3 |
| `req.hostname()` | `req.hostname` | Lo mismo sin el puerto: `example.com` (`::1` para `[::1]:3000`) |

Detrás de un proxy inverso, la petición llega a Vitesse en HTTP sin cifrar, y el `Host` puede ser un nombre interno. Declara tu proxy con [`app.trust_proxy`](#detrás-de-un-proxy-trust_proxy), y estos métodos leen en su lugar `X-Forwarded-Proto` y `X-Forwarded-Host`.

> [!NOTE]
> El host viene del cliente. No construyas con él enlaces sensibles para la seguridad (el enlace de un correo de restablecimiento de contraseña, por ejemplo): usa una URL base sacada de tu configuración.

## Dirección del cliente

```rust
app.get("/ip", |req: Request| async move {
    match req.ip() {
        Some(ip) => format!("Tu IP: {ip}"),
        None => "desconocida".to_string(),
    }
});
```

- `req.ip()` devuelve la dirección IP del cliente como `Option<IpAddr>`, como `req.ip`. Por defecto, es la dirección del otro extremo de la conexión TCP.
- `req.remote_addr()` devuelve la IP y el puerto de la conexión TCP como `Option<SocketAddr>`, como `req.socket.remoteAddress`. Nunca lee las cabeceras de proxy.
- En un socket Unix (`app.run_unix(...)`, consulta [Sockets de dominio Unix](server.md#sockets-de-dominio-unix)), la conexión no tiene dirección IP: los dos son `None`, salvo que configures `trust_proxy` (abajo), en cuyo caso `req.ip()` sale de `X-Forwarded-For`.

### Detrás de un proxy: `trust_proxy`

Detrás de un proxy inverso o un balanceador de carga (Nginx, Caddy, Heroku, el balanceador de una nube…), el otro extremo de la conexión es el proxy: `req.ip()` sería la dirección del proxy en todas las peticiones, y `req.protocol()` diría `"http"` aunque el visitante use HTTPS. El proxy pasa los valores originales en cabeceras: `X-Forwarded-For` (la dirección del cliente, seguida de los proxies por los que ha pasado), `X-Forwarded-Proto` y `X-Forwarded-Host`. Como con `app.set('trust proxy', …)` en Express, indica a Vitesse en qué proxies confiar:

```rust
let mut app = App::new();
app.trust_proxy("loopback"); // Nginx o Caddy en la misma máquina
```

| Valor | Proxies de confianza | Express |
|---|---|---|
| `false` (por defecto) | ninguno: las cabeceras se ignoran | `false` |
| `"loopback"`, `"10.0.0.0/8, 192.168.1.10"` | los proxies cuya dirección está en estas redes | las mismas cadenas |
| `1`, `2`… | ese número de proxies, los más cercanos al servidor, sea cual sea su dirección | los mismos números |
| `true` | todos: el cliente es la dirección más a la izquierda de `X-Forwarded-For` | `true` |

Las redes son direcciones IP o rangos CIDR, además de tres nombres: `loopback` (`127.0.0.1/8`, `::1`), `linklocal` (`169.254.0.0/16`, `fe80::/10`) y `uniquelocal` (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `fc00::/7`). Combínalos con comas: `"loopback, uniquelocal"`. Una dirección no válida hace que `trust_proxy` entre en pánico al arrancar. Un proxy conectado por un socket Unix, al que solo pueden llegar los procesos de la misma máquina, es de confianza con cualquier valor salvo `false` y `0`.

Cuando la petición viene de un proxy de confianza:

- `req.ip()` recorre `X-Forwarded-For` desde la derecha (el proxy más cercano) y se detiene en la primera dirección que no es un proxy de confianza: ese es el cliente.
- `req.protocol()` y `req.secure()` siguen a `X-Forwarded-Proto: https`.
- `req.host()` y `req.hostname()` usan `X-Forwarded-Host` cuando el proxy la envía.
- `req.remote_addr()` no cambia: sigue siendo la dirección del proxy.

Por ejemplo, con `app.trust_proxy("loopback, 10.0.0.0/8")`, una petición del proxy local (`127.0.0.1`) con `X-Forwarded-For: 203.0.113.9, 10.0.0.2` viene de `203.0.113.9`: `10.0.0.2` es un balanceador de carga interno, de confianza, y `203.0.113.9` no lo es. Los middlewares incluidos también se basan en `req.ip()`: [`middleware::rate_limit`](middleware.md#rate_limit) cuenta entonces por visitante y no por proxy. Las configuraciones de proxy (Nginx, Caddy, Heroku) están en [Puesta en producción](production.md).

> [!WARNING]
> Cualquiera puede enviar cabeceras `X-Forwarded-*`. Confía solo en los proxies que gestionas, y asegúrate de que los clientes no pueden llegar a la aplicación sin pasar por ellos. Evita `true` salvo que tu proxy reemplace `X-Forwarded-For` en lugar de añadirle valores: si no, el cliente elige su propia dirección.

La configuración analizada es un `vitesse::TrustProxy` (`"loopback".parse::<TrustProxy>()`), y `trust.trusts(ip, 0)` indica si una dirección es de confianza, por si necesitas la misma regla en tu propio código.

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
| `set_method(método)` | `method-override` | |
| `path()` | `req.path` | `&str` |
| `uri()` | `req.originalUrl` | `&Uri` |
| `version()` | `req.httpVersion` | `Version` |
| `host()` | `req.host` | `Option<&str>` |
| `hostname()` | `req.hostname` | `Option<&str>` |
| `protocol()` | `req.protocol` | `&'static str` (`"http"` o `"https"`) |
| `secure()` | `req.secure` | `bool` |
| `set_uri(uri)` | asignar `req.url` | |
| `route()` | `req.route.path` | `Option<&'static str>` |
| `path_for(nombre, &[...])` | | `Result<String, Error>` (`500`): la ruta concreta de una ruta con nombre |
| `url_for(nombre, &[...])` | | `Result<String, Error>` (`500`): lo mismo, con protocolo y host |
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
| `accepts(&[...])` | `req.accepts([...])` | `Option<&str>` (`None`: responde `406`) |
| `accepts_language(&[...])` | `req.acceptsLanguages(...)` | `Option<&str>` |
| `cookie(nombre)` | `req.cookies.nombre` | `Option<&str>` |
| `signed_cookie(nombre)` (feature `session`) | `req.signedCookies.nombre` | `Option<String>` (`None` si falta o se modificó) |
| `sign_cookie(cookie)` (feature `session`) | `res.cookie(nombre, valor, { signed: true })` | `Cookie`, para enviar con `.cookie(...)` |
| `session()` (feature `session`) | `req.session` | `&Session` (`500` sin el middleware `session`) |
| `json::<T>().await` | `req.body` + `express.json()` | `Result<T, Error>` |
| `form::<T>().await` | `req.body` + `express.urlencoded()` | `Result<T, Error>` |
| `text().await` | `req.body` + `express.text()` | `Result<String, Error>` |
| `bytes().await` | `req.body` + `express.raw()` | `Result<Bytes, Error>` |
| `multipart()` (feature `multipart`) | `multer` | `Result<Multipart, Error>` (`415`) |
| `take_body()` | `req` (flujo) | `Body` |
| `set_body(cuerpo)` | | |
| `body_limit()` / `set_body_limit(bytes)` | `express.json({ limit })` | `usize` / |
| `ip()` | `req.ip` (sigue a `trust_proxy`) | `Option<IpAddr>` |
| `remote_addr()` | `req.socket.remoteAddress` | `Option<SocketAddr>` |
| `state::<T>()` / `try_state::<T>()` | `req.app.locals` | `&T` / `Option<&T>` |
| `get::<T>()` / `set(valor)` | `res.locals` | `Option<&T>` / `Option<T>` (el valor anterior) |
| `extensions()` / `extensions_mut()` | | `&Extensions` / `&mut Extensions` |
| `render(plantilla, contexto)` (feature `templates`) | `res.render(vista, locals)` | `Result<Html<String>, Error>`, consulta [Plantillas HTML](templates.md) |
