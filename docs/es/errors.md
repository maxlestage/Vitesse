# Manejo de errores

En Vitesse, los errores son valores normales: un handler devuelve `Err(...)`, casi siempre mediante el operador `?`, y el error se convierte en una respuesta HTTP con el estado adecuado. Esta página cubre el tipo `Error`, las conversiones automáticas, las páginas de error personalizadas, los `404` y los pánicos.

## El tipo `Error`

`vitesse::Error` lleva tres cosas:

- un **estado** HTTP;
- un **mensaje**, que se envía al cliente en JSON: `{"error": "mensaje"}`;
- opcionalmente, una **fuente**: el error original, que se escribe en los logs del servidor pero nunca se envía al cliente.

| Constructor | Estado |
|---|---|
| `Error::bad_request(msg)` | `400 Bad Request` |
| `Error::unauthorized(msg)` | `401 Unauthorized` |
| `Error::forbidden(msg)` | `403 Forbidden` |
| `Error::not_found(msg)` | `404 Not Found` |
| `Error::conflict(msg)` | `409 Conflict` |
| `Error::payload_too_large()` | `413 Payload Too Large` |
| `Error::unprocessable(msg)` | `422 Unprocessable Entity` |
| `Error::internal(msg)` | `500 Internal Server Error` |
| `Error::new(estado, msg)` | cualquier estado: `Error::new(418, "soy una tetera")` |
| `Error::from_status(estado)` | cualquier estado, con su frase estándar como mensaje (`Service Unavailable`) |

`.with_source(err)` adjunta la causa original. En un error existente, `status()`, `message()` y `source()` leen sus partes, y `Error` implementa `Display` (`500 Internal Server Error (causa)`) para tus logs.

## Devolver errores desde un handler

Haz que tu handler devuelva `vitesse::Result<T>`, un alias de `Result<T, vitesse::Error>`, y usa `?`:

```rust
use vitesse::prelude::*;

// `find_item` es tu propia función, que devuelve un `Option<Item>`.
async fn show_item(req: Request) -> vitesse::Result<Json<Item>> {
    let id: u64 = req.param_as("id")?; // 400 si `id` no es un número
    let item = find_item(id).ok_or_else(|| Error::not_found(format!("artículo {id} no encontrado")))?;
    Ok(Json(item))
}
```

```http
GET /items/42 HTTP/1.1

HTTP/1.1 404 Not Found
content-type: application/json

{"error":"artículo 42 no encontrado"}
```

En una closure, Rust no puede adivinar el tipo de error: anota el `Ok` final.

```rust
app.get("/double/:n", |req: Request| async move {
    let n: i64 = req.param_as("n")?;
    Ok::<_, Error>(format!("{}", n * 2))
});
```

También puedes devolver un `Error` directamente (`async { Error::new(418, "soy una tetera") }`), y devolver `None` desde un handler que devuelve un `Option` produce un `404 {"error":"Not Found"}`.

Comparado con Express, no hay `next(err)` ni excepciones que capturar: el error viaja en el valor de retorno, y el compilador se asegura de que se trate.

## `?` con otros tipos de error

Cualquier error estándar (`std::error::Error + Send + Sync + 'static`: errores de E/S, de parseo, de `serde_json`, de drivers de bases de datos…) se convierte automáticamente con `?`. Se transforma en un `500 {"error":"Internal Server Error"}`: el detalle **no** se envía al cliente (podría filtrar información sensible), sino que se escribe en la salida de error del servidor:

```rust
app.get("/parse", |_| async {
    let n: u32 = "abc".parse()?; // ParseIntError -> 500
    Ok::<_, Error>(n.to_string())
});
```

```text
[vitesse] error 500: invalid digit found in string
```

Cuando el cliente merece un mensaje mejor, convierte tú mismo el error con `map_err`:

