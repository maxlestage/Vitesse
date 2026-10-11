# Preguntas frecuentes y límites

Las respuestas a las preguntas más habituales sobre Vitesse, seguidas de una lista honesta de lo que (todavía) no hace. Si tu pregunta no está aquí, abre una issue en GitHub.

## General

### ¿Está Vitesse listo para producción?

Vitesse es joven (versión 0.1), pero su motor se pensó para producción desde el principio: la batería de pruebas cubre a fondo el motor HTTP (pipelining, cuerpos `chunked`, `Expect: 100-continue`, límites de tamaño, apagado ordenado) y se ejecuta en Linux, macOS y Windows con cada cambio. Los pánicos se convierten en respuestas `500` en lugar de tumbar el servidor, las peticiones mal formadas se rechazan y `SIGTERM` deja terminar las peticiones en curso.

Hasta la versión 1.0, la API todavía puede cambiar entre versiones menores. Depende de `vitesse = { git = "https://github.com/maxlestage/Vitesse" }` (así Cargo solo instala actualizaciones compatibles `0.1.x`), lee el registro de cambios antes de actualizar, sirve HTTPS ([directamente](https.md) o a través de un proxy inverso, consulta [Producción](production.md)) y haz pruebas de carga con tu propio tráfico.

### ¿El proyecto es realmente 100 % Rust?

