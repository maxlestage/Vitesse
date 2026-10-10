[English](README.md) · [Français](README.fr.md) · **Español**

# Vitesse ⚡

**La comodidad de Express.js, la velocidad de Rust.**

[![CI](https://github.com/maxlestage/Vitesse/actions/workflows/ci.yml/badge.svg)](https://github.com/maxlestage/Vitesse/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/vitesse.svg)](https://crates.io/crates/vitesse)
[![docs.rs](https://img.shields.io/docsrs/vitesse)](https://docs.rs/vitesse)
[![Licencia: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#licencia)

Vitesse es un framework web minimalista que lleva la API de Express
(`app.get`, `req.params`, `res.status(201).json(...)`, `app.use`, `Router`,
`express.static`…) a Rust nativo, con su propio motor HTTP/1.1 sobre
[tokio](https://tokio.rs).

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    app.run(3000)
}
```

- **Familiar**: rutas y parámetros, middlewares con `next`, routers,
  archivos estáticos, cookies, JSON y formularios, y un cliente de pruebas en
  memoria.
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
- **Frente a Express**: unas 50 veces más rápido (y todavía 25 veces más que
  Express en modo clúster con los mismos 2 núcleos).

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

<sub>VM de 4 vCPU: servidor fijado en 2 núcleos, [wrk](https://github.com/wg/wrk)
en los otros 2, 128 conexiones keep-alive, 10 s por escenario, misma máquina
y misma sesión para todos. Node 22.22 / Express 5.3.0, Drogon 1.9.13 (GCC 13,
`-O3`), axum 0.8, actix-web 4.15, Rust 1.97, asignador del sistema en todos.
Sin pipelining, los servidores más rápidos saturan wrk: el tiempo de CPU por
petición, medido del lado del servidor, es entonces el juez más fiable. Las
mediciones varían unos pocos puntos porcentuales de una ejecución a otra.
Para reproducirlo: `bench/run.sh` (código de los servidores en `bench/`).</sub>

## Instalación

```sh
cargo add vitesse
cargo add serde --features derive   # para tus estructuras JSON
```

O, para seguir la versión en desarrollo en GitHub:

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse" }
serde = { version = "1", features = ["derive"] }
```

Vitesse requiere Rust 1.85 o posterior. No hace falta `#[tokio::main]`:
`app.run(port)` crea el runtime, usa todos los núcleos y se apaga de forma
ordenada con `Ctrl+C` / `SIGTERM`. Si ya tienes un runtime de tokio, usa
`app.listen(port).await` en su lugar.

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

## Documentación

- **Sitio web**: https://maxlestage.github.io/Vitesse/#/docs
- **En este repositorio**: [docs/es/README.md](docs/es/README.md), desde
  [tu primera aplicación](docs/es/first-app.md) hasta la
  [puesta en producción](docs/es/production.md), con una guía para quienes
  [vienen de Express](docs/es/from-express.md).
- **Referencia de la API**: [docs.rs/vitesse](https://docs.rs/vitesse)

La documentación también está disponible en [inglés](docs/en/README.md) y en
[francés](docs/fr/README.md).

## Desplegar en Heroku

[![Deploy to Heroku](https://www.herokucdn.com/deploy/button.svg)](https://www.heroku.com/deploy?template=https://github.com/maxlestage/Vitesse)

Un solo toque despliega la aplicación de demostración (`examples/demo.rs`)
en tu cuenta de Heroku: Heroku construye la imagen Docker por sí mismo, así
que ni siquiera necesitas una computadora (Heroku ya no tiene plan gratuito:
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
| `express.json()` + `req.body` | `req.json::<T>().await?` |
| `next()` | `next.run(req).await` |
| `req.params.id` | `req.param("id")` o `req.param_as::<u64>("id")?` |
| `req.query.q` | `req.query("q")` o `req.query_as::<T>()?` |
| `res.send('texto')` | devolver `"texto"` |
| `res.status(201).json(obj)` | `res::status(201).json(obj)` o `(201, Json(obj))` |
| `res.redirect('/login')` | `Redirect::to("/login")` |
| `app.listen(3000)` | `app.run(3000)` |

La tabla completa está en [Viniendo de Express](docs/es/from-express.md).

## Por qué es rápido

Casi todo el tiempo de una petición simple se pasa en el kernel, leyendo y
escribiendo el socket: un servidor rápido es el que añade lo menos posible
alrededor. Vitesse hace exactamente **un `read` y un `write` por petición**,
y uno solo de cada para todo un lote de peticiones con pipelining.

- **Su propio motor HTTP/1.1**: la cabecera de las peticiones se analiza con
  [httparse](https://github.com/seanmonstar/httparse) (SIMD) sin copias, las
  cabeceras y la URI solo se construyen si un handler las pide, y las
  respuestas se escriben directamente en un búfer reutilizado.
- **Casi ninguna asignación de memoria**: las peticiones, los búferes y las
  tablas de cabeceras de las respuestas se reciclan por thread.
- **Un thread por núcleo** (Linux): cada núcleo tiene su propio bucle de
  eventos y su propio socket `SO_REUSEPORT`, y una petición nunca cambia de
  thread.
- **Un router sin regex**: un árbol de segmentos, sin asignaciones para las
  rutas estáticas.
- **Cero contadores atómicos compartidos por petición**: la aplicación se
  congela al arrancar (`&'static`), así que handlers, middlewares y estado
  se leen sin `Arc`.

Los detalles están en [Rendimiento](docs/es/performance.md).

## Límites actuales

Como Express, Vitesse hace pocas cosas a propósito. No incluye (todavía):
HTTP/2 ni TLS (colócalo detrás de un proxy inverso como Nginx o Caddy, como
se suele hacer con Express: consulta
[Puesta en producción](docs/es/production.md)), WebSocket, compresión,
motores de plantillas ni parámetros parciales dentro de un segmento
(`/vuelos/:origen-:destino`).

## Ejecutar el proyecto

```sh
cargo run --release --example hello      # Hello World
cargo run --release --example rest_api   # API CRUD completa
cargo run --release --example demo       # la app de demostración desplegada en Heroku (lee $PORT)
cargo test                               # tests unitarios, de integración y doctests
bench/run.sh                             # benchmark (wrk, Node.js, y Drogon si está instalado)
docker build -t vitesse-demo . && docker run --rm -p 8080:8080 vitesse-demo
```

### El sitio de presentación

La carpeta [`site/`](site) contiene el sitio web del proyecto, escrito en
Rust con [Yew](https://yew.rs) y compilado a WebAssembly con
[Trunk](https://trunkrs.dev). El workflow `Site` lo publica en GitHub Pages
cada vez que cambia en `master`.

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
cd site && trunk serve --open            # http://127.0.0.1:8080, recarga en caliente
```

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

La documentación está en [`docs/`](docs), en inglés, francés y español:
cuando modifiques una página, actualiza también los otros idiomas, o
indícalo en tu pull request. Los cambios importantes se anotan en
[`CHANGELOG.md`](CHANGELOG.md).

### Publicar una versión (mantenedores)

1. Sube `version` en `Cargo.toml` y actualiza `CHANGELOG.md`.
2. En GitHub, abre **Releases** → **Draft a new release**, crea un tag
   `vX.Y.Z` que coincida con la versión y publica la release.
3. El workflow `Release` ejecuta los tests y publica el crate en crates.io.
   Necesita el secreto de repositorio `CARGO_REGISTRY_TOKEN` (un token de la
   API de crates.io con los permisos `publish-new` y `publish-update`).
   También se puede lanzar a mano desde la pestaña **Actions**, por defecto
   en modo de prueba («dry run»).

## Licencia

Con licencia [Apache 2.0](LICENSE-APACHE) o [MIT](LICENSE-MIT), a tu
elección.

Salvo que indiques expresamente lo contrario, cualquier contribución que
envíes intencionadamente para su inclusión en este proyecto, tal como se
define en la licencia Apache-2.0, quedará bajo esta doble licencia, sin
términos ni condiciones adicionales.
