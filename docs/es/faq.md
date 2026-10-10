# Preguntas frecuentes y límites

Las respuestas a las preguntas más habituales sobre Vitesse, seguidas de una lista honesta de lo que (todavía) no hace. Si tu pregunta no está aquí, abre una issue en [GitHub](https://github.com/maxlestage/Vitesse/issues).

## General

### ¿Está Vitesse listo para producción?

Vitesse es joven (versión 0.1), pero su motor se pensó para producción desde el principio: la batería de pruebas cubre a fondo el motor HTTP (pipelining, cuerpos `chunked`, `Expect: 100-continue`, límites de tamaño, apagado ordenado) y se ejecuta en Linux, macOS y Windows con cada cambio. Los pánicos se convierten en respuestas `500` en lugar de tumbar el servidor, las peticiones mal formadas se rechazan y `SIGTERM` deja terminar las peticiones en curso.

Hasta la versión 1.0, la API todavía puede cambiar entre versiones menores. Depende de `vitesse = "0.1"` (así Cargo solo instala actualizaciones compatibles `0.1.x`), lee el [registro de cambios](https://github.com/maxlestage/Vitesse/blob/master/CHANGELOG.md) antes de actualizar, pon un proxy inverso delante para HTTPS (consulta [Producción](production.md)) y haz pruebas de carga con tu propio tráfico.

### ¿El proyecto es realmente 100 % Rust?

Sí: el framework, los ejemplos, la herramienta de benchmark (`bench/runner`) y el sitio web (escrito con [Yew](https://yew.rs) y compilado a WebAssembly) están en Rust, y el repositorio no contiene JavaScript ni TypeScript. Dos matices, para ser precisos:

- los navegadores no pueden arrancar WebAssembly sin unas pocas líneas de JavaScript, así que la compilación del sitio (Trunk y wasm-bindgen) genera automáticamente un pequeño archivo de carga, que ni se escribe a mano ni se guarda en el repositorio;
- la documentación muestra fragmentos de Express (JavaScript), pero solo como comparaciones de antes y después para quienes vienen de Express.

El benchmark también mide servidores escritos en otros lenguajes, porque compararlos es justamente el objetivo: el servidor de Drogon está en C++ (`bench/drogon`), y wrk, que genera la carga, ejecuta scripts de escenario escritos en Lua (`bench/lua`).

### ¿Por qué no actix-web o axum?

Los dos son frameworks excelentes y maduros. Vitesse encaja bien si:

- vienes de Express y quieres mantener la misma forma de pensar: una sola `Request`, middlewares con `next`, routers y handlers que simplemente devuelven su respuesta, sin extractores ni capas de servicios que aprender;
- buscas el máximo rendimiento: en el [benchmark](performance.md), Vitesse atiende más peticiones por segundo que ambos, con menos CPU por petición.

Elige actix-web o axum si necesitas lo que Vitesse (todavía) no hace: HTTP/2 o TLS sobre TCP integrados, el ecosistema de middlewares de [tower](https://github.com/tower-rs/tower) o una API 1.x estable.

### ¿En qué se diferencia de Express?

La API es parecida a propósito, pero los handlers devuelven su respuesta en lugar de modificar `res`, los datos tienen tipos, los errores se propagan con `?` y el programa se compila, lo que lo hace unas 50 veces más rápido en el benchmark. La guía [Viniendo de Express](from-express.md) lo explica todo.

### ¿Qué runtime asíncrono usa?

[tokio](https://tokio.rs), y solo tokio. `app.run` crea su propio runtime, mientras que `app.listen` y `app.bind` se ejecutan dentro del tuyo. Cualquier crate basado en tokio funciona en tus handlers: drivers de bases de datos, clientes HTTP, Redis… Vitesse reexporta el crate como `vitesse::tokio` (con las features que usa él mismo); añade tokio a tu `Cargo.toml` para usar `#[tokio::main]` o `#[tokio::test]`. Otros runtimes (async-std, smol) no están soportados.

### ¿Qué versión de Rust necesito?

Rust 1.85 o posterior (la edición 2024), como indica `rust-version` en el `Cargo.toml` de Vitesse. Actualiza con `rustup update`.

### ¿Funciona en Windows y macOS?

Sí: la batería de pruebas se ejecuta en Linux, macOS y Windows. El modo de un hilo por núcleo (un bucle de eventos y un socket `SO_REUSEPORT` por núcleo) es una optimización exclusiva de Linux; en otros sistemas, `app.run` usa el runtime multihilo de tokio, algo más lento pero con exactamente la misma API. `Ctrl+C` detiene el servidor limpiamente en todas partes, y `SIGTERM` en Unix. La herramienta de benchmark (`bench/runner`) solo funciona en Linux.

## Funcionalidades

### ¿Soporta Vitesse HTTPS y HTTP/2?

No sobre TCP: Vitesse habla HTTP/1.1 sobre TCP sin cifrar. Ponlo detrás de un proxy inverso (Nginx, Caddy o el balanceador de carga de tu plataforma) que gestione TLS y HTTP/2 y reenvíe las peticiones en HTTP/1.1, como es habitual con Express. Plataformas como Heroku ya lo hacen por ti. Consulta [Producción](production.md). La excepción es HTTP/3, cuyo TLS está integrado en QUIC: consulta la pregunta siguiente.

### ¿Soporta Vitesse WebSocket y HTTP/3?

Sí, los dos:

- **WebSocket** viene integrado (la feature `ws`, activada por defecto): `app.ws("/chat", |req, socket| async move { … })`, al estilo de `express-ws`. Consulta [WebSocket](websocket.md).
- **HTTP/3** sobre QUIC está disponible con la feature opcional `http3`: `app.http3(Http3::from_pem_files(...)?)` sirve la misma aplicación por UDP, junto a HTTP/1.1. Consulta [HTTP/3 y QUIC](http3.md).

WebSocket solo funciona sobre HTTP/1.1, no sobre HTTP/3. Para enviar datos del servidor al cliente en un solo sentido, una respuesta en flujo también es una opción: `Body::from_stream` envía cada fragmento en cuanto se produce, que es todo lo que necesitas para [Server-Sent Events](https://developer.mozilla.org/es/docs/Web/API/Server-sent_events) con el tipo de contenido `text/event-stream` (consulta [Respuestas](responses.md)).

### ¿Compresión?

No viene incluida. Deja que el proxy inverso comprima las respuestas (gzip, brotli), que suele ser además la opción más eficiente.

### ¿Motores de plantillas?

No vienen incluidos, igual que Express sin motor de vistas. Usa el crate de plantillas que prefieras ([askama](https://crates.io/crates/askama), [minijinja](https://crates.io/crates/minijinja), [tera](https://crates.io/crates/tera)…) y devuelve el resultado con `Html(renderizado)`. Carga o compila tus plantillas una sola vez al arrancar y compártelas mediante el [estado](state.md).

### ¿Cómo uso una base de datos?

Con cualquier driver asíncrono (por ejemplo [sqlx](https://crates.io/crates/sqlx)). Crea el pool de conexiones una sola vez al arrancar, regístralo con `app.state(pool)` y léelo en los handlers con `req.state::<Pool>()`. Como crear un pool suele ser asíncrono, arranca el servidor con `#[tokio::main]` y `app.listen` (que se apaga de forma ordenada, igual que `app.run`), como se muestra en [Configuración del servidor](server.md).

### ¿Subida de archivos (multipart)?

No hay un analizador de `multipart/form-data` integrado. Lee el cuerpo en bruto con `req.bytes()` (dentro del `body_limit`, que tendrás que subir para archivos grandes) o como flujo con `req.take_body()`, y analízalo con un crate específico como [multer](https://crates.io/crates/multer).

### ¿Sesiones y autenticación?

No hay sesiones integradas, pero tienes todas las piezas: lee cookies con `req.cookie(nombre)`, créalas con `Cookie` (`http_only`, `secure`, `same_site`…), comprueba un token en un middleware y adjunta el usuario a la petición con `req.set(user)`. Consulta [Middlewares](middleware.md).

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

`middleware::logger()` muestra una línea por petición, como `morgan('dev')`: `GET /users/42 200 0.084 ms`. La causa de los errores del servidor (un error `5xx` con una fuente, por ejemplo un error de base de datos convertido con `?`) se muestra en la salida de errores, con el prefijo `[vitesse]`. Para logs estructurados, escribe tu propio middleware con el crate de logging que prefieras.

### ¿Por qué `req.ip()` devuelve la dirección del proxy?

Porque es el proxy quien está conectado a Vitesse. La dirección del cliente está en la cabecera `X-Forwarded-For` (`req.header("x-forwarded-for")`), en la que solo debes confiar si la pone tu propio proxy. Consulta [Producción](production.md).

### ¿Puedo añadir rutas con el servidor en marcha?

No. La aplicación se congela cuando arranca el servidor (es parte de lo que la hace rápida: sin bloqueos ni contadores de referencias). Define tus rutas al arrancar y usa el [estado](state.md) para los datos que cambian.

## Despliegue y comunidad

### ¿Cómo despliego una aplicación Vitesse?

Compila un binario en modo release (`cargo build --release`), escucha en el puerto que indique la variable de entorno `PORT` y ejecútalo. Guías paso a paso: [Heroku desde el móvil](heroku-mobile.md), [Docker](docker.md) y [Producción](production.md) para proxies inversos y HTTPS.

### ¿Cómo informo de un error o contribuyo?

Abre una issue en [GitHub](https://github.com/maxlestage/Vitesse/issues) con tu versión de Vitesse, tu sistema y, a ser posible, un ejemplo mínimo que reproduzca el problema. Las pull requests también son bienvenidas; para un cambio grande, abre antes una issue para comentarlo. Antes de enviar tus cambios, ejecuta las mismas comprobaciones que la CI:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

La documentación está en [`docs/`](https://github.com/maxlestage/Vitesse/tree/master/docs), en inglés, francés y español: cuando cambies una página, actualiza también los otros idiomas o menciónalo en tu pull request.

### ¿Cuál es la licencia?

Vitesse tiene doble licencia, [MIT](https://github.com/maxlestage/Vitesse/blob/master/LICENSE-MIT) o [Apache 2.0](https://github.com/maxlestage/Vitesse/blob/master/LICENSE-APACHE), a tu elección (`MIT OR Apache-2.0`), como la mayor parte del ecosistema Rust.

## Límites actuales

Como Express, Vitesse hace pocas cosas a propósito. No incluye (todavía):

- **HTTP/2 y TLS sobre TCP** (HTTPS): pon Vitesse detrás de un proxy inverso como Nginx o Caddy (consulta [Producción](production.md)). [HTTP/3](http3.md), cuyo TLS va integrado, sí está soportado;
- **WebSocket sobre HTTP/3**: [WebSocket](websocket.md) funciona sobre HTTP/1.1;
- **0-RTT y lectura por partes de los cuerpos de las peticiones en HTTP/3**: en HTTP/3, los cuerpos de las peticiones se leen enteros antes de ejecutar el handler;
- **Compresión**: déjasela al proxy inverso;
- **Motores de plantillas**: usa un crate de plantillas y devuelve `Html(...)`;
- **Parámetros parciales dentro de un segmento** (`/vuelos/:desde-:hasta`), ni parámetros opcionales (`/:id?`) ni expresiones regulares en las rutas;
- **Formularios multipart**: usa un crate específico sobre el cuerpo en bruto.

Otras cosas que conviene saber:

- los límites del motor son fijos: 60 KiB para la cabecera de la petición, 64 cabeceras, unos 60 s antes de cerrar una conexión inactiva y 10 s de periodo de gracia al apagar (consulta [Configuración del servidor](server.md));
- `body_limit` se aplica a toda la aplicación, no ruta por ruta;
- las rutas distinguen mayúsculas y minúsculas (`/Users` ≠ `/users`);
- no se pueden añadir rutas una vez arrancado el servidor;
- `req.ip()` no interpreta automáticamente `X-Forwarded-For`;
- tokio es el único runtime soportado.
