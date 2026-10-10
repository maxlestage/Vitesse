# Middlewares

Un middleware es una función que se ejecuta alrededor de tus handlers: recibe la petición y `next`, el resto de la cadena, y puede actuar antes del handler, después de él, o responder en su lugar. Esta página explica cómo escribirlos, en qué orden se ejecutan, y documenta los middlewares que incluye Vitesse.

## Tu primer middleware

En Express, un middleware recibe `(req, res, next)` y llama a `next()`. En Vitesse, recibe `(req, next)`, pasa la petición al siguiente eslabón con `next.run(req).await`, y **devuelve** la respuesta:

```js
app.use((req, res, next) => {
  console.log(`${req.method} ${req.path}`);
  next();
});
```

```rust
use vitesse::prelude::*;

let mut app = App::new();
app.middleware(|req: Request, next: Next| async move {
    println!("{} {}", req.method(), req.path());
    next.run(req).await
});
```

`next.run(req)` toma posesión de la petición y te devuelve la `Response` que produce el resto de la cadena. Como consume `next`, solo se puede llamar una vez: el compilador descarta los errores del tipo «`next()` llamado dos veces».

## Antes y después del handler

El código antes de `next.run` se ejecuta a la ida, y el código después se ejecuta a la vuelta, con la respuesta en la mano:

```rust
use std::time::Instant;

app.middleware(|req: Request, next: Next| async move {
    let start = Instant::now();
    let res = next.run(req).await;
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    res.header("x-response-time", format!("{ms:.3}ms"))
});
```

Para modificar la respuesta, usa los métodos del builder (`.header(...)`, `.status(...)`) o los métodos «in situ» sobre una respuesta `mut` (`set_header`, `set_status`, `headers_mut()`), descritos en [Responder](responses.md).

## Cortar la cadena

Para detener la cadena, devuelve una respuesta sin llamar a `next.run`:

```rust
app.middleware(|req: Request, next: Next| async move {
    if req.header("x-api-key") != Some("secret") {
        return Error::unauthorized("falta la clave de API").into_response();
    }
    next.run(req).await
});
```

Un middleware puede devolver cualquier cosa que implemente `IntoResponse`, incluido un `vitesse::Result<Response>`, lo que te permite usar `?`:

```rust
async fn require_json(req: Request, next: Next) -> vitesse::Result<Response> {
    if req.method() == Method::POST && !req.is("json") {
        return Err(Error::new(415, "envía JSON"));
    }
    Ok(next.run(req).await)
}

app.middleware(require_json);
```

## Modificar la petición

Recibe la petición como `mut req` para modificarla antes de pasarla:

- `req.set(valor)` adjunta un dato que los handlers leen con `req.get::<T>()` (el equivalente de `res.locals` o `req.user`; el tipo debe implementar `Clone`). Ver [Estado compartido](state.md).
- `req.headers_mut()` da acceso a las cabeceras, y `req.set_body(...)` reemplaza el cuerpo.
- `req.set_uri(...)` reescribe la URL. Los middlewares globales se ejecutan *antes* del enrutamiento, así que se enruta la nueva ruta:

```rust
app.middleware(|mut req: Request, next: Next| async move {
    if req.path().starts_with("/old/") {
        let new = req.uri().to_string().replacen("/old/", "/new/", 1);
        if let Ok(uri) = new.parse() {
            req.set_uri(uri);
        }
    }
    next.run(req).await
});
```

Un middleware también puede leer el cuerpo (`req.bytes().await`, `req.json().await`…), por ejemplo para verificar la firma de un webhook: el cuerpo queda en caché y el handler puede volver a leerlo.

## Orden de ejecución

Los middlewares globales se ejecutan en el orden en que se añadieron, como las capas de una cebolla:

```rust
app.middleware(a);
app.middleware(b);
app.get("/", handler);
// a (ida) → b (ida) → handler → b (vuelta) → a (vuelta)
```

La cadena completa de una petición es: **middlewares globales → middlewares del [router](routers.md) → middlewares de la ruta (`.with`) → handler**.