```rust
app.get("/config", |_| async {
    let text = std::fs::read_to_string("config.toml")
        // 500 con un mensaje claro; el error de E/S se guarda como fuente y se registra.
        .map_err(|e| Error::internal("configuración no disponible").with_source(e))?;
    let port: u16 = text.trim().parse()
        // 400: la culpa es del cliente... en este ejemplo.
        .map_err(|_| Error::bad_request("se esperaba un número de puerto"))?;
    Ok::<_, Error>(format!("puerto {port}"))
});
```

> [!NOTE]
> `Box<dyn std::error::Error + Send + Sync>` y `anyhow::Error` no implementan `std::error::Error`, así que `?` no puede convertirlos. Pero `with_source` sí los acepta: `.map_err(|e| Error::internal("...").with_source(e))?`.

### Errores que produce Vitesse

| Situación | Respuesta |
|---|---|
| `req.param_as` falla | `400`: `invalid parameter 'id': 'abc'` o `missing parameter 'id'` |
| `req.query_as` falla | `400`: `invalid query string: ...` |
| `req.json` / `req.form` con un cuerpo inválido | `400`: `invalid JSON: ...` / `invalid form data: ...` |
| `req.text` con un cuerpo que no es UTF-8 | `400`: `the body is not valid UTF-8` |
| Cuerpo mayor que `app.body_limit` | `413 Payload Too Large` |
| Ninguna ruta coincide | `404`, `{"error":"Cannot GET /ruta"}` |
| La ruta existe, pero no para este método | `405 Method Not Allowed`, con una cabecera `Allow` |
| Un handler entra en pánico | `500 Internal Server Error` |
| Vence `middleware::timeout` | `503`, `request timed out` |

## Tus propios tipos de error

Para usar `?` con tus propios errores de negocio, haz que se correspondan con un `vitesse::Error`. Hay dos enfoques, según tu tipo implemente o no `std::error::Error`.

**Tu tipo no implementa `std::error::Error`**: implementa `From<TuError> for vitesse::Error`, y `?` hace la conversión:

```rust
#[derive(Debug)]
enum ShopError {
    OutOfStock(u64),
    InvalidQuantity,
    Database(std::io::Error),
}

impl From<ShopError> for Error {
    fn from(err: ShopError) -> Self {
        match err {
            ShopError::OutOfStock(id) => Error::conflict(format!("artículo {id} agotado")),
            ShopError::InvalidQuantity => Error::unprocessable("cantidad inválida"),
            ShopError::Database(e) => Error::from_status(500).with_source(e),
        }
    }
}

// `reserve` es tu lógica de negocio: fn reserve(id: u64) -> Result<(), ShopError>
async fn order(req: Request) -> vitesse::Result<&'static str> {
    let id: u64 = req.param_as("id")?;
    reserve(id)?; // ShopError -> vitesse::Error
    Ok("reservado")
}
```

**Tu tipo implementa `std::error::Error`** (a mano o con `thiserror`): el `From` anterior entraría en conflicto con la conversión automática (que convierte cualquier error estándar en un `500`). Implementa `IntoResponse` en su lugar y devuelve `Result<T, TuError>`:

```rust
use std::fmt;

#[derive(Debug)]
enum ApiError {
    InvalidId,
    NotFound(u64),
    Io(std::io::Error),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::InvalidId => write!(f, "identificador inválido"),
            ApiError::NotFound(id) => write!(f, "nota {id} no encontrada"),
            ApiError::Io(e) => write!(f, "error de E/S: {e}"),
        }
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let err = match self {
            ApiError::InvalidId => Error::bad_request("identificador inválido"),
            ApiError::NotFound(id) => Error::not_found(format!("nota {id} no encontrada")),
            other => Error::from_status(500).with_source(other),
        };
        err.into_response()
    }
}

async fn read_note(req: Request) -> Result<String, ApiError> {
    let id: u64 = req.param("id").and_then(|s| s.parse().ok()).ok_or(ApiError::InvalidId)?;
    match tokio::fs::read_to_string(format!("notes/{id}.txt")).await {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(ApiError::NotFound(id)),
        Err(e) => Err(ApiError::Io(e)),
    }
}
```

