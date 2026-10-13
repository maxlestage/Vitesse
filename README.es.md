[![Vitesse: la comodidad de Express, la velocidad de Rust](https://raw.githubusercontent.com/maxlestage/Vitesse/master/site/assets/og-image.png)](https://maxlestage.github.io/Vitesse/)

[English](README.md) · [Français](README.fr.md) · **Español**

# Vitesse ⚡

**La comodidad de Express.js, la velocidad de Rust.**

[![crates.io](https://img.shields.io/crates/v/vitesse.svg)](https://crates.io/crates/vitesse)
[![docs.rs](https://img.shields.io/docsrs/vitesse)](https://docs.rs/vitesse)
[![Licencia: propietaria](https://img.shields.io/badge/license-proprietary-2a2a2e.svg)](#licencia)

Vitesse es un framework web minimalista que lleva la API de Express
(`app.get`, `req.params`, `res.status(201).json(...)`, `app.use`, `Router`,
`express.static`…) a Rust nativo, con su propio motor HTTP/1.1 sobre
[tokio](https://tokio.rs), WebSocket integrado, y HTTPS, HTTP/2 y HTTP/3
sobre QUIC como opciones.

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    app.run(3000)
}
```

- **Familiar**: rutas y parámetros, middlewares con `next`, routers,
  archivos estáticos, cookies, JSON, formularios y subida de archivos, y un
  cliente de pruebas en memoria.
- **Tiempo real y protocolos modernos**: WebSocket con `app.ws`, Server-Sent
  Events, HTTPS y HTTP/2 (features `tls` y `http2`), y HTTP/3 sobre QUIC con
  una sola línea (feature `http3`).
- **Todo incluido**: CORS, compresión, IDs de petición, autenticación Basic
  y Bearer, limitación de tasa y de concurrencia, proxies de confianza,
  cookies firmadas y sesiones (feature `session`), plantillas HTML (feature
  `templates`), y cualquier capa de [tower](https://crates.io/crates/tower)
  (feature `tower`).
- **Rutas expresivas**: `/flights/:from-:to`, `/users/:id(\d+)`,
  `/archive/:year?`, rutas con nombre y generación de URL.
- **Rápido**: más rápido que actix-web, axum y Drogon en el benchmark de
  abajo.
- **Robusto**: los pánicos se convierten en respuestas `500`, los límites de
  tamaño y los tiempos de espera vienen integrados, y el servidor se apaga de
  forma ordenada.

## Benchmark

Peticiones por segundo y, entre paréntesis, el tiempo de CPU que consume el
servidor por cada petición (cuanto menos, mejor):

| Escenario | Express 5 | Drogon 1.9 (C++) | axum 0.8 | actix-web 4 | **Vitesse** |
|---|---:|---:|---:|---:|---:|
| `GET /` (texto) | 5.921 (180 µs) | 203.244 (9,7 µs) | 168.165 (11,7 µs) | 274.195 (7,1 µs) | **291.415 (6,1 µs)** |
| `GET /json` | 5.765 (184 µs) | 120.489 (16,5 µs) | 165.988 (11,9 µs) | 248.987 (7,9 µs) | **313.438 (5,9 µs)** |
| `GET /json` enviado por un navegador (12 cabeceras) | 5.627 (188 µs) | 92.041 (21,7 µs) | 131.710 (15,0 µs) | 179.505 (10,9 µs) | **290.478 (6,8 µs)** |
| `GET /users/:id` (parámetro + JSON) | 5.627 (187 µs) | 98.486 (20,1 µs) | 151.303 (13,1 µs) | 209.173 (9,4 µs) | **304.323 (6,2 µs)** |
| `POST /echo` (lee y devuelve JSON) | 4.538 (235 µs) | 69.876 (28,4 µs) | 105.570 (18,8 µs) | 170.129 (11,7 µs) | **253.941 (7,6 µs)** |
| `GET /` con pipelining ×16 | 8.352 (128 µs) | 724.059 (2,7 µs) | 213.678 (9,3 µs) | 1.272.510 (1,6 µs) | **2.781.541 (0,64 µs)** |

- **Frente a actix-web**, el framework de Rust con fama de ser el más
  rápido: hasta un **+62 %** de rendimiento con una petición real de
  navegador, del +45 al +49 % con parámetros o un cuerpo JSON, **2,2 veces
  más** con pipelining y entre un 15 y un 59 % menos de CPU por petición.
- **Frente a axum**: de 1,7 a 2,4 veces más peticiones por segundo, la mitad
  de CPU por petición y 13 veces más con pipelining.
- **Frente a Drogon (C++)**: de 1,4 a 3,8 veces más rápido.
- **Frente a Express**: unas 50 veces más rápido.

**¿Por qué la diferencia con actix es menor en `GET /`?** En la petición más
simple, todos los servidores rápidos chocan con el mismo suelo: unos 4,7 µs
de trabajo del kernel por petición (lectura, escritura y, en la interfaz de
loopback, el procesamiento de la recepción del lado del cliente, que se
imputa al envío del servidor). Vitesse solo añade ~1,4 µs encima, actix
~2,4 µs y axum ~7 µs. En cuanto la petición se parece a una real (cabeceras
de navegador, parámetros, cuerpo JSON, pipelining), el código del framework
marca la diferencia y la brecha se amplía. En producción, a través de una
red real, la parte del trabajo del kernel del lado del servidor es menor,
así que la ventaja de Vitesse se nota todavía más.

<sub>VM de 4 vCPU: servidor fijado en 2 núcleos, wrk
en los otros 2, 128 conexiones keep-alive, 10 s por escenario, misma máquina
y misma sesión para todos. Node 22.22 / Express 5.3.0, Drogon 1.9.13 (GCC 13,
`-O3`), axum 0.8, actix-web 4.15, Rust 1.97, asignador del sistema en todos.
Sin pipelining, los servidores más rápidos saturan wrk: el tiempo de CPU por
petición, medido del lado del servidor, es entonces el juez más fiable. Las
mediciones varían unos pocos puntos porcentuales de una ejecución a otra.
Para reproducirlo:
`cargo run --release --manifest-path bench/runner/Cargo.toml` (código de
los servidores en `bench/`). El servidor Express, medido en la misma sesión,
se retiró después del repositorio para que el proyecto siga siendo 100 %
Rust, sin JavaScript: sigue en el historial de git
(`git show 484eed3:bench/express/server.js`), y la herramienta de benchmark
compara ahora Drogon, axum, actix-web y Vitesse.</sub>

## Instalación

```sh
cargo add vitesse --git https://github.com/maxlestage/Vitesse
cargo add serde --features derive   # para tus estructuras JSON
```

Vitesse requiere Rust 1.85 o posterior. No hace falta `#[tokio::main]`:
`app.run(port)` crea el runtime, usa todos los núcleos y se apaga de forma
ordenada con `Ctrl+C` / `SIGTERM`. Si ya tienes un runtime de tokio, usa
`app.listen(port).await` en su lugar.

### Features de Cargo

| Feature | Por defecto | Añade |
|---|---|---|
| `ws` | Activada | WebSocket: `app.ws(...)` y `vitesse::ws` ([guía](docs/es/websocket.md)) |
| `tls` | Desactivada | HTTPS: `app.tls(...)` y `vitesse::tls`, con rustls ([guía](docs/es/https.md)) |
| `http2` | Desactivada | HTTP/2: mediante ALPN con `tls`, y h2c sin cifrar ([guía](docs/es/https.md)) |
| `http3` | Desactivada | HTTP/3 sobre QUIC: `app.http3(...)` y `vitesse::http3` ([guía](docs/es/http3.md)) |
| `multipart` | Desactivada | Subida de archivos: `req.multipart()` y `vitesse::multipart` ([guía](docs/es/requests.md)) |
| `compress` | Desactivada | `middleware::compress()` y `middleware::decompress()`: Brotli, gzip ([guía](docs/es/middleware.md#compress)) |
| `session` | Desactivada | Cookies firmadas y sesiones: `app.cookie_secret(...)`, `middleware::session(...)` ([guía](docs/es/sessions.md)) |
| `templates` | Desactivada | Plantillas HTML con minijinja: `app.views(...)`, `req.render(...)` ([guía](docs/es/templates.md)) |
| `tower` | Desactivada | Capas de tower como middlewares, la aplicación como `Service` de tower ([guía](docs/es/middleware.md)) |

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tls", "http2"] }
```

## Recorrido rápido

```rust
use serde::Deserialize;
use vitesse::prelude::*;

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

// Middleware de ruta: solo deja pasar las peticiones con el token correcto.
async fn auth(req: Request, next: Next) -> Response {
    match req.header("authorization") {
        Some("Bearer secret") => next.run(req).await,
        _ => Error::unauthorized("token ausente o no válido").into_response(),
    }
}

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    // Middlewares globales (app.use): para cada petición, en orden.
    app.middleware(middleware::logger());
    app.middleware(|req: Request, next: Next| async move {
        let start = std::time::Instant::now();
        let res = next.run(req).await;
        res.header("x-response-time", format!("{:.3}ms", start.elapsed().as_secs_f64() * 1000.0))
    });

    app.get("/", |_| async { "Hello World!" });

    // Parámetros de ruta: 400 automático si `id` no es un número.
    app.get("/users/:id", |req: Request| async move {
        let id: u32 = req.param_as("id")?;
        Ok::<_, Error>(Json(json!({ "id": id, "name": "Ada" })))
    });

    // Cuerpo JSON de entrada, 201 Created de salida (400 si el JSON no es válido).
    app.post("/users", |req: Request| async move {
        let user: NewUser = req.json().await?;
        Ok::<_, Error>((201, Json(json!({ "name": user.name }))))
    });

    // Un router montado bajo un prefijo, protegido por un middleware.
    let mut admin = Router::new();
    admin.middleware(auth);
    admin.get("/stats", |_| async { json!({ "users": 1 }) });
    app.mount("/admin", admin);

    app.run(3000)
}
```

```sh
curl localhost:3000/users/42      # {"id":42,"name":"Ada"}
curl -X POST localhost:3000/users -H 'content-type: application/json' -d '{"name":"Ada"}'
curl localhost:3000/admin/stats -H 'authorization: Bearer secret'
```

### WebSocket

```rust
// Una ruta WebSocket, al estilo de express-ws: la petición y después el socket.
app.ws("/echo/:name", |req, mut socket| async move {
    let name = req.param("name").unwrap_or("desconocido").to_owned();
    while let Some(Ok(message)) = socket.recv().await {
        if let ws::Message::Text(text) = message {
            if socket.send(format!("{name} dijo: {text}")).await.is_err() {
                break;
            }
        }
    }
});
```

Pruébala con [websocat](https://crates.io/crates/websocat):
`websocat ws://localhost:3000/echo/ada`. Subprotocolos, límites de tamaño,
`split` y una sala de chat completa: [WebSocket](docs/es/websocket.md).

### HTTP/3

```rust
use vitesse::http3::Http3;

// Con la feature `http3`: la misma aplicación, servida también sobre QUIC (UDP).
app.http3(Http3::from_pem_files("fullchain.pem", "privkey.pem")?);
app.run(443) // HTTP/1.1 en TCP 443, HTTP/3 en UDP 443
```

Certificados, `Alt-Svc` y despliegue detrás de Caddy o Nginx:
[HTTP/3 y QUIC](docs/es/http3.md).

### HTTPS y HTTP/2

```rust
use vitesse::tls::Tls;

// Con las features `tls` y `http2`: TLS con rustls, HTTP/2 para los navegadores.
app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?);
app.run(443)
```

Sin TLS, la feature `http2` también responde a los clientes que hablan
HTTP/2 sin cifrar (h2c). Consulta [HTTPS y HTTP/2](docs/es/https.md).

### Server-Sent Events

```rust
use vitesse::sse::Event;

app.get("/clock", |_| async {
    let (tx, events) = sse::channel(16);
    tokio::spawn(async move {
        for tick in 0.. {
            // Falla en cuanto el navegador se ha ido.
            if tx.send(Event::new().data(tick.to_string())).await.is_err() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });
    events
});
```

## Documentación

- **Sitio web**: https://maxlestage.github.io/Vitesse/es/docs/
- **En este repositorio**: [docs/es/README.md](docs/es/README.md), desde
  [tu primera aplicación](docs/es/first-app.md) hasta la
  [puesta en producción](docs/es/production.md), con una guía para quienes
  [vienen de Express](docs/es/from-express.md), y las guías de
  [WebSocket](docs/es/websocket.md) y [HTTP/3](docs/es/http3.md).
- **Referencia de la API**: [docs.rs/vitesse](https://docs.rs/vitesse)

La documentación también está disponible en [inglés](docs/en/README.md) y en
[francés](docs/fr/README.md).

## Desplegar en Heroku

Abre `https://heroku.com/deploy?template=https://github.com/maxlestage/Vitesse` en tu
móvil para desplegar el sitio web de Vitesse (`site/`, servido por el propio Vitesse,
con su documentación) en tu cuenta de Heroku: Heroku lee `app.json` y lo compila por sí
mismo con el buildpack de Rust (`emk/rust`; también es posible una construcción Docker
de la aplicación de demostración, `examples/demo.rs`), así que
ni siquiera necesitas una computadora (Heroku ya no tiene plan gratuito:
hace falta un dyno de pago). La guía paso a paso también explica los
despliegues automáticos con GitHub Actions:
[Desplegar en Heroku desde el móvil](docs/es/heroku-mobile.md).

## De Express a Vitesse

| Express | Vitesse |
|---|---|
| `const app = express()` | `let mut app = App::new();` |
| `app.get('/u/:id', (req, res) => …)` | `app.get("/u/:id", \|req: Request\| async move { … })` |
| `app.use(fn)` | `app.middleware(fn)` |
| `app.use('/api', router)` | `app.mount("/api", router)` |
| `express.Router()` | `Router::new()` |
| `app.use(express.static('public'))` | `app.middleware(ServeDir::new("public"))` |
| `app.ws('/chat', (ws, req) => …)` (express-ws) | `app.ws("/chat", \|req, socket\| async move { … })` |
| `express.json()` + `req.body` | `req.json::<T>().await?` |
| `next()` | `next.run(req).await` |
| `req.params.id` | `req.param("id")` o `req.param_as::<u64>("id")?` |
| `req.query.q` | `req.query("q")` o `req.query_as::<T>()?` |
| `res.send('texto')` | devolver `"texto"` |
| `res.status(201).json(obj)` | `res::status(201).json(obj)` o `(201, Json(obj))` |
| `res.redirect('/login')` | `Redirect::to("/login")` |
| `app.listen(3000)` | `app.run(3000)` |

La tabla completa está en [Viniendo de Express](docs/es/from-express.md).
¿Vienes de los frameworks del propio Rust? [Viniendo de axum o
actix-web](docs/es/from-axum-actix.md) relaciona sus funcionalidades,
extractor por extractor y capa por capa.

## Por qué es rápido

Casi todo el tiempo de una petición simple se pasa en el kernel, leyendo y
escribiendo el socket: un servidor rápido es el que añade lo menos posible
alrededor. Vitesse hace exactamente **un `read` y un `write` por petición**,
y uno solo de cada para todo un lote de peticiones con pipelining.

- **Su propio motor HTTP/1.1**: la cabecera de las peticiones se analiza con
  httparse (SIMD) sin copias, las
  cabeceras y la URI solo se construyen si un handler las pide, y las
  respuestas se escriben directamente en un búfer reutilizado.
- **Casi ninguna asignación de memoria**: las peticiones, los búferes y las
  tablas de cabeceras de las respuestas se reciclan por thread.
- **Un thread por núcleo** (Linux): cada núcleo tiene su propio bucle de
  eventos y su propio socket `SO_REUSEPORT`, y una petición nunca cambia de
  thread.
- **Un router en árbol**: un árbol de segmentos, sin asignaciones para las
  rutas estáticas; las expresiones regulares solo se ejecutan para los
  parámetros que declaran una restricción.
- **Cero contadores atómicos compartidos por petición**: la aplicación se
  congela al arrancar (`&'static`), así que handlers, middlewares y estado
  se leen sin `Arc`.

Los detalles están en [Rendimiento](docs/es/performance.md).

## Límites actuales

No incluye, a propósito: WebSocket sobre HTTP/3 (ningún navegador lo usa),
el server push de HTTP/2 (eliminado de los navegadores), el mecanismo
`Upgrade: h2c` (eliminado del estándar HTTP/2; h2c «prior knowledge»
funciona) ni 0-RTT sobre HTTP/3 (peticiones que se pueden reenviar). tokio
es el único runtime, y la API puede cambiar hasta la 1.0. La lista completa
está en las [preguntas frecuentes](docs/es/faq.md).

## Ejecutar el proyecto

```sh
cargo run --release --example hello      # Hello World
cargo run --release --example rest_api   # API CRUD completa
cargo run --release --example demo       # la demo, construcción Docker de Heroku (lee $PORT)
cargo run --release --example chat       # sala de chat WebSocket (websocat ws://localhost:3000/chat/ada)
cargo run --release --example http3 --features http3   # HTTP/1.1 + HTTP/3 en el puerto 4433, certificado autofirmado
cargo run --release --example https --features tls,http2   # HTTPS + HTTP/2 en el puerto 3443, certificado autofirmado
cargo run --release --example live --features multipart    # Server-Sent Events y subida de archivos
cargo test                               # tests unitarios, de integración y doctests
cargo run --release --manifest-path bench/runner/Cargo.toml   # benchmark (Linux, wrk, y Drogon si está instalado)
docker build -t vitesse-demo . && docker run --rm -p 8080:8080 vitesse-demo
```

### El sitio de presentación

La carpeta [`site/`](site) contiene el sitio web del proyecto, en español,
inglés y francés, con la documentación de [`docs/`](docs). Lo sirve el
propio Vitesse: el servidor es una aplicación Vitesse cuyas páginas están
escritas con [Active](https://maxlestage.github.io/Active/) (renderizadas en
el servidor, con todas las etiquetas SEO), y las partes interactivas son
islas de Active compiladas a WebAssembly. Es una Progressive Web App: se
instala en la pantalla de inicio de un móvil y sigue funcionando sin
conexión. El workflow `Site` lo exporta a archivos estáticos y lo publica en
GitHub Pages cada vez que cambia en `master`.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129   # la versión de site/Cargo.lock
cd site
# 1. El código del navegador (islas y animaciones) → site/pkg
cargo build --lib --target wasm32-unknown-unknown --profile wasm-release
wasm-bindgen --target web --no-typescript --out-dir pkg target/wasm32-unknown-unknown/wasm-release/vitesse_site.wasm
# 2. El servidor, en http://localhost:3000 (PORT para cambiar de puerto)
cargo run --release
# O todo el sitio en archivos estáticos, detrás de la ruta de GitHub Pages
BASE_PATH=/Vitesse SITE_URL=https://maxlestage.github.io/Vitesse cargo run --release -- export dist
```

El JavaScript que los navegadores necesitan para arrancar el WebAssembly y el
service worker se generan (con wasm-bindgen y en el servidor): el
repositorio sigue sin contener ningún archivo JavaScript.

## Contribuir

Los informes de errores, las ideas, las correcciones de la documentación y
las pull requests son bienvenidos. Para un cambio grande, abre primero un
issue para hablarlo.

Antes de abrir una pull request, ejecuta las mismas comprobaciones que la CI
(que se ejecuta en Linux, macOS y Windows):

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

La herramienta de benchmark ([`bench/runner`](bench/runner)) y el sitio web
([`site/`](site)) son crates aparte, fuera de la compilación principal: si
los modificas, ejecuta también `cargo test` en su carpeta (para el sitio,
también `cargo clippy --lib --target wasm32-unknown-unknown -- -D warnings`).

La documentación está en [`docs/`](docs), en inglés, francés y español:
cuando modifiques una página, actualiza también los otros idiomas, o
indícalo en tu pull request. Los cambios importantes se anotan en
[`CHANGELOG.md`](CHANGELOG.md).

### Publicar una versión (mantenedores)

1. Sube `version` en `Cargo.toml` y escribe su sección en `CHANGELOG.md`.
2. Haz push a `master`. Cuando la CI está en verde, el workflow `Release`
   crea el tag de la nueva versión (`vX.Y.Z`) y su release de GitHub con esa
   sección de `CHANGELOG.md` como notas, ejecuta los tests y publica el
   crate en crates.io. No hay ningún formulario que rellenar.
3. La publicación necesita el secreto de repositorio `CARGO_REGISTRY_TOKEN`
   (un token de la API de crates.io con los permisos `publish-new` y
   `publish-update`). Sin él, el tag y la release de GitHub se crean igual,
   y la publicación espera: añade el secreto y lanza el workflow a mano
   desde la pestaña **Actions**, con la casilla **Only check the package**
   desmarcada. Una release redactada a mano (**Releases** → **Draft a new
   release**) también publica, y un lanzamiento manual es por defecto en
   modo de prueba («dry run»).

## Licencia

Vitesse es software propietario: © 2026 Maxime Nathan Lestage, todos los
derechos reservados. No es software de código abierto. El código fuente se
publica en crates.io y GitHub para que pueda leerse, auditarse y compilarse,
y puedes usarlo libremente como dependencia de tus propios programas, también
comerciales. Modificarlo o redistribuir su código fuente requiere el permiso
escrito del autor. Condiciones completas: [LICENSE](LICENSE).

Las contribuciones (código, documentación, ideas) son bienvenidas y pasan a
formar parte del proyecto en los mismos términos: al enviar una, cedes sus
derechos al autor, como indica la licencia.

## Créditos

Diseñado y desarrollado por Maxime Nathan Lestage,
dentro de una misma familia de proyectos Rust: [Active](https://maxlestage.github.io/Active/), [Vitesse](https://maxlestage.github.io/Vitesse/), [Securite](https://maxlestage.github.io/Securite/), [Memoire](https://maxlestage.github.io/Memoire/), [Paquets](https://maxlestage.github.io/Paquets/), [FerroSwift](https://maxlestage.github.io/FerroSwift/) y [Solidify](https://maxlestage.github.io/Solidify/).