> [!IMPORTANT]
> A diferencia de `app.use()` en Express, la posición de `app.middleware(...)` respecto a tus rutas no importa: un middleware global envuelve **todas** las peticiones, incluidas las rutas declaradas antes que él, los `404` y los `405`. El manejador [`app.on_error`](errors.md) siempre es la capa más externa.

## Un middleware solo para algunas rutas

Hay tres formas de limitar un middleware a algunas rutas:

- **Una ruta**: `.with(middleware)` (del trait `HandlerExt`, incluido en el preludio). Las llamadas se encadenan y se ejecutan de izquierda a derecha. Pon una closure entre paréntesis antes de llamar a `.with`:

```rust
#[derive(Clone)]
struct CurrentUser {
    name: String,
}

async fn auth(mut req: Request, next: Next) -> Response {
    match req.header("authorization") {
        Some("Bearer secret") => {
            req.set(CurrentUser { name: "ada".into() });
            next.run(req).await
        }
        _ => Error::unauthorized("inicia sesión").into_response(),
    }
}

async fn dashboard(req: Request) -> String {
    let user = req.get::<CurrentUser>().unwrap();
    format!("Hola {}", user.name)
}

app.get("/dashboard", dashboard.with(auth));
app.get("/health", (|_| async { "ok" }).with(auth));
```

- **Un grupo de rutas**: colócalas en un `Router` y llama a `router.middleware(...)` (ver [Routers](routers.md)).
- **Un prefijo de ruta**: comprueba la ruta en un middleware global:

```rust
app.middleware(|req: Request, next: Next| async move {
    if req.path().starts_with("/admin") && req.header("x-admin") != Some("1") {
        return Error::forbidden("solo para administradores").into_response();
    }
    next.run(req).await
});
```

## Middlewares incluidos

El módulo `vitesse::middleware` (accesible como `middleware::` con el preludio) trae los clásicos:

| Middleware | Equivalente en Express | Función |
|---|---|---|
| `middleware::logger()` | `morgan('dev')` | registra cada petición |
| `middleware::cors()` | `cors()` | cabeceras CORS y peticiones preliminares |
| `middleware::helmet()` | `helmet()` | cabeceras de seguridad |
| `middleware::timeout(duración)` | `connect-timeout` | `503` cuando una petición tarda demasiado |
| `middleware::serve_static(carpeta)` | `express.static(carpeta)` | archivos estáticos, ver [Archivos estáticos](static-files.md) |

### `logger`

Escribe una línea por petición en la salida estándar: método, URL, estado y duración. El estado aparece en color cuando la salida es una terminal.

```rust
app.middleware(middleware::logger());
```

```text
GET /users/42 200 0.084 ms
```

Añádelo primero para que mida toda la cadena.

### `cors`

Sin opciones, `middleware::cors()` se comporta como `cors()` en Express: se permiten todos los orígenes (`Access-Control-Allow-Origin: *`) para los métodos `GET, HEAD, PUT, PATCH, POST, DELETE`. Cada opción se encadena:

```rust
use std::time::Duration;

app.middleware(
    middleware::cors()
        .allow_origin("https://app.example.com")
        .allow_origin("https://admin.example.com")
        .allow_methods([Method::GET, Method::POST])
        .allow_headers("content-type, authorization")
        .expose_headers("x-total-count")
        .allow_credentials(true)
        .max_age(Duration::from_secs(600)),
);
```

| Opción | Efecto |
|---|---|
| `allow_origin(origen)` | solo permite los orígenes indicados (se puede llamar varias veces; `"*"` mantiene todos los orígenes) |
| `allow_methods(métodos)` | métodos enviados en `Access-Control-Allow-Methods` |
| `allow_headers(lista)` | `Access-Control-Allow-Headers` (por defecto, repite las cabeceras que pide el navegador) |
| `expose_headers(lista)` | cabeceras de respuesta que JavaScript puede leer |
| `allow_credentials(true)` | añade `Access-Control-Allow-Credentials: true` (cookies y autenticación), para combinar con `allow_origin` |
| `max_age(duración)` | cuánto tiempo puede guardar el navegador en caché la respuesta preliminar |