> [!TIP]
> Construye la respuesta a partir de un `vitesse::Error`, como arriba, en lugar de hacerlo a mano: así conserva el formato JSON habitual, la fuente se registra y pasa por `app.on_error`.

## Páginas de error personalizadas: `app.on_error`

Por defecto, los errores se envían como `{"error": "mensaje"}`. `app.on_error` reemplaza ese formato en toda la aplicación, como el middleware de error `(err, req, res, next)` de Express:

```rust
app.on_error(|err: Error| {
    if let Some(source) = err.source() {
        eprintln!("{} {}: {source}", err.status().as_u16(), err.message());
    }
    res::status(err.status()).json(json!({
        "error": { "status": err.status().as_u16(), "message": err.message() }
    }))
});
```

La función recibe el `Error` y devuelve cualquier cosa que implemente `IntoResponse`. Se llama para **cada respuesta que procede de un `Error`**: errores devueltos por handlers y middlewares, el `404` por defecto, los `405`, los errores al leer el cuerpo, los `413`, los pánicos, los tiempos de espera agotados y los archivos estáticos que no existen. No se aplica a las respuestas que construyes tú, como `(404, "aquí no está")` o `StatusCode::NOT_FOUND`. Las cabeceras que ponen tus middlewares (CORS, por ejemplo) se conservan.

> [!WARNING]
> Fija el estado tú mismo, como en `res::status(err.status())`: si devuelves solo `Html(...)`, la página de error sale con un `200 OK`. Además, en cuanto defines `on_error`, la fuente de los `500` ya no se registra automáticamente: regístrala en tu manejador, como arriba.

`on_error` no recibe la petición. Si tu formato depende de ella (HTML para los navegadores, JSON para todo lo demás), escribe un middleware global que inspeccione la respuesta con `res.take_error()`:

```rust
app.middleware(|req: Request, next: Next| async move {
    let wants_html = req.header("accept").is_some_and(|a| a.contains("text/html"));
    let mut res = next.run(req).await;
    if wants_html {
        if let Some(err) = res.take_error() {
            return res::status(err.status())
                .html(format!("<h1>{}</h1><p>{}</p>", err.status(), err.message()));
        }
    }
    res
});
```

## 404 personalizado: `app.fallback`

Cuando ninguna ruta coincide, Vitesse llama al handler de respaldo, que por defecto responde `404 {"error":"Cannot GET /ruta"}`. Reemplázalo con `app.fallback`, que recibe un handler normal:

```rust
app.fallback(|req: Request| async move {
    (404, Html(format!("<h1>Página no encontrada</h1><p>{} no existe.</p>", req.path())))
});
```

Los middlewares globales también se ejecutan para el handler de respaldo. No se llama cuando la ruta existe con otro método (eso es un `405`). Si devuelve un `Error`, la respuesta pasa por `on_error`. Para aplicaciones de una sola página, consulta [Archivos estáticos](static-files.md).

## Pánicos

Un pánico en un handler (un `unwrap()` sobre `None`, un índice fuera de rango, `req.state::<T>()` con un tipo que nunca se registró…) no tumba el servidor: Vitesse lo captura y responde `500 {"error":"Internal Server Error"}`. Rust escribe el mensaje del pánico en la salida de error, las demás peticiones siguen con normalidad, y el `500` pasa por tus middlewares y por `on_error` como cualquier otro error.

Aun así, prefiere `?` y errores explícitos: un pánico es un bug, no una forma de responder.

> [!WARNING]
> Los pánicos solo se pueden capturar con la estrategia por defecto (*unwinding*). Con `panic = "abort"` en un `[profile]` de tu `Cargo.toml`, un pánico detiene todo el proceso. Además, un pánico dentro de un *middleware* se captura en lo más alto de la cadena: el cliente recibe igualmente un `500`, pero los middlewares exteriores y `on_error` no se ejecutan para esa petición.