Sí: el framework, los ejemplos, la herramienta de benchmark (`bench/runner`) y el sitio web (servido por el propio Vitesse, con páginas e islas WebAssembly escritas con [Active](https://maxlestage.github.io/Active/)) están en Rust, y el repositorio no contiene JavaScript ni TypeScript. Dos matices, para ser precisos:

- los navegadores no pueden arrancar WebAssembly sin unas pocas líneas de JavaScript, y solo ejecutan un service worker (lo que hace que el sitio funcione sin conexión) en forma de script: la compilación del sitio (wasm-bindgen) genera un pequeño archivo de carga, y el servidor del sitio genera su service worker; ninguno de los dos se escribe a mano ni se guarda en el repositorio;
- la documentación muestra fragmentos de Express (JavaScript), pero solo como comparaciones de antes y después para quienes vienen de Express.

El benchmark también mide servidores escritos en otros lenguajes, porque compararlos es justamente el objetivo: el servidor de Drogon está en C++ (`bench/drogon`), y wrk, que genera la carga, ejecuta scripts de escenario escritos en Lua (`bench/lua`).

### ¿Por qué no actix-web o axum?

Los dos son frameworks excelentes y maduros, y Vitesse cubre ya la mayor parte de lo que ofrecen: HTTPS y HTTP/2 (además de HTTP/3), WebSocket, Server-Sent Events, subida de archivos multipart, límites de cuerpo por ruta, autenticación, límites de peticiones por cliente y de concurrencia, ids de petición, compresión, proxies de confianza, tiempos de espera configurables, sockets Unix y el ecosistema de middlewares de [tower](https://crates.io/crates/tower) a través de la feature `tower`. [Viniendo de axum o actix-web](from-axum-actix.md) da las equivalencias, funcionalidad por funcionalidad.

Vitesse encaja bien si:

- vienes de Express y quieres mantener la misma forma de pensar: una sola `Request`, middlewares con `next`, routers y handlers que simplemente devuelven su respuesta;
- buscas el máximo rendimiento: en el [benchmark](performance.md), Vitesse atiende más peticiones por segundo que ambos, con menos CPU por petición.

Lo que sigue siendo distinto:

- **sin extractores, por diseño**: un handler le pide a su `Request` lo que necesita (`req.param_as`, `req.json`…) en lugar de declarar argumentos tipados;
- **todavía sin API 1.x estable**: Vitesse está en la versión 0.1 y aún puede cambiar antes de la 1.0, mientras que axum y actix-web llevan años en producción;
- **un ecosistema más pequeño**: hay menos crates, ejemplos y respuestas en internet escritos para Vitesse, aunque las capas y los servicios de tower salvan buena parte de la distancia.

### ¿En qué se diferencia de Express?

La API es parecida a propósito, pero los handlers devuelven su respuesta en lugar de modificar `res`, los datos tienen tipos, los errores se propagan con `?` y el programa se compila, lo que lo hace unas 50 veces más rápido en el benchmark. La guía [Viniendo de Express](from-express.md) lo explica todo.

### ¿Qué runtime asíncrono usa?

[tokio](https://tokio.rs), y solo tokio. `app.run` crea su propio runtime, mientras que `app.listen` y `app.bind` se ejecutan dentro del tuyo. Cualquier crate basado en tokio funciona en tus handlers: drivers de bases de datos, clientes HTTP, Redis… Vitesse reexporta el crate como `vitesse::tokio` (con las features que usa él mismo); añade tokio a tu `Cargo.toml` para usar `#[tokio::main]` o `#[tokio::test]`. Otros runtimes (async-std, smol) no están soportados.

### ¿Qué versión de Rust necesito?

Rust 1.85 o posterior (la edición 2024), como indica `rust-version` en el `Cargo.toml` de Vitesse. Actualiza con `rustup update`.

### ¿Funciona en Windows y macOS?

Sí: la batería de pruebas se ejecuta en Linux, macOS y Windows. El modo de un hilo por núcleo (un bucle de eventos y un socket `SO_REUSEPORT` por núcleo) es una optimización exclusiva de Linux; en otros sistemas, `app.run` usa el runtime multihilo de tokio, algo más lento pero con exactamente la misma API. `Ctrl+C` detiene el servidor limpiamente en todas partes, y `SIGTERM` en Unix. Los sockets de dominio Unix (`app.run_unix`) no están disponibles en Windows. La herramienta de benchmark (`bench/runner`) solo funciona en Linux.

## Funcionalidades

### ¿Soporta Vitesse HTTPS y HTTP/2?

Sí, con dos features de Cargo opcionales. `tls` sirve HTTPS (TLS 1.2 y 1.3, con rustls: sin OpenSSL que instalar): `app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?)`. `http2` añade HTTP/2, que los navegadores negocian sobre TLS, y que también reciben sin TLS los clientes que empiezan en HTTP/2 (h2c). Las mismas rutas y los mismos middlewares responden en los dos protocolos. Consulta [HTTPS y HTTP/2](https.md).

Un proxy inverso (Nginx, Caddy) delante sigue siendo una buena opción, para tener certificados automáticos o varias aplicaciones en una misma máquina, y plataformas como Heroku gestionan HTTPS por ti: consulta [Producción](production.md).

### ¿Soporta Vitesse WebSocket y HTTP/3?

Sí, los dos:

- **WebSocket** viene integrado (la feature `ws`, activada por defecto): `app.ws("/chat", |req, socket| async move { … })`, al estilo de `express-ws`. Consulta [WebSocket](websocket.md).
- **HTTP/3** sobre QUIC está disponible con la feature opcional `http3`: `app.http3(Http3::from_pem_files(...)?)` sirve la misma aplicación por UDP, junto a HTTP/1.1. Consulta [HTTP/3 y QUIC](http3.md).

WebSocket funciona sobre HTTP/1.1 (`ws://`, o `wss://` con la feature `tls`), no sobre HTTP/2 ni HTTP/3. Para enviar datos del servidor al cliente en un solo sentido, los Server-Sent Events suelen ser más sencillos: consulta la pregunta siguiente.

### ¿Soporta Vitesse Server-Sent Events?

Sí, con `vitesse::sse` (siempre disponible, y en el preludio como `sse`): devuelve una respuesta `Sse`, y el navegador la lee con `EventSource`. Lo más sencillo es un canal, alimentado por una tarea:

```rust
use std::time::Duration;

use vitesse::prelude::*;
use vitesse::sse::Event;

let mut app = App::new();
app.get("/clock", |_| async {
    let (tx, stream) = sse::channel(16);
    tokio::spawn(async move {
        for i in 0.. {
            // `send` falla en cuanto el navegador se ha ido.
            if tx.send(Event::new().event("tick").data(i.to_string())).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    stream
});
```

Vitesse envía un comentario de keep-alive cada 15 segundos, para que los proxies no cierren una conexión silenciosa. Funciona sobre HTTP/1.1, HTTP/2 y HTTP/3. Consulta [Server-Sent Events](responses.md#server-sent-events).

### ¿Compresión?

Sí, con la feature `compress`. `middleware::compress()` comprime las respuestas en Brotli, o en gzip para los navegadores que no aceptan Brotli, cuando merece la pena (cuerpos de tipo texto de 1 KiB o más). `middleware::decompress()` hace lo contrario con los cuerpos de petición enviados con `Content-Encoding: gzip`, `br` o `deflate`, dentro del límite del cuerpo (que también frena las «bombas zip»):

```rust
app.middleware(middleware::compress());
app.middleware(middleware::decompress());
```

Detrás de un proxy inverso, también puedes dejar que sea el proxy quien comprima las respuestas. Consulta [Middlewares](middleware.md#compress).

### ¿Motores de plantillas?

No vienen incluidos, igual que Express sin motor de vistas. Usa el crate de plantillas que prefieras ([askama](https://crates.io/crates/askama), [minijinja](https://crates.io/crates/minijinja), [tera](https://crates.io/crates/tera)…) y devuelve el resultado con `Html(renderizado)`. Carga o compila tus plantillas una sola vez al arrancar y compártelas mediante el [estado](state.md).

### ¿Cómo uso una base de datos?

Con cualquier driver asíncrono (por ejemplo [sqlx](https://crates.io/crates/sqlx)). Crea el pool de conexiones una sola vez al arrancar, regístralo con `app.state(pool)` y léelo en los handlers con `req.state::<Pool>()`. Como crear un pool suele ser asíncrono, arranca el servidor con `#[tokio::main]` y `app.listen` (que se apaga de forma ordenada, igual que `app.run`), como se muestra en [Configuración del servidor](server.md).

### ¿Subida de archivos (multipart)?

Sí, con la feature `multipart`. `req.multipart()?` lee un cuerpo `multipart/form-data` campo a campo, a medida que llega, y `field.save(path)` escribe un archivo en disco como flujo, sin guardarlo en memoria. Sube el límite del cuerpo solo en la ruta de subida, con `middleware::body_limit`:

```rust
async fn upload(req: Request) -> vitesse::Result<String> {
    let mut form = req.multipart()?; // 415 si el cuerpo no es multipart
    let (mut files, mut bytes) = (0, 0);
    while let Some(field) = form.next_field().await? {
        if field.file_name().is_some() {
            files += 1;
            // Nunca uses tal cual el nombre de archivo del cliente como ruta.
            bytes += field.save(format!("uploads/{files}.bin")).await?;
        }
    }
    Ok(format!("{files} archivos, {bytes} bytes"))
}

app.post("/upload", upload.with(middleware::body_limit(100 * 1024 * 1024)));
```

Consulta [Subida de archivos](requests.md#subida-de-archivos-multipart).

### ¿Sesiones y autenticación?

Para la autenticación hay dos middlewares incluidos: `middleware::basic_auth(("admin", "secret"))` (el navegador pide un nombre de usuario y una contraseña) y `middleware::bearer_auth(token)` (`Authorization: Bearer …`, para las API). Los dos aceptan también una closure, para comprobar las credenciales con tus propios usuarios, y las comparan en tiempo constante. Envían las credenciales con cada petición: úsalos solo sobre HTTPS. Consulta [`basic_auth` y `bearer_auth`](middleware.md#basic_auth-y-bearer_auth).

Para las sesiones, Vitesse no tiene almacén propio: usa [tower-sessions](https://crates.io/crates/tower-sessions) a través de la feature `tower`, como con axum:

```rust
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};

app.middleware(middleware::layer(SessionManagerLayer::new(MemoryStore::default())));
app.get("/visits", |req: Request| async move {
    let session = req.get::<Session>().unwrap().clone();
    let visits: u32 = session.get("visits").await?.unwrap_or(0) + 1;
    session.insert("visits", visits).await?;
    Ok::<_, Error>(format!("visita n.º {visits}"))
});
```

Su cookie es `Secure` por defecto, así que necesita HTTPS (o `.with_secure(false)` durante el desarrollo), y `MemoryStore` lo olvida todo al reiniciar: usa un almacén persistente en producción. Consulta [Sesiones con tower-sessions](from-axum-actix.md#sesiones-con-tower-sessions).

También puedes construirlas tú mismo: lee cookies con `req.cookie(nombre)`, créalas con `Cookie` (`http_only`, `secure`, `same_site`…), comprueba un token en un middleware y adjunta el usuario a la petición con `req.set(user)`. Consulta [Middlewares](middleware.md).

### ¿Por qué CORS rechaza mis cookies?

`middleware::cors()` permite todos los orígenes con `Access-Control-Allow-Origin: *`. En una petición con credenciales (cookies, `Authorization`), los navegadores rechazan una respuesta que dice `*`, y eso es lo que sigues obteniendo con `allow_credentials(true)` si no indicas ningún origen. Es intencionado, como en Express: devolver cualquier origen permitiría a cualquier web leer las respuestas de un usuario con la sesión iniciada. Indica explícitamente tus orígenes de confianza:

```rust
app.middleware(
    middleware::cors()
        .allow_origin("https://app.example.com")
        .allow_credentials(true),
);
```

### ¿Cómo registro las peticiones y los errores?

`middleware::logger()` muestra una línea por petición, como `morgan('dev')`: `GET /users/42 200 0.084 ms`. La causa de los errores del servidor (un error `5xx` con una fuente, por ejemplo un error de base de datos convertido con `?`) se muestra en la salida de errores, con el prefijo `[vitesse]`.

Para logs estructurados, escribe tu propio middleware con el crate de logging que prefieras. Dos utilidades hacen los registros más fáciles de aprovechar: `middleware::request_id()` da a cada petición un id (`req.get::<middleware::RequestId>()`), que se devuelve en la cabecera de respuesta `X-Request-Id`, y `req.route()` da el patrón de la ruta elegida (`/users/:id`), para agrupar los registros por patrón de ruta y no por ruta concreta. Con la feature `tower`, el `TraceLayer` de tower-http también funciona, a través de `middleware::layer`. Consulta [Registros](production.md#registros) para un ejemplo completo.

### ¿Por qué `req.ip()` devuelve la dirección del proxy?

Porque es el proxy quien está conectado a Vitesse. La dirección del cliente está en la cabecera `X-Forwarded-For`, que un cliente también podría enviar: indica a Vitesse en qué proxies confiar, como el ajuste `trust proxy` de Express, y `req.ip()` la lee por ti:

```rust
app.trust_proxy("loopback"); // un proxy en la misma máquina; o "10.0.0.0/8", un número de saltos, true
```

`req.protocol()` y `req.hostname()` siguen entonces también a `X-Forwarded-Proto` y `X-Forwarded-Host`. Consulta [Obtener la IP del cliente](production.md#obtener-la-ip-del-cliente).

### ¿Puedo usar middlewares de tower o tower-http?

Sí, con la feature `tower`. `middleware::layer(...)` ejecuta cualquier `Layer` de tower como un middleware de Vitesse, de forma global, en un router o en una sola ruta: `CorsLayer`, `TraceLayer` o `SetResponseHeaderLayer` de tower-http, `ConcurrencyLimitLayer` de tower, tower-governor, tower-sessions…

```rust
use tower_http::cors::CorsLayer;

app.middleware(middleware::layer(CorsLayer::permissive()));
```

En sentido inverso, `app.into_service()` convierte tu aplicación en un `Service` de tower: sírvela con hyper, anídala en un router de axum (`Router::nest_service`) o pruébala con `tower::ServiceExt::oneshot`. Consulta [Capas de tower](middleware.md#capas-de-tower).

### ¿Puedo añadir rutas con el servidor en marcha?

No. La aplicación se congela cuando arranca el servidor (es parte de lo que la hace rápida: sin bloqueos ni contadores de referencias). Define tus rutas al arrancar y usa el [estado](state.md) para los datos que cambian.

## Despliegue y comunidad

### ¿Cómo despliego una aplicación Vitesse?

Compila un binario en modo release (`cargo build --release`), escucha en el puerto que indique la variable de entorno `PORT` y ejecútalo. Guías paso a paso: [Heroku desde el móvil](heroku-mobile.md), [Docker](docker.md) y [Producción](production.md) para proxies inversos y HTTPS.

### ¿Cómo informo de un error o contribuyo?

Abre una issue en GitHub con tu versión de Vitesse, tu sistema y, a ser posible, un ejemplo mínimo que reproduzca el problema. Las pull requests también son bienvenidas; para un cambio grande, abre antes una issue para comentarlo. Antes de enviar tus cambios, ejecuta las mismas comprobaciones que la CI:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

La documentación está en `docs/`, en inglés, francés y español: cuando cambies una página, actualiza también los otros idiomas o menciónalo en tu pull request.

### ¿Cuál es la licencia?

Vitesse es software propietario: © Maxime Nathan Lestage, todos los derechos reservados. Su código fuente se publica en crates.io y GitHub para que puedas leerlo, auditarlo y compilarlo, y puedes usarlo libremente como dependencia de tus propios programas, también comerciales. Modificarlo o redistribuir su código fuente requiere el permiso escrito del autor. Las condiciones completas están en el archivo [LICENSE](https://maxlestage.github.io/Vitesse/LICENSE.txt) del repositorio.

## Límites actuales

Como Express, Vitesse hace pocas cosas a propósito. No incluye (todavía):

- **Extractores**, por diseño: un handler recibe una sola `Request` y le pide lo que necesita;
- **WebSocket sobre HTTP/2 y HTTP/3**: [WebSocket](websocket.md) funciona sobre HTTP/1.1, con o sin TLS;
- **El server push de HTTP/2** y el mecanismo **`Upgrade: h2c`**: HTTP/2 funciona sobre TLS y sin cifrar con *prior knowledge* (consulta [HTTPS y HTTP/2](https.md));
- **0-RTT y lectura por partes de los cuerpos de las peticiones en HTTP/3**: en HTTP/3, los cuerpos de las peticiones se leen enteros antes de ejecutar el handler;
- **Motores de plantillas**: usa un crate de plantillas y devuelve `Html(...)`;
- **Un almacén de sesiones integrado y cookies firmadas**: usa [tower-sessions](from-axum-actix.md#sesiones-con-tower-sessions) a través de la feature `tower`;
- **Parámetros parciales dentro de un segmento** (`/vuelos/:desde-:hasta`), ni parámetros opcionales (`/:id?`) ni expresiones regulares en las rutas;
- **Generación de URL a partir de rutas con nombre** (el `url_for` de actix-web);
- **Runtimes distintos de tokio**.

Otras cosas que conviene saber:

- los límites de tamaño del motor son fijos (60 KiB para la cabecera de la petición, 64 cabeceras), pero sus plazos son ajustes: `keep_alive` (60 s), `header_timeout` (30 s) y `shutdown_timeout` (10 s), además de `max_connections` (consulta [Configuración del servidor](server.md#tiempos-de-espera-y-conexiones));
- `body_limit` se aplica a toda la aplicación, y `middleware::body_limit` lo cambia por ruta o por router;
- las rutas distinguen mayúsculas y minúsculas (`/Users` ≠ `/users`);
- no se pueden añadir rutas una vez arrancado el servidor;
- `X-Forwarded-For`, `X-Forwarded-Proto` y `X-Forwarded-Host` solo se leen una vez que declaras tus proxies con `app.trust_proxy(...)`;
- `middleware::rate_limit` cuenta en la memoria de cada proceso: con varias instancias, cada una cuenta por su cuenta.
