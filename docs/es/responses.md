# Responder

En Vitesse, un handler no escribe en un objeto `res`: **devuelve** su respuesta. Puedes devolver cualquier cosa que implemente el trait `IntoResponse`: una cadena, `Json(...)`, una tupla `(estado, cuerpo)`, un `Result`, o una `Response` completa construida con los helpers `res`, inspirados en Express.

## Devuelve tu respuesta

En Express llamas a un método de `res`. En Vitesse, el valor que devuelve el handler *es* la respuesta:

```js
app.get('/', (req, res) => res.send('Hello World!'));
app.post('/users', (req, res) => res.status(201).json({ id: 1 }));
```

```rust
use vitesse::prelude::*;

let mut app = App::new();
app.get("/", |_| async { "Hello World!" });
app.post("/users", |_| async { (201, Json(json!({ "id": 1 }))) });
```

El compilador comprueba que cada rama del código produzca una respuesta: no puedes olvidarte de responder, y el error «headers already sent» de Express no existe.

## Qué puede devolver un handler

| Tipo devuelto | Estado | `Content-Type` |
|---|---|---|
| `&'static str`, `String`, `Cow<'static, str>` | `200` | `text/plain; charset=utf-8` |
| `Json(valor)` (cualquier tipo `Serialize`) | `200` | `application/json` |
| `serde_json::Value` (la macro `json!`) | `200` | `application/json` |
| `Html(cuerpo)` | `200` | `text/html; charset=utf-8` |
| `Bytes`, `Vec<u8>`, `&'static [u8]` | `200` | `application/octet-stream` |
| `()` | `200` | ninguno (cuerpo vacío) |
| `StatusCode` | ese estado | `text/plain`, con la frase del estado como cuerpo (`Not Found`) |
| `(estado, T)` | `estado` | el de `T` |
| `Option<T>` | `T`, o `404` si es `None` | |
| `Result<T, E>` | `T` o `E` (ambos implementan `IntoResponse`) | |
| `Error` | el estado del error | `application/json`: `{"error": "..."}` |
| `Redirect` | `301`, `302`, `303` o `307` | ninguno, cabecera `Location` |
| `Body` | `200` | ninguno (ver [Streaming](#streaming)) |
| `Sse` (de `sse::channel` o `Sse::new`) | `200` | `text/event-stream` (ver [Server-Sent Events](#server-sent-events)) |
| `Response` | lo que hayas construido | |
| `http::Response<Body>` | sin cambios | |

```rust
use serde::Serialize;
use vitesse::prelude::*;

#[derive(Serialize)]
struct User {
    id: u32,
    name: String,
}

let mut app = App::new();
app.get("/text", |_| async { "¡Hola!" });
app.get("/sum", |_| async { format!("1 + 1 = {}", 1 + 1) });
app.get("/user", |_| async { Json(User { id: 1, name: "Ada".into() }) });
app.get("/stats", |_| async { json!({ "users": 42, "online": true }) });
app.get("/page", |_| async { Html("<h1>Bienvenido</h1>") });
app.get("/ping", |_| async { StatusCode::NO_CONTENT });
```

> [!NOTE]
> La macro `json!` y `serde_json` vienen reexportadas por Vitesse (`vitesse::json`, `vitesse::serde_json`): no necesitas añadir `serde_json` a tu `Cargo.toml`. Para derivar `Serialize` en tus propios tipos, añade `serde = { version = "1", features = ["derive"] }`.

### Códigos de estado

Una tupla `(estado, cuerpo)` cambia el estado de cualquier respuesta. El estado puede ser un número o una constante `StatusCode`:

```rust
app.post("/users", |_| async { (201, Json(json!({ "id": 2 }))) });
app.post("/jobs", |_| async { (StatusCode::ACCEPTED, "en cola") });
```

Devolver un `StatusCode` solo envía el estado con su frase como texto, igual que `res.sendStatus(404)` en Express. Un código inválido (como `1000`) se convierte en un `500`.

### `Option` y `Result`

`None` se convierte en un `404 {"error":"Not Found"}`. Un `Result` envía el valor o el error: la mayoría de los handlers devuelven un `vitesse::Result<T>` y usan `?`, como se explica en [Manejo de errores](errors.md).

```rust
// `find_user` es tu propia función, que devuelve un `Option<User>`.
async fn show_user(req: Request) -> vitesse::Result<Json<User>> {
    let id: u32 = req.param_as("id")?; // 400 si no es un número
    let user = find_user(id).ok_or_else(|| Error::not_found("usuario no encontrado"))?;
    Ok(Json(user))
}
```

## El builder `res`

Cuando necesitas controlarlo todo (estado, cabeceras, cookies y cuerpo a la vez), construye una `Response`. El módulo `res` ofrece puntos de entrada al estilo de Express, y cada uno devuelve una `Response` que puedes seguir encadenando:

```rust
app.get("/custom", |_| async {
    res::status(202)
        .header("x-powered-by", "Vitesse")
        .cookie(Cookie::new("visto", "1").http_only(true))
        .json(json!({ "ok": true }))
});
```

| Express | Vitesse |
|---|---|
| `res.status(201)` | `res::status(201)` o `.status(201)` |
| `res.send(cuerpo)` | `res::send(cuerpo)` o `.send(cuerpo)` |
| `res.json(obj)` | `res::json(obj)` o `.json(obj)` |
| `res.type('text/csv')` | `.content_type("text/csv")` |
| `res.set(nombre, valor)` | `.header(nombre, valor)` |
| `res.append(nombre, valor)` | `.append_header(nombre, valor)` |
| `res.cookie(...)` / `res.clearCookie(nombre)` | `.cookie(Cookie::new(...))` / `.clear_cookie(nombre)` |
| `res.attachment(nombre)` | `.attachment(nombre)` |
| `res.redirect(url)` | `res::redirect(url)` |
| `res.sendStatus(404)` | `res::send_status(404)` |
| `res.sendFile(ruta)` | `res::file(ruta).await` |
| `res.download(ruta, nombre)` | `res::download(ruta, nombre).await` |

También existen `res::text(...)` / `.text(...)` y `res::html(...)` / `.html(...)`, y `Response::new()` te da un `200` vacío como punto de partida. Cada método recibe la respuesta y la devuelve, así que el orden es libre. Solo recuerda que `.text()`, `.html()` y `.json()` fijan el `Content-Type` (reemplazando un `.content_type(...)` anterior), mientras que `.send()` solo fija el cuerpo.

> [!WARNING]
> A diferencia de `res.send()` en Express, `send` nunca adivina el `Content-Type`: `res::send("hola")` sale sin él. Usa `text`, `html` o `json`, o añade `.content_type(...)`.

### Cabeceras

`.header(nombre, valor)` fija una cabecera (reemplazando el valor anterior) y `.append_header(nombre, valor)` añade un valor más. Los nombres pueden ser cadenas o las constantes de `vitesse::header`; los valores, `&str`, `String` o enteros. Un nombre o un valor inválido (un salto de línea, por ejemplo) se ignora en silencio.

```rust
use vitesse::header;

app.get("/report", |_| async {
    Response::new()
        .header(header::CACHE_CONTROL, "no-store")
        .header("x-total-count", 42)
        .append_header("vary", "accept")
        .append_header("vary", "accept-language")
        .text("...")
});
```

Para modificar una respuesta que ya tienes (normalmente en un [middleware](middleware.md)), usa las versiones «in situ»: `set_status`, `set_header`, `headers_mut()`, y para leerla, `status_code()` y `get_header(nombre)`.

### Tipo de contenido

`text`, `html` y `json` cubren los casos habituales. Para todo lo demás, fíjalo tú:

```rust
app.get("/export.csv", |_| async {
    res::send("id,nombre\n1,Ada\n").content_type("text/csv; charset=utf-8")
});
```

### Cookies

```rust
use std::time::Duration;
use vitesse::SameSite;

app.post("/login", |_| async {
    let session = Cookie::new("session", "abc123")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::from_secs(7 * 24 * 3600));
    Response::new().cookie(session).json(json!({ "ok": true }))
});

app.post("/logout", |_| async { res::redirect("/").clear_cookie("session") });
```

La ruta de inicio de sesión envía `Set-Cookie: session=abc123; Path=/; Max-Age=604800; HttpOnly; Secure; SameSite=Lax`.

| Método de `Cookie` | Atributo |
|---|---|
| `Cookie::new(nombre, valor)` | `nombre=valor; Path=/` |
| `.path("/admin")` | `Path` |
| `.domain("example.com")` | `Domain` |
| `.max_age(Duration)` | `Max-Age` (en segundos) |
| `.secure(true)` | `Secure` |
| `.http_only(true)` | `HttpOnly` |
| `.same_site(SameSite::Strict / Lax / None)` | `SameSite` |

Cada llamada a `.cookie(...)` añade su propia cabecera `Set-Cookie`, así que puedes enviar varias. `.clear_cookie(nombre)` envía `nombre=; Path=/; Max-Age=0`: si la cookie se creó con otra ruta u otro dominio, construye tú mismo la cookie de borrado con los mismos atributos y `.max_age(Duration::ZERO)`. Para leer las cookies que envía el navegador, usa `req.cookie("session")` (ver [Leer la petición](requests.md)).

> [!IMPORTANT]
> El valor se escribe tal cual: no se firma, ni se cifra, ni se codifica. Mantenlo compatible con una URL (un token de sesión aleatorio, por ejemplo) y guarda los datos sensibles en el servidor. Los navegadores solo aceptan `SameSite::None` junto con `.secure(true)`.

### Redirecciones

| Helper | Estado |
|---|---|
| `Redirect::to(url)` o `res::redirect(url)` | `302 Found` (el valor por defecto de Express) |
| `Redirect::permanent(url)` | `301 Moved Permanently` |
| `Redirect::see_other(url)` | `303 See Other` (después de un `POST` de formulario) |
| `Redirect::temporary(url)` | `307 Temporary Redirect` (conserva el método y el cuerpo) |

```rust
app.get("/pagina-antigua", |_| async { Redirect::permanent("/pagina-nueva") });
app.post("/contacto", |_| async { Redirect::see_other("/gracias") });
```

Para otro estado, pon la cabecera tú mismo: `res::status(308).header("location", "/v2")`. Una URL con caracteres inválidos (como un salto de línea) produce un `500`.

## Archivos y descargas

```rust
app.get("/terminos", |_| async { res::file("legal/terminos.pdf").await });
app.get("/factura", |_| async {
    res::download("archivos/factura-42.pdf", "factura.pdf").await
});
```

`res::file` adivina el `Content-Type` a partir de la extensión, envía los archivos grandes por partes, y responde `404 {"error":"Not Found"}` si el archivo no existe. Como Express, también envía `ETag` y `Last-Modified`, responde `304 Not Modified` cuando el navegador ya tiene el archivo, y `206 Partial Content` a las peticiones `Range` de los reproductores de vídeo y los gestores de descargas (consulta [Archivos estáticos](static-files.md#peticiones-parciales)). `res::download` añade además `Content-Disposition: attachment` para que el navegador guarde el archivo con el nombre indicado (se admiten nombres no ASCII). `.attachment("export.csv")` hace lo mismo con cualquier respuesta, algo práctico para contenido generado. Las rutas relativas parten de la carpeta desde la que se lanzó el servidor. Para servir una carpeta entera, consulta [Archivos estáticos](static-files.md).

## Streaming

`Body::from_stream(flujo)` envía cada elemento de un flujo en cuanto se produce, con `Transfer-Encoding: chunked` en HTTP/1.1. El flujo produce valores `Result<D, E>`, donde `D` se convierte en bytes (`String`, `&'static str`, `Vec<u8>`, `Bytes`) y `E` es un tipo de error. Los flujos suelen venir de crates como `tokio-stream` o `futures-util`. Aquí tienes una exportación CSV grande, producida por una tarea y enviada sobre la marcha:

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
```

```rust
use tokio_stream::wrappers::ReceiverStream;
use vitesse::prelude::*;

app.get("/export.csv", |_| async {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<String, std::io::Error>>(16);
    tokio::spawn(async move {
        let _ = tx.send(Ok("id,nombre\n".to_string())).await;
        for id in 1..=100_000 {
            // En una aplicación real: filas leídas de la base de datos, página a página.
            if tx.send(Ok(format!("{id},usuario {id}\n"))).await.is_err() {
                break; // el cliente se ha ido
            }
        }
    });
    Response::new()
        .content_type("text/csv; charset=utf-8")
        .attachment("users.csv")
        .send(Body::from_stream(ReceiverStream::new(rx)))
});
```

Para enviar eventos a una página del navegador (notificaciones, progreso, datos en directo), no escribas el formato `text/event-stream` a mano: usa los [Server-Sent Events](#server-sent-events), justo debajo.

`Body::wrap(cuerpo)` acepta cualquier [`http_body::Body`](https://docs.rs/http-body), y `req.take_body()` te entrega el cuerpo de la petición como un `Body` que puedes devolver tal cual:

```rust
app.post("/echo", |req: Request| async move { req.take_body() });
```

> [!TIP]
> Si conoces de antemano el tamaño total de un flujo, indícalo con `.header("content-length", tamaño)`: la respuesta se envía entonces con esa longitud en lugar de la codificación `chunked`.

## Server-Sent Events

Los Server-Sent Events (SSE) mantienen una respuesta abierta y envían eventos al navegador a medida que ocurren: notificaciones, una barra de progreso, un panel en directo, las palabras de la respuesta de un chatbot. Cuando solo habla el servidor, es más sencillo que un [WebSocket](websocket.md): es HTTP normal, y el navegador lo recibe con `EventSource`, que se reconecta solo. El módulo `sse` (en el preludio, sin feature) se encarga del formato y de las cabeceras.

### Con un canal

La forma más sencilla es `sse::channel(buffer)`: devuelve un emisor y la respuesta. El handler devuelve la respuesta de inmediato, y una tarea envía los eventos:

```rust
use std::time::Duration;
use vitesse::prelude::*;
use vitesse::sse::Event;

app.get("/clock", |_| async {
    let (tx, stream) = sse::channel(16);
    tokio::spawn(async move {
        for seconds in 0.. {
            let event = Event::new().event("tick").data(format!("{seconds} s"));
            if tx.send(event).await.is_err() {
                break; // el navegador se ha ido
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    stream
});
```

La respuesta sale con `content-type: text/event-stream` y `cache-control: no-cache`, y cada evento sale en cuanto se envía:

```text
event: tick
data: 0 s

event: tick
data: 1 s

```

- `buffer` es el número de eventos que pueden esperar mientras la conexión va lenta; cuando está lleno, `send` espera.
- La respuesta termina cuando se descartan todos los `Sender`: aquí, cuando termina la tarea. `Sender` es `Clone`, así que varias tareas pueden alimentar la misma respuesta.
- `tx.try_send(evento)` envía sin esperar. Falla con `sse::TrySendError::Full` cuando el buffer está lleno (una conexión lenta: sáltate el evento, o envíalo más tarde) y con `TrySendError::Closed` cuando el navegador se ha ido; en los dos casos te devuelve el evento. `tx.is_closed()` te dice si el navegador sigue ahí.

### Desde un flujo

`Sse::new(flujo)` acepta cualquier `Stream` de `Result<Event, E>`, de tokio-stream, futures-util o tu propio código. La respuesta termina con el flujo:

```rust
use std::convert::Infallible;
use vitesse::sse::{Event, Sse};

app.get("/countdown", |_| async {
    let events = (1..=3).rev().map(|n| Ok::<_, Infallible>(Event::new().data(n.to_string())));
    Sse::new(futures_util::stream::iter(events))
});
```

Un `Err` del flujo corta la conexión: el navegador ve un error de red y se reconecta.

### Eventos

| Método de `Event` | Línea enviada | En el navegador |
|---|---|---|
| `Event::new().data(texto)` | `data: …` (una línea por cada línea de `texto`, que el navegador vuelve a unir) | `e.data` |
| `.json_data(&valor)?` | `data: …`, `valor` serializado a JSON | `JSON.parse(e.data)` |
| `.event("tick")` | `event: tick` | el listener que lo recibe: `addEventListener("tick", …)` (`message` sin él) |
| `.id("42")` | `id: 42` | se devuelve en `Last-Event-ID` cuando el navegador se reconecta |
| `.retry(Duration::from_secs(5))` | `retry: 5000` | cuánto espera el navegador antes de reconectarse |
| `.comment("…")` | `: …` | se ignora |

`json_data` devuelve un `Result` porque la serialización puede fallar (un `500` con `?`). `event` e `id` entran en pánico si contienen un salto de línea, que rompería el formato. `event.to_string()` devuelve el texto que se enviará, algo práctico en las pruebas.

### Keep-alive

Los proxies y los balanceadores de carga cierran las conexiones que se quedan en silencio demasiado tiempo, a menudo al cabo de un minuto. Por eso, cuando no se ha enviado ningún evento durante 15 segundos, tanto `sse::channel` como `Sse::new` envían un comentario (`:`), que el navegador ignora. Para cambiarlo, o desactivarlo con `.no_keep_alive()`:

```rust
use vitesse::sse::KeepAlive;

let (tx, stream) = sse::channel(16);
// … lanza la tarea que usa `tx`
stream.keep_alive(KeepAlive::new().interval(Duration::from_secs(30)).text("ping"))
```

### Cuando el navegador se va

Cuando el usuario cierra la pestaña, la conexión se cierra y Vitesse descarta la respuesta. A partir de ahí, `tx.send(...)` devuelve un error: detén la tarea en ese punto, como en el reloj de arriba. Vitesse se da cuenta de la marcha la siguiente vez que escribe en la conexión: un evento o, como muy tarde, el siguiente comentario de keep-alive.

Una tarea que envía poco tardaría mucho en enterarse. Haz que espere a la vez a `tx.closed()`, con `tokio::select!`. Aquí, cada página recibe las noticias publicadas en un canal `broadcast` guardado en el [estado](state.md):

```rust
use tokio::sync::broadcast;

let (news, _) = broadcast::channel::<String>(100);
app.state(news);

app.get("/news", |req: Request| async move {
    let mut inbox = req.state::<broadcast::Sender<String>>().subscribe();
    let (tx, stream) = sse::channel(16);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tx.closed() => break, // el navegador se ha ido
                message = inbox.recv() => match message {
                    Ok(text) => { let _ = tx.send(Event::new().data(text)).await; }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                },
            }
        }
    });
    stream
});
```

### En el navegador

```js
const source = new EventSource("/clock");
source.addEventListener("tick", (e) => console.log("tick", e.data));
source.onmessage = (e) => console.log(e.data); // eventos sin tipo `event`
```

`EventSource` se reconecta solo cuando se corta la conexión, y también cuando termina la respuesta. Al reconectarse, envía el `id` del último evento que recibió en una cabecera `Last-Event-ID`: léela para enviar lo que la página se ha perdido.

```rust
app.get("/feed", |req: Request| async move {
    let last: u64 = req.header("last-event-id").and_then(|id| id.parse().ok()).unwrap_or(0);
    let (tx, stream) = sse::channel(16);
    tokio::spawn(async move {
        for n in last + 1.. {
            let event = Event::new().id(n.to_string()).data(format!("mensaje {n}"));
            if tx.send(event).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    stream
});
```

Para parar del todo, llama a `source.close()` en JavaScript (tras un último evento que avise a la página de que se acabó, por ejemplo), o responde a la reconexión con `204 No Content`.

### A tener en cuenta

- SSE funciona igual sobre HTTP/1.1, [HTTP/2](https.md) y [HTTP/3](http3.md). En HTTP/1.1, los navegadores abren como máximo 6 conexiones por sitio, y cada `EventSource` mantiene una ocupada: con muchas pestañas abiertas, HTTP/2 o HTTP/3, que comparten una sola conexión, evitan ese límite.
- `middleware::compress()` no toca los flujos de eventos, y `middleware::timeout(...)` solo limita el tiempo que se tarda en devolver la respuesta, no lo que dura el flujo.
- Nginx guarda por defecto las respuestas en un buffer, lo que retiene los eventos: añade `proxy_buffering off;` a la `location`, o envía la cabecera desde Vitesse con `stream.into_response().header("x-accel-buffering", "no")`.
- `examples/live.rs`, en el repositorio, muestra un reloj en directo en una página (`cargo run --example live --features multipart`).

## `HEAD`, `204` y `304`

No tienes que hacer nada en estos casos: una petición `HEAD` usa la ruta `GET` y el motor envía las cabeceras (incluida `Content-Length`) sin el cuerpo, y las respuestas `204 No Content`, `304 Not Modified` y `1xx` nunca llevan cuerpo.

## Tus propios tipos

Implementa `IntoResponse` para devolver tus propios tipos directamente desde los handlers:

```rust
struct Csv(String);

impl IntoResponse for Csv {
    fn into_response(self) -> Response {
        res::send(self.0).content_type("text/csv; charset=utf-8")
    }
}

app.get("/export", |_| async { Csv("id,nombre\n1,Ada\n".into()) });
```

La misma técnica convierte tus propios tipos de error en respuestas (ver [Manejo de errores](errors.md)). Por último, si trabajas con el crate [`http`](https://docs.rs/http), `Response::from_http` y `Response::into_http` convierten en ambos sentidos, y un handler puede devolver directamente una `http::Response<Body>`.
