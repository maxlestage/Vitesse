# Middlewares

Un middleware es una función que se ejecuta alrededor de tus handlers: recibe la petición y `next`, el resto de la cadena, y puede actuar antes del handler, después de él, o responder en su lugar. Esta página explica cómo escribirlos, en qué orden se ejecutan, documenta los middlewares que incluye Vitesse y muestra cómo usar las capas de tower del ecosistema de axum.

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
- `req.headers_mut()` da acceso a las cabeceras, `req.set_body(...)` reemplaza el cuerpo, y `req.set_body_limit(...)` cambia el tamaño máximo del cuerpo que puede leer el handler (ver [Límite de tamaño del cuerpo](requests.md#límite-de-tamaño-del-cuerpo-y-413)).
- `req.set_method(...)` cambia el método, para [sobrescribir el método](requests.md#sobrescribir-el-método).
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

La cadena completa de una petición es: **middlewares globales → middlewares del [router](routers.md) → middlewares de la ruta (`.with`) → handler**. Los middlewares de un router también se ejecutan, después de los globales, para las peticiones bajo su prefijo a las que no responde ninguna ruta: el `404`, el `405` y el `OPTIONS` automático (consulta [Routers](routers.md#peticiones-a-las-que-no-responde-ninguna-ruta)).

> [!IMPORTANT]
> A diferencia de `app.use()` en Express, la posición de `app.middleware(...)` respecto a tus rutas no importa: un middleware global envuelve **todas** las peticiones, incluidas las rutas declaradas antes que él, los `404` y los `405`. El manejador [`app.on_error`](errors.md) siempre es la capa más externa.

## Hooks del ciclo de vida

Los hooks son funciones simples que se ejecutan en un momento dado de cada petición, como los hooks de Fastify. No devuelven un future, lo que los hace más ligeros que un middleware para lo que hacen: leer o modificar la petición o la respuesta.

| Hook | Se ejecuta | Puede |
|---|---|---|
| `app.on_request(\|req: &mut Request\| …)` | Primero, antes de los middlewares y del enrutamiento (`onRequest` de Fastify) | Modificar la petición; detenerla con `Err(error)` |
| `app.pre_handler(\|req: &mut Request\| …)` | Después del enrutamiento, justo antes de los middlewares y el handler de la ruta (`preHandler`): se conocen `req.route()` y los parámetros | Igual |
| `app.on_response(\|res: &mut Response\| …)` | En cada respuesta, antes de enviarla (`onSend`) | Añadir cabeceras, cambiar el estado… |

```rust
app.on_request(|req: &mut Request| {
    if req.header("x-api-key") != Some("secret") {
        return Err(Error::unauthorized("falta la clave de API"));
    }
    req.set(std::time::Instant::now());
    Ok(())
});

app.pre_handler(|req: &mut Request| {
    println!("{} {}", req.method(), req.route().unwrap_or("?"));
    Ok(())
});

app.on_response(|res: &mut Response| {
    res.set_header("x-powered-by", "Vitesse");
});
```

- Un error devuelto por `on_request` o `pre_handler` se convierte en la respuesta, pasando por `app.on_error` y los hooks `on_response`.
- `on_response` ve todas las respuestas: rutas, `404`, errores (antes de que `app.on_error` las reescriba: `res.error()` sigue ahí).
- `pre_handler` se ejecuta para todas las rutas, incluidas las de los routers montados, pero no para las respuestas `404`, `405` y `OPTIONS` automáticas, que no tienen ruta.
- Los hooks de un mismo tipo se ejecutan en el orden en que se añadieron; un pánico en un hook se convierte en un `500`.
- Sin hooks, no se añade nada al recorrido de la petición. Para ejecutar código *alrededor* del handler (medir la duración, `tokio::time::timeout`…), o de forma asíncrona, usa un middleware.

### Hooks asíncronos

`app.on_request_async` y `app.pre_handler_async` son las versiones de `on_request` y `pre_handler` para los hooks que esperan algo (una caché, una base de datos, otro servicio). Reciben la petición y la devuelven, o la detienen con un error:

```rust
app.pre_handler_async(|mut req: Request| async move {
    if let Some(slug) = req.param("tenant").map(str::to_owned) {
        let tenant = load_tenant(&slug).await.ok_or_else(|| Error::not_found("inquilino desconocido"))?;
        req.set(tenant);
    }
    Ok(req)
});
```

Los hooks asíncronos se ejecutan después de los hooks simples del mismo tipo, en el orden en que se añadieron. Los routers y los grupos tienen sus propios hooks, que solo los cubren a ellos: consulta [Routers](routers.md).

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

- **Un grupo de rutas**: colócalas en un `Router` y llama a `router.middleware(...)`. Como `router.use()` en Express, cubre todo el prefijo del router, `404` incluidos (ver [Routers](routers.md)).
- **Un prefijo de ruta**: comprueba la ruta en un middleware global:

```rust
app.middleware(|req: Request, next: Next| async move {
    if req.path().starts_with("/admin") && req.header("x-admin") != Some("1") {
        return Error::forbidden("solo para administradores").into_response();
    }
    next.run(req).await
});
```

## Grupos de middlewares: `Stack` y `middleware::group`

Una `Stack` reúne varios middlewares en uno solo, para usarlos juntos en la aplicación, un router, un grupo o una sola ruta:

```rust
use std::time::Duration;
use vitesse::middleware::Stack;

let api = Stack::new()
    .with(middleware::cors())
    .with(middleware::rate_limit(100, Duration::from_secs(60)));

app.group("/api", |r| {
    r.middleware(api.clone());
    r.get("/users", list_users);
});
app.post("/webhook", webhook.with(api));
```

Como los grupos `web` y `api` de Laravel, una pila puede tener nombre con `app.middleware_group`, y usarse por su nombre con `middleware::group`, incluso en un módulo que no tiene acceso a la aplicación:

```rust
app.middleware_group("web", Stack::new().with(middleware::helmet()).with(middleware::request_id()));
app.middleware_group("api", Stack::new().with(middleware::cors()).with(middleware::bearer_auth("secret")));
app.group("/admin", admin::routes);

// src/admin.rs
pub fn routes(r: &mut Router) {
    r.middleware(middleware::group("web"));
    r.get("/dashboard", dashboard);
}
```

Los middlewares de una pila se ejecutan en orden, exactamente como si se hubieran añadido uno a uno. El nombre se busca en la primera petición y luego se recuerda; un nombre desconocido responde `500` y queda registrado.

## Middlewares incluidos

El módulo `vitesse::middleware` (accesible como `middleware::` con el preludio) trae los clásicos:

| Middleware | Equivalente en Express | Función |
|---|---|---|
| `middleware::logger()` | `morgan('dev')` | registra cada petición |
| `middleware::cors()` | `cors()` | cabeceras CORS y peticiones preliminares |
| `middleware::helmet()` | `helmet()` | cabeceras de seguridad |
| `middleware::timeout(duración)` | `connect-timeout` | `503` cuando una petición tarda demasiado |
| `middleware::body_limit(bytes)` | `express.json({ limit })` | límite de tamaño del cuerpo para una ruta o un router |
| `middleware::request_id()` | `express-request-id` | un id para cada petición, en `X-Request-Id` |
| `middleware::basic_auth(credenciales)` | `express-basic-auth` | autenticación HTTP Basic (nombre de usuario y contraseña) |
| `middleware::bearer_auth(token)` | `passport-http-bearer` | tokens `Authorization: Bearer` |
| `middleware::session(almacén)` | `express-session` | sesiones del lado del servidor, que se leen con `req.session()` (feature `session`) |
| `middleware::concurrency_limit(max)` | | limita el número de peticiones atendidas a la vez |
| `middleware::rate_limit(max, periodo)` | `express-rate-limit` | peticiones por cliente y periodo, `429` por encima |
| `middleware::etag()` | el ajuste `etag` de Express | `ETag` y `304 Not Modified` para las respuestas en memoria |
| `middleware::cache(duración)` | `apicache` | una caché de respuestas completas en el servidor, con un tiempo de vida |
| `middleware::serve_static(carpeta)` | `express.static(carpeta)` | archivos estáticos, ver [Archivos estáticos](static-files.md) |
| `middleware::compress()` | `compression()` | respuestas en Brotli o gzip (feature `compress`) |
| `middleware::decompress()` | | cuerpos de petición comprimidos (feature `compress`) |
| `middleware::layer(capa)` | | cualquier capa de tower (feature `tower`), ver [Capas de tower](#capas-de-tower) |

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

Las peticiones sin cabecera `Origin` pasan sin cambios. Las peticiones preliminares (`OPTIONS` con `Access-Control-Request-Method`) reciben directamente un `204`, sin llegar a tus rutas. Añade `cors()` a la aplicación o a un [router](routers.md), que cubre también las peticiones preliminares de sus rutas; en una sola ruta (`.with`), nunca ve la petición preliminar, que responde el `OPTIONS` automático sin cabeceras CORS. Cuando un origen no está permitido, Vitesse no añade ninguna cabecera CORS y el navegador bloquea la respuesta: CORS protege los navegadores de los usuarios, no sustituye a la autenticación.

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

### `body_limit`

Cambia el tamaño máximo del cuerpo que pueden leer los handlers, en las rutas que cubre: una ruta con `.with`, o todo un router. El resto de la aplicación mantiene `app.body_limit` (1 MiB por defecto):

```rust
app.post("/upload", upload.with(middleware::body_limit(100 * 1024 * 1024))); // 100 MiB
```

Por encima del límite, leer el cuerpo falla con `413 Payload Too Large`. Todo lo relativo a él está en [Límite de tamaño del cuerpo y 413](requests.md#límite-de-tamaño-del-cuerpo-y-413).

### `request_id`

Da a cada petición un id, para seguirla en tus registros y de un servicio al siguiente:

```rust
use vitesse::middleware::RequestId;

app.middleware(middleware::request_id());
app.get("/", |req: Request| async move {
    let id = req.get::<RequestId>().unwrap();
    println!("[{id}] página de inicio");
    "hola"
});
```

- Un `X-Request-Id` recibido con la petición (puesto por tu proxy, o por el servicio que te llama) se conserva si parece válido: de 1 a 200 caracteres ASCII visibles. Si no, Vitesse genera un UUID v4 (`9b2f3c1e-4a7d-4e1b-…`).
- El id se pone en la cabecera de la petición (reenvíalo cuando llames a otros servicios), en `req.get::<RequestId>()` (`RequestId(pub String)`, que se imprime con `{}`) y en la cabecera de la respuesta, incluidos los `404` y los errores.
- `.header("x-correlation-id")` usa otra cabecera. `.trust_incoming(false)` genera siempre un id nuevo: útil cuando los clientes llegan directamente a la aplicación y no quieres sus ids en tus registros.

Añádelo antes que los middlewares que escriben registros, para que puedan leer el id. Identifica peticiones, no es un secreto: no lo uses como token.

### `basic_auth` y `bearer_auth`

Autenticación HTTP, para un back office o una API interna. `basic_auth` pide un nombre de usuario y una contraseña:

```rust
use vitesse::middleware::BasicUser;

let password = std::env::var("ADMIN_PASSWORD").expect("ADMIN_PASSWORD no está definida");

let mut admin = Router::new();
admin.middleware(middleware::basic_auth(("admin".to_string(), password)).realm("Back office"));
admin.get("/", |req: Request| async move {
    let user = req.get::<BasicUser>().unwrap();
    format!("Hola {}", user.0)
});
app.mount("/admin", admin);
```

Sin credenciales válidas, la respuesta es un `401` con `WWW-Authenticate: Basic realm="Back office", charset="UTF-8"`, y el navegador muestra su ventana de inicio de sesión. Una vez dentro, el nombre de usuario está en `req.get::<BasicUser>()`. Las credenciales pueden ser:

| Credenciales | Ejemplo |
|---|---|
| un par | `("admin", "secret")`, con `&'static str` o `String` |
| varios pares | `[("admin", "s3cret"), ("ada", "lovelace")]`, o un `Vec` de pares |
| una closure | `\|user: &str, password: &str\| user == "ada" && check(password)` |

`bearer_auth` comprueba una cabecera `Authorization: Bearer <token>`, para las API:

```rust
let token = std::env::var("API_TOKEN").expect("API_TOKEN no está definida");

let mut api = Router::new();
api.middleware(middleware::bearer_auth(token));
api.get("/stats", |_| async { Json(json!({ "users": 42 })) });
app.mount("/api", api);
```

Acepta un token (`&'static str` o `String`), un array o un `Vec` de tokens, o una closure `|token: &str| -> bool`. Sin token, la respuesta es un `401` con `WWW-Authenticate: Bearer`; con uno incorrecto, `WWW-Authenticate: Bearer error="invalid_token"`.

- Las contraseñas y los tokens se comparan en tiempo constante, para que el tiempo de respuesta no los delate. `middleware::constant_time_eq(a, b)` hace lo mismo en tu propio código.
- Las closures son síncronas: para buscar a los usuarios en una base de datos, escribe tu propio middleware (consulta [Cortar la cadena](#cortar-la-cadena)).
- En un router, la autenticación también cubre los `404` bajo su prefijo: un visitante sin credenciales no averigua qué páginas existen.

> [!IMPORTANT]
> La autenticación Basic envía la contraseña con cada petición, simplemente codificada en base64, y cualquiera que vea un token puede reutilizarlo: úsalos solo sobre HTTPS ([HTTPS y HTTP/2](https.md), o un proxy que termine el TLS).

### `session`

Sesiones del lado del servidor, como `express-session`. Necesita la feature de Cargo `session`:

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["session"] }
```

```rust
use vitesse::session::MemoryStore;

app.middleware(middleware::session(MemoryStore::new()));
app.get("/visits", |req: Request| async move {
    let session = req.session();
    let visits = session.get::<u32>("visits").unwrap_or(0) + 1;
    session.insert("visits", visits)?;
    Ok::<_, Error>(format!("visita n.º {visits}"))
});
```

- A la entrada, carga la sesión del visitante desde el almacén, con el id de la cookie `vitesse.sid`. A la salida, guarda la sesión y envía la cookie, solo si algo cambió: los visitantes que nunca escriben en su sesión no reciben ninguna cookie.
- Los handlers, y los middlewares que se ejecutan después de él, acceden a la sesión con `req.session()`, que responde `500` donde el middleware no está instalado. Añádelo con `app.middleware`, antes de un middleware de autenticación que lea la sesión.
- La cookie es `HttpOnly`, `SameSite=Lax`, `Secure` sobre HTTPS, y firmada cuando la aplicación tiene un secreto (`app.cookie_secret(...)`). `.cookie_name(...)`, `.max_age(...)` (1 día por defecto), `.rolling(true)`, `.secure(...)`, `.same_site(...)`, `.domain(...)` y `.path(...)` la modifican.
- `MemoryStore` guarda las sesiones en la memoria del proceso; implementa el trait `SessionStore` para guardarlas en Redis o en una base de datos.

El inicio y el cierre de sesión, las opciones, los almacenes y el CSRF se explican en [Cookies firmadas y sesiones](sessions.md).

### `concurrency_limit`

Limita el número de peticiones que se atienden a la vez en lo que cubre, para proteger una ruta costosa o una base de datos frágil:

```rust
use std::time::Duration;

// Como máximo 4 informes a la vez; los demás esperan hasta 2 s y después reciben un 503.
app.get(
    "/report",
    report.with(middleware::concurrency_limit(4).max_wait(Duration::from_secs(2))),
);
```

- Como máximo hay `max` peticiones dentro a la vez; las siguientes esperan su turno.
- `.max_wait(duración)` limita esa espera: pasado ese tiempo, la respuesta es `503 Service Unavailable` con `Retry-After: 1`. Con `Duration::ZERO`, las peticiones se rechazan en cuanto se alcanza el límite, lo que mantiene el servidor ágil durante un pico.
- Cada middleware cuenta por su cuenta: con `app.middleware(...)`, el límite cubre toda la aplicación; con `.with(...)`, solo esa ruta.

Esto limita las peticiones en curso, vengan de donde vengan. `app.max_connections` (consulta [Configuración del servidor](server.md#tiempos-de-espera-y-conexiones)) limita en cambio las conexiones abiertas, incluidas las inactivas.

### `rate_limit`

Limita cada cliente a `max` peticiones por `periodo`, como `express-rate-limit`:

```rust
use std::time::Duration;

app.middleware(middleware::rate_limit(100, Duration::from_secs(60))); // 100 peticiones por minuto
app.post("/login", login.with(middleware::rate_limit(5, Duration::from_secs(60)))); // frena los intentos de adivinar contraseñas
```

- Un cliente puede enviar una ráfaga de `max` peticiones, y después una cada `periodo / max` (el algoritmo GCRA): con `rate_limit(60, Duration::from_secs(60))`, 60 de golpe, y después una por segundo.
- Por encima, la respuesta es `429 Too Many Requests` con `Retry-After` (en segundos). Todas las respuestas que pasan por el middleware llevan `X-RateLimit-Limit` y `X-RateLimit-Remaining`.
- Por defecto, un cliente es una dirección IP, `req.ip()`. Detrás de un proxy inverso, configura [`app.trust_proxy`](requests.md#detrás-de-un-proxy-trust_proxy), o todos los visitantes compartirán el contador del proxy. Las peticiones sin ninguna dirección (en un socket Unix sin `trust_proxy`) también comparten un mismo contador, en lugar de escapar del límite.
- `.key(...)` identifica a los clientes de otra forma, por clave de API o por id de usuario, por ejemplo. Cuando devuelve `None`, la petición no se limita:

```rust
let mut api = Router::new();
api.middleware(
    middleware::rate_limit(10, Duration::from_secs(1))
        .key(|req: &Request| req.header("x-api-key").map(str::to_owned)),
);
```

Los contadores viven en la memoria del proceso y se limpian automáticamente. Con varias instancias de la aplicación (varios servidores o contenedores), cada una cuenta por su cuenta.

### `etag`

Añade un `ETag` a las respuestas `200` de las peticiones `GET` y `HEAD` cuyo cuerpo está en memoria (texto, JSON, HTML…), y responde `304 Not Modified`, sin cuerpo, cuando el navegador ya tiene esa versión (`If-None-Match`). Es el ajuste `etag` de Express, aquí opcional:

```rust
app.middleware(middleware::etag());
```

El handler se ejecuta igualmente: se ahorra ancho de banda, no trabajo. Los cuerpos en flujo se dejan tal cual, y los archivos servidos con `res::file` o `ServeDir` ya tienen su propio `ETag`.

### `cache`

Guarda respuestas completas en memoria durante un tiempo de vida, como `apicache` o el `responsecache` de Laravel: el primer `GET` de una URL ejecuta el handler, los siguientes, hasta que la entrada caduca, se responden desde la memoria sin ejecutarlo.

```rust
use std::time::Duration;

let cache = middleware::cache(Duration::from_secs(60)).max_entries(1_000);
app.get("/posts", list_posts.with(cache.clone()));
app.post("/posts", move |req: Request| {
    let cache = cache.clone();
    async move {
        let post = create_post(req).await?;
        cache.invalidate("/posts"); // la lista ha cambiado
        Ok::<_, Error>((201, Json(post)))
    }
});
```

- Las respuestas en caché llevan un `ETag` (y reciben `304` cuando el navegador tiene la misma versión), `Age`, `X-Cache: HIT` o `MISS`, y `Cache-Control: public, max-age=…` salvo que el handler haya puesto uno (`.cache_control(false)` para no decir nada a los navegadores).
- Solo se guardan las respuestas `200` con el cuerpo en memoria, de 1 MiB como máximo por defecto (`.max_body(bytes)`), y nunca las que ponen una cookie o dicen `Cache-Control: no-store`, `private` o `no-cache`. Las peticiones con una cabecera `Authorization` no se guardan.
- La clave es la ruta y la query string. `.key(|req| …)` añade aquello de lo que depende la respuesta (el idioma, el usuario), o devuelve `None` para no guardar una petición.
- Los clones comparten sus entradas: `cache.invalidate(clave)`, `cache.invalidate_prefix("/posts")` y `cache.clear()` las eliminan. Por encima de `.max_entries(n)` (10 000 por defecto), primero se van las entradas caducadas y luego las más antiguas.

Las entradas viven en la memoria del proceso: con varias instancias de la aplicación, cada una tiene su propia caché.

### `compress`

Comprime las respuestas con Brotli, o con gzip cuando el navegador no acepta Brotli, como `compression()` de Express. Necesita la feature de Cargo `compress`:

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["compress"] }
```

```rust
app.middleware(middleware::compress());
```

Solo se comprimen los cuerpos que merecen la pena: HTML, CSS, JavaScript, JSON, XML, SVG, WebAssembly y los demás tipos de texto, de 1 KiB o más, de tamaño conocido (no en streaming, así que los [Server-Sent Events](responses.md#server-sent-events) también pasan tal cual) y no codificados ya. Las imágenes, los vídeos y los archivos comprimidos pasan tal cual. La respuesta recibe `Content-Encoding` y `Vary: Accept-Encoding`, y la compresión corre en un hilo bloqueante, así que los workers siguen atendiendo otras peticiones. Añádelo antes que los demás middlewares, para que todas las respuestas pasen por él.

### `decompress`

Al revés: algunos clientes (apps móviles, otros servicios, agentes que envían registros) comprimen los cuerpos que envían. `decompress()` los decodifica antes de que tus handlers los lean con `req.json()`, `req.text()` y los demás. Viene con la misma feature `compress`:

```rust
app.middleware(middleware::decompress());
```

- Los cuerpos enviados con `Content-Encoding: gzip` (o `x-gzip`), `br` o `deflate` se descomprimen, y después se quita `Content-Encoding`: el handler ve el cuerpo original. Las peticiones sin `Content-Encoding` pasan sin cambios.
- Otra codificación recibe un `415 Unsupported Media Type`, y unos datos corruptos, un `400`.
- Tanto el cuerpo comprimido como el descomprimido están sujetos al [límite del cuerpo](requests.md#límite-de-tamaño-del-cuerpo-y-413) (`413` por encima), así que una pequeña «bomba zip» no puede llenar la memoria.
- El cuerpo se lee entero y después se descomprime en un hilo bloqueante, así que los workers siguen atendiendo otras peticiones.

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

> [!NOTE]
> Un pánico en un middleware escrito como closure o como `async fn` se convierte en un `500` que vuelve a pasar por los middlewares exteriores y por `app.on_error`, igual que un pánico en un handler. Un `impl Middleware` escrito a mano no recibe este trato: sus pánicos se capturan más arriba en la cadena (consulta [Pánicos](errors.md#pánicos)).

## Capas de tower

axum, hyper y tonic comparten un ecosistema de middlewares: [tower](https://docs.rs/tower) y [tower-http](https://docs.rs/tower-http). Con la feature `tower`, `middleware::layer(...)` ejecuta cualquier `Layer` de tower como un middleware de Vitesse:

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tower"] }
tower-http = { version = "0.6", features = ["cors", "set-header", "trace"] }
```

```rust
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use vitesse::http::{HeaderName, HeaderValue};
use vitesse::prelude::*;

let mut app = App::new();
app.middleware(middleware::layer(TraceLayer::new_for_http()));
app.middleware(middleware::layer(SetResponseHeaderLayer::overriding(
    HeaderName::from_static("x-powered-by"),
    HeaderValue::from_static("Vitesse"),
)));

let mut api = Router::new();
api.middleware(middleware::layer(CorsLayer::permissive()));
api.get("/users", |_| async { Json(vec!["ada", "grace"]) });
app.mount("/api", api);
```

Una capa va donde va un middleware: `app.middleware(...)`, `router.middleware(...)` y `handler.with(...)`, en el mismo [orden](#orden-de-ejecución) que los demás. `TraceLayer` informa a través del crate [tracing](https://docs.rs/tracing): sus líneas solo aparecen una vez instalado un subscriber, `tracing_subscriber::fmt::init()` por ejemplo.

Funciona cualquier capa cuyo servicio acepte un `http::Request<vitesse::Body>` y devuelva un `http::Response`: la mayor parte de tower-http (`CorsLayer`, `SetResponseHeaderLayer`, `ValidateRequestHeaderLayer`, `RequestBodyLimitLayer`, `TraceLayer`…), las propias capas de tower (`ConcurrencyLimitLayer`, `MapRequestLayer`…) y crates como tower-governor.

Cómo se encuentran los dos mundos:

- **La petición.** La capa ve un `http::Request` construido a partir de la petición de Vitesse. Todo lo que cambie (método, URI, cabeceras, extensiones, cuerpo) se traslada a la petición que recibe el resto de la cadena, enrutamiento incluido si la capa es global.
- **La respuesta.** Vuelve como una `Response` de Vitesse, que los middlewares que rodean la capa todavía pueden cambiar.
- **Los errores.** Un error devuelto por el servicio de tower (el `Timeout` de tower devuelve uno, por ejemplo) se convierte en un `500`, que se registra sin mostrarse al cliente. Los errores que produce Vitesse (un `404`, el `Err` de un handler) atraviesan la capa y siguen llegando a [`app.on_error`](errors.md).
- **Un servicio para todas las peticiones.** El servicio se construye una vez, al arrancar, y después se clona para cada petición, como en axum. El estado que comparten sus clones lo comparten todas las peticiones: un `ConcurrencyLimitLayer` limita de verdad toda la ruta.

> [!TIP]
> Cada capa convierte la petición en `http::Request` y luego al revés, lo que cuesta un poco. Para lo que Vitesse ya incluye (CORS, tiempos de espera, compresión, límites, autenticación), los middlewares incluidos son la opción más rápida; las capas están para todo lo demás.

En sentido inverso, `app.into_service()` convierte toda una aplicación Vitesse en un `Service` de tower: sírvela con hyper, anídala en un router de axum (`Router::nest_service`) o pruébala con `tower::ServiceExt` (consulta [Probar como servicio de tower](testing.md#probar-como-servicio-de-tower)).