Cuando el origen de la petición está en la lista, Vitesse lo devuelve en `Access-Control-Allow-Origin` y añade `Vary: Origin`. Sin ningún `allow_origin`, la respuesta es siempre `*`, incluso con `allow_credentials(true)`, exactamente como en Express.

> [!IMPORTANT]
> Los navegadores rechazan las peticiones con credenciales (cookies, `Authorization`) cuando la respuesta es `*`. Para usar cookies o autenticación entre orígenes, enumera explícitamente tus orígenes de confianza con `.allow_origin(...)`, una vez por origen. Vitesse nunca devuelve a propósito un origen cualquiera: eso permitiría que cualquier sitio web leyera las respuestas de un usuario con sesión iniciada.

Las peticiones sin cabecera `Origin` pasan sin cambios. Las peticiones preliminares (`OPTIONS` con `Access-Control-Request-Method`) reciben directamente un `204`, sin llegar a tus rutas. Cuando un origen no está permitido, Vitesse no añade ninguna cabecera CORS y el navegador bloquea la respuesta: CORS protege los navegadores de los usuarios, no sustituye a la autenticación.

> [!TIP]
> Añade `cors()` antes de tu middleware de autenticación: así, las respuestas de error (un `401`, por ejemplo) también llevan las cabeceras CORS, y el JavaScript de tu front-end puede leerlas.

### `helmet`

Añade las cabeceras de seguridad habituales, sin sobrescribir las que tu handler ya haya puesto:

```text
x-content-type-options: nosniff
x-frame-options: SAMEORIGIN
x-dns-prefetch-control: off
x-download-options: noopen
x-permitted-cross-domain-policies: none
x-xss-protection: 0
referrer-policy: no-referrer
cross-origin-opener-policy: same-origin
strict-transport-security: max-age=31536000; includeSubDomains
```

### `timeout`

Cancela las peticiones que superan la duración indicada y responde `503 {"error":"request timed out"}`:

```rust
app.middleware(middleware::timeout(Duration::from_secs(10)));
```

El `Future` del handler se descarta en su siguiente `.await`. El código que bloquea el hilo sin esperar nunca (un cálculo largo, `std::thread::sleep`) no se puede interrumpir.

## Escribir un middleware reutilizable

La forma más sencilla es una `async fn`, como `auth` más arriba. Para un middleware configurable, escribe una función que devuelva `impl Middleware`. El trait `Middleware` no está en el preludio: impórtalo desde `vitesse`.

```rust
use vitesse::Middleware;

fn api_key(expected: &'static str) -> impl Middleware {
    move |req: Request, next: Next| async move {
        if req.header("x-api-key") != Some(expected) {
            return Error::unauthorized("clave de API inválida").into_response();
        }
        next.run(req).await
    }
}

app.middleware(api_key("mi-clave-secreta"));
```

Si la configuración no es `Copy` (un `String`, un `Vec`…), clónala dentro del `Future` de cada petición: `move |req: Request, next: Next| { let value = value.clone(); async move { /* ... */ } }`.

Para tener más control, implementa el trait en una estructura. El middleware vive tanto como la aplicación (`&'static self`), así que su `Future` puede tomar prestados sus campos sin `Arc` ni clones:

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use vitesse::{BoxFuture, Middleware};

struct RequestCounter {
    total: AtomicU64,
}

impl Middleware for RequestCounter {
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response> {
        Box::pin(async move {
            let n = self.total.fetch_add(1, Ordering::Relaxed) + 1;
            next.run(req).await.header("x-request-number", n)
        })
    }
}

app.middleware(RequestCounter { total: AtomicU64::new(0) });
```

`next.run(req)` ya devuelve un `BoxFuture<Response>`: cuando no necesitas tocar la respuesta, devuélvelo directamente, sin `Box::pin`.
