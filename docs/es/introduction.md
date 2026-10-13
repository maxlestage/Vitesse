# Introducción

Vitesse es un framework web minimalista para Rust que toma prestada la API de Express.js. Escribes `app.get("/users/:id", ...)`, lees `req.param("id")`, respondes con `res::status(201).json(...)` y encadenas middlewares con `next`. La diferencia es que todo se ejecuta como código nativo, con el rendimiento que eso trae.

## ¿Qué es Vitesse?

Vitesse («velocidad» en francés) es una biblioteca para crear servidores HTTP, sitios web y API JSON. Trae su propio motor HTTP/1.1 construido sobre [tokio](https://tokio.rs), un enrutador en árbol que solo ejecuta una expresión regular cuando una ruta la pide, una cadena de middlewares y un conjunto de middlewares listos para usar, además de WebSocket y, de forma opcional, HTTPS, HTTP/2, HTTP/3 sobre QUIC, sesiones y plantillas HTML.

Este es un servidor completo:

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    app.run(3000)
}
```

Si ya escribiste código con Express, seguramente ya sabes leerlo:

```js
const express = require('express');
const app = express();

app.get('/', (req, res) => res.send('Hello World!'));

app.listen(3000);
```

## ¿Para quién es?

- **Desarrolladores de Express y Node.js** que quieren la velocidad, el bajo consumo de memoria y la fiabilidad de Rust sin reaprender su forma de pensar un servidor web.
- **Desarrolladores de Rust** que buscan un framework pequeño y explícito. No hay extractores ni macros procedurales: un handler recibe una `Request` y devuelve una respuesta.
- **Quien paga servidores**: menos CPU por petición significa menos máquinas, o máquinas más pequeñas.

No necesitas ser experto en Rust. La mayoría de los handlers ocupan pocas líneas, y esta documentación explica las particularidades de Rust a medida que aparecen.

## Filosofía

- **Un núcleo pequeño, como Express.** Vitesse cubre el enrutamiento, los middlewares, utilidades para leer peticiones y construir respuestas, los archivos estáticos, WebSocket y un cliente de pruebas. Las sesiones y las plantillas son features opcionales, y para la base de datos y lo demás, tú eliges tus crates.
- **Una API conocida.** La mayoría de los conceptos de Express tienen su equivalente directo en Vitesse. La correspondencia completa está en [Viniendo de Express](from-express.md).
- **Sin magia.** Un handler le pide a la petición lo que necesita (`req.param("id")`, `req.json().await`). Los errores son valores normales, y `?` los convierte en respuestas HTTP.
- **Rápido por defecto.** Las cifras de abajo no requieren ningún ajuste. `app.run(3000)` ya reparte el trabajo entre todos los núcleos.
- **Robusto por defecto.** Un handler que entra en pánico da un `500`, un cuerpo demasiado grande un `413`, y una petición mal formada se rechaza. `Ctrl+C` y `SIGTERM` inician un apagado ordenado que deja terminar las peticiones en curso.

## Características principales

- **Enrutamiento al estilo Express**: `get`, `post`, `put`, `patch`, `delete`, `all`, parámetros (`/users/:id`, `/flights/:from-:to`), restricciones (`/users/:id(\d+)`), parámetros opcionales (`/archive/:year?`) y comodines (`/files/*path`). `HEAD`, `OPTIONS` y `405 Method Not Allowed` se gestionan solos, y las rutas con nombre construyen sus propias URL (`req.path_for("user", &[&42])`).
- **Entradas tipadas**: `req.param_as::<u64>("id")?`, `req.query_as::<T>()?`, `req.json::<T>().await?` y `req.form::<T>().await?` responden `400` o `413` automáticamente cuando la entrada no es válida.
- **Respuestas flexibles**: devuelve un `&str`, un `String`, `Json(...)`, `json!({...})`, `Html(...)`, una tupla `(estado, cuerpo)`, un `Option` o un `Result`. También puedes construir la respuesta tú mismo con `res::status(201).header(...).json(...)`.
- **Middlewares** en tres niveles (global, por enrutador, por ruta) con `next.run(req).await`. Incluidos: `logger`, `cors`, `helmet`, `timeout`, ids de petición, autenticación `Basic` y `Bearer`, límites de peticiones por cliente y de concurrencia, límites de cuerpo por ruta, y compresión en los dos sentidos (feature `compress`).
- **Enrutadores** que se montan bajo un prefijo, cada uno con sus propios middlewares y su fallback, **estado compartido** para toda la aplicación y **datos por petición** que añaden los middlewares.
- **Un único tipo de error** (`vitesse::Error`), que se convierte en `{"error": "..."}`. Con `app.on_error` puedes cambiar el formato de todas las respuestas de error.
- **Archivos estáticos** con tipos MIME, `ETag`/`Last-Modified`, peticiones `Range`, y protección contra `../` y los archivos ocultos.
- **Cookies, redirecciones, descargas y streaming** del cuerpo en ambos sentidos, **negociación de contenido** (`req.accepts(&["html", "json"])`) y **subida de archivos** (feature `multipart`) leída como flujo.
- **Cookies firmadas y sesiones** (feature `session`): `req.sign_cookie(...)`, y `req.session()` con un almacén en memoria o el tuyo propio.
- **Plantillas HTML** (feature `templates`): `app.views("templates")` y `req.render("page.html", json!({...}))`, con la sintaxis Jinja2 de minijinja.
- **Server-Sent Events** con `vitesse::sse`: envía actualizaciones en directo al `EventSource` del navegador.
- **WebSocket** con `app.ws("/chat", |req, socket| async move { … })`, al estilo de `express-ws`: los parámetros de ruta, los middlewares y el estado funcionan como en cualquier ruta, sobre HTTP/1.1 y HTTP/2.
- **HTTPS y HTTP/2**, con las features opcionales `tls` y `http2`: sin necesidad de proxy inverso. **HTTP/3 sobre QUIC** con la feature `http3`: la misma aplicación también responde por UDP.
- **Ajustes de producción**: keep-alive y tiempos de espera, un límite de conexiones, proxies de confianza (`app.trust_proxy`, para la dirección real del cliente), sockets Unix.
- **Interoperabilidad con tower** (feature `tower`): usa middlewares de tower y tower-http, o sirve la aplicación como un `Service` de tower.
- **Un `TestClient`** que prueba tu aplicación en memoria, sin abrir ningún puerto.
- **Un runtime gestionado**: no necesitas `#[tokio::main]`. Vitesse usa un hilo por núcleo en Linux y se apaga de forma ordenada.

## ¿Qué tan rápido es?

Peticiones por segundo y, entre paréntesis, el tiempo de CPU que consume el servidor por petición (cuanto más bajo, mejor):

| Escenario | Express 5 | Drogon 1.9 (C++) | axum 0.8 | actix-web 4 | **Vitesse** |
|---|---:|---:|---:|---:|---:|
| `GET /` (texto) | 5 921 (180 µs) | 203 244 (9,7 µs) | 168 165 (11,7 µs) | 274 195 (7,1 µs) | **291 415 (6,1 µs)** |
| `GET /json` | 5 765 (184 µs) | 120 489 (16,5 µs) | 165 988 (11,9 µs) | 248 987 (7,9 µs) | **313 438 (5,9 µs)** |
| `GET /json` enviado por un navegador (12 cabeceras) | 5 627 (188 µs) | 92 041 (21,7 µs) | 131 710 (15,0 µs) | 179 505 (10,9 µs) | **290 478 (6,8 µs)** |
| `GET /users/:id` (parámetro + JSON) | 5 627 (187 µs) | 98 486 (20,1 µs) | 151 303 (13,1 µs) | 209 173 (9,4 µs) | **304 323 (6,2 µs)** |
| `POST /echo` (lee y devuelve JSON) | 4 538 (235 µs) | 69 876 (28,4 µs) | 105 570 (18,8 µs) | 170 129 (11,7 µs) | **253 941 (7,6 µs)** |
| `GET /` con pipelining ×16 | 8 352 (128 µs) | 724 059 (2,7 µs) | 213 678 (9,3 µs) | 1 272 510 (1,6 µs) | **2 781 541 (0,64 µs)** |

En resumen:

- **Frente a actix-web**, al que a menudo se considera el framework de Rust más rápido: hasta un **+62 %** de rendimiento con una petición real de navegador, entre un +45 y un +49 % con parámetros o un cuerpo JSON, **2,2 veces más** con pipelining y entre un 15 y un 59 % menos de CPU por petición.
- **Frente a axum**: entre 1,7 y 2,4 veces más peticiones por segundo, la mitad de CPU por petición y 13 veces más con pipelining.
- **Frente a Drogon (C++)**: entre 1,4 y 3,8 veces más rápido.
- **Frente a Express**: unas 50 veces más rápido.

Todas estas mediciones se hicieron en una VM de 4 vCPU: el servidor fijado en 2 núcleos, wrk en los otros 2, 128 conexiones keep-alive y 10 s por escenario. El servidor Express usado para estas cifras se retiró después del repositorio para que siga siendo 100 % Rust (sigue en el historial de git), y el benchmark compara ahora Drogon, axum, actix-web y Vitesse. La página de [Rendimiento](performance.md) explica la metodología, cómo reproducir las cifras (con la herramienta en Rust de `bench/runner`) y por qué Vitesse es tan rápido. Los resultados en bruto también están en el README.

## Limitaciones actuales

Algunas cosas se dejan fuera, cada una por un motivo:

- Extractores: por diseño, un handler recibe una sola `Request` y le pide lo que necesita.
- WebSocket sobre HTTP/3: los navegadores no dependen de él, y abren sus [WebSockets](websocket.md) sobre HTTP/2 o HTTP/1.1.
- El server push de HTTP/2 y `Upgrade: h2c`, que los navegadores y la especificación de HTTP/2 han abandonado, y 0-RTT sobre HTTP/3, que expone las peticiones a reenvíos.
- Runtimes distintos de tokio.
- Una API 1.x estable: Vitesse está en la versión 0.1.

Las [preguntas frecuentes](faq.md#límites-actuales) explican los motivos en detalle.

## Cómo está organizada esta documentación

- **Primeros pasos**: [Instalación](installation.md) y luego [Tu primera aplicación](first-app.md), un tutorial paso a paso.
- **Lo esencial**: [Enrutamiento](routing.md), [Leer la petición](requests.md), [Enviar la respuesta](responses.md), [Plantillas HTML](templates.md), [Middlewares](middleware.md), [Enrutadores](routers.md), [Estado compartido](state.md), [Cookies firmadas y sesiones](sessions.md), [Gestión de errores](errors.md), [Archivos estáticos](static-files.md) y [WebSocket](websocket.md).
- **Para ir más allá**: [Pruebas](testing.md), [Configuración del servidor](server.md) (direcciones, hilos, tiempos de espera, apagado ordenado), [HTTPS y HTTP/2](https.md), [HTTP/3 y QUIC](http3.md), [Rendimiento](performance.md), [Viniendo de Express](from-express.md) y [Viniendo de axum o actix-web](from-axum-actix.md).
- **Despliegue**: [Desplegar en Heroku desde el móvil](heroku-mobile.md), [Docker](docker.md) y [Puesta en producción](production.md).
- **Referencia**: la [chuleta](cheatsheet.md) y las [preguntas frecuentes](faq.md).

> [!TIP]
> Si ya conoces bien Express, haz [Tu primera aplicación](first-app.md) y luego ten a mano [Viniendo de Express](from-express.md) mientras escribes tus propias rutas.

La referencia completa de la API se genera a partir del código fuente. Ejecuta `cargo doc --open` en un proyecto que dependa de Vitesse, o explora el código en [GitHub](https://github.com/maxlestage/Vitesse).
