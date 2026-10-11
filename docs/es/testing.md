# Pruebas

Vitesse incluye un cliente de pruebas en memoria, `vitesse::test::TestClient`, que envía las peticiones directamente a tu aplicación sin abrir ningún puerto. Cumple el papel de supertest en el mundo Express: las pruebas son rápidas, se ejecutan en paralelo y no necesitan ningún servidor en marcha.

## Preparación

`TestClient` forma parte de Vitesse: solo necesitas un runtime asíncrono para tus pruebas. Añade tokio como dependencia de desarrollo (no hace falta si ya está en tus `[dependencies]` con las features `macros` y `rt`):

```toml
[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt"] }
```

Cada prueba es una `async fn` marcada con `#[tokio::test]`, que arranca un pequeño runtime solo para esa prueba.

## Haz que tu aplicación se pueda probar

Las pruebas tienen que construir la misma aplicación que `main`, pero sin arrancar el servidor. Lo más sencillo es una función que devuelva la `App`, en `src/lib.rs`:

```rust
// src/lib.rs
use vitesse::prelude::*;

pub fn app() -> App {
    let mut app = App::new();
    app.get("/hello/:name", |req: Request| async move {
        format!("¡Hola, {}!", req.param("name").unwrap_or("desconocido"))
    });
    app
}
```

```rust
// src/main.rs
fn main() -> std::io::Result<()> {
    my_api::app().run(3000) // `my_api` es el nombre de tu paquete
}
```

Las pruebas de integración de la carpeta `tests/` ya pueden llamar a `my_api::app()`.

> [!TIP]
> Es el mismo patrón que `module.exports = app` en `app.js` con `app.listen()` en `server.js`: definir la aplicación y arrancar el servidor son dos pasos separados.

## Tu primera prueba

```rust
// tests/hello.rs
use vitesse::test::TestClient;

#[tokio::test]
async fn saluda() {
    let client = TestClient::new(my_api::app());

    let res = client.get("/hello/Ada").await;

    assert_eq!(res.status(), 200);
    assert_eq!(res.header("content-type"), Some("text/plain; charset=utf-8"));
    assert_eq!(res.text(), "¡Hola, Ada!");
}
```

Ejecútala con `cargo test`. `TestClient::new` recibe la `App` y la congela, igual que haría `app.run`. A partir de ahí, cada petición pasa por todo lo que pasaría una petición real: middlewares globales, enrutamiento, middlewares de ruta, el handler, `app.on_error`, `app.body_limit` y los pánicos convertidos en respuestas `500`.

## Construir peticiones

Empieza la petición desde el cliente:

| Método | Petición |
|---|---|
| `client.get(uri)` | `GET` |
| `client.post(uri)` | `POST` |
| `client.put(uri)` | `PUT` |
| `client.patch(uri)` | `PATCH` |
| `client.delete(uri)` | `DELETE` |
| `client.request(Method::HEAD, uri)` | cualquier otro método (`HEAD`, `OPTIONS`…) |

Después encadena lo que necesites y envíala con `.await`:

| Método | Efecto |
|---|---|
| `.header(nombre, valor)` | Añade una cabecera (entra en pánico si el nombre o el valor no son válidos) |
| `.body(datos)` | Cuerpo en bruto: `&'static str`, `String`, `Vec<u8>`, `Bytes`… |
| `.json(&valor)` | Serializa `valor` con serde y pone `content-type: application/json` |
| `.form(&valor)` | Formulario codificado, con `content-type: application/x-www-form-urlencoded` |
| `.await` o `.send().await` | Envía la petición y devuelve una `TestResponse` |

```rust
let res = client
    .post("/items")
    .header("authorization", "Bearer secret")
    .json(&json!({ "name": "Ada" }))
    .await;

// La query string va en la URI, ya codificada.
let res = client.get("/search?q=caf%C3%A9+cr%C3%A8me").await;

let res = client.post("/login").form(&[("user", "ada"), ("remember", "true")]).await;
let res = client.request(Method::OPTIONS, "/items").await;
```

> [!NOTE]
> Una petición de prueba es perezosa: no se envía nada hasta que haces `.await`. Si te olvidas del `.await`, el compilador te avisa.

## Leer la respuesta

Una `TestResponse` ya tiene todo su cuerpo en memoria:

| Método | Devuelve |
|---|---|
| `res.status()` | El `StatusCode`, que se puede comparar con un número: `assert_eq!(res.status(), 404)` |
| `res.header(nombre)` | Una cabecera, como `Option<&str>` |
| `res.headers()` | Todas las cabeceras (`&HeaderMap`) |
| `res.text()` | El cuerpo como `String` |
| `res.bytes()` | El cuerpo en bruto (`&Bytes`) |
| `res.json::<T>()` | El cuerpo deserializado desde JSON (entra en pánico, mostrando el cuerpo, si no es válido) |

```rust
#[derive(serde::Deserialize, Debug, PartialEq)]
struct User {
    id: u64,
    name: String,
}

let user: User = res.json();                        // tipado
let body: vitesse::serde_json::Value = res.json();  // sin tipar
assert_eq!(body["name"], "Ada");
```

> [!NOTE]
> Aquí no interviene ningún socket, así que las cabeceras que el motor HTTP añade al escribir en la red (`content-length`, `date`, `connection`, `transfer-encoding`) no aparecen en una `TestResponse`. Para comprobarlas, prueba contra un servidor real (más abajo).

## Un ejemplo completo

La aplicación `hello` del principio, ampliada con una pequeña API de usuarios con estado compartido, y sus pruebas:

```rust
// src/lib.rs
use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use vitesse::prelude::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
}

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

/// Nuestra «base de datos»: un mapa en memoria compartido por todas las peticiones.
#[derive(Default)]
struct Users(Mutex<HashMap<u64, User>>);

pub fn app() -> App {
    let mut app = App::new();
    app.state(Users::default());

    app.get("/hello/:name", |req: Request| async move {
        format!("¡Hola, {}!", req.param("name").unwrap_or("desconocido"))
    });

    app.post("/users", |req: Request| async move {
        let new: NewUser = req.json().await?;
        if new.name.trim().is_empty() {
            return Err(Error::unprocessable("el nombre es obligatorio"));
        }
        let mut users = req.state::<Users>().0.lock().unwrap();
        let user = User { id: users.len() as u64 + 1, name: new.name };
        users.insert(user.id, user.clone());
        Ok((201, Json(user)))
    });

    app.get("/users/:id", |req: Request| async move {
        let id: u64 = req.param_as("id")?;
        let users = req.state::<Users>().0.lock().unwrap();
        users
            .get(&id)
            .cloned()
            .map(Json)
            .ok_or_else(|| Error::not_found("usuario no encontrado"))
    });

    app
}
```

```rust
// tests/api.rs
use my_api::{User, app};
use vitesse::json;
use vitesse::serde_json::Value;
use vitesse::test::TestClient;

#[tokio::test]
async fn crea_y_luego_lee_un_usuario() {
    let client = TestClient::new(app());

    let res = client.post("/users").json(&json!({ "name": "Ada" })).await;
    assert_eq!(res.status(), 201);
    let created: User = res.json();
    assert_eq!(created.name, "Ada");

    let res = client.get(&format!("/users/{}", created.id)).await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.json::<User>(), created);
}

#[tokio::test]
async fn rechaza_entradas_incorrectas() {
    let client = TestClient::new(app());

    // Ni siquiera es JSON: 400, generado por `req.json()`.
    let res = client.post("/users").body("{uy").await;
    assert_eq!(res.status(), 400);

    // JSON válido, pero falla nuestra propia validación: 422 con nuestro mensaje.
    let res = client.post("/users").json(&json!({ "name": "" })).await;
    assert_eq!(res.status(), 422);
    let body: Value = res.json();
    assert_eq!(body["error"], "el nombre es obligatorio");

    // No es un número: 400. Id desconocido: 404.
    assert_eq!(client.get("/users/abc").await.status(), 400);
    assert_eq!(client.get("/users/999").await.status(), 404);
}
```

Cada llamada a `app()` crea un estado nuevo: cada prueba empieza con una «base de datos» vacía y las pruebas nunca se pisan entre sí, aunque se ejecuten en paralelo.

## Probar los errores

Los errores son respuestas como cualquier otra: comprueba el estado y el cuerpo `{"error": "..."}`. Los pánicos y los errores internos también merecen una prueba, para asegurarte de que no se filtra nada al cliente:

```rust
use vitesse::prelude::*;
use vitesse::test::TestClient;

async fn boom(_req: Request) -> &'static str {
    panic!("algo salió mal")
}

async fn parse(_req: Request) -> vitesse::Result<String> {
    let n: u32 = "no es un número".parse()?; // un error estándar: se convierte en un 500
    Ok(n.to_string())
}

#[tokio::test]
async fn los_errores_se_convierten_en_respuestas() {
    let mut app = App::new();
    app.get("/boom", boom);
    app.get("/parse", parse);
    let client = TestClient::new(app);

    // Un pánico no tumba nada: se convierte en un 500.
    assert_eq!(client.get("/boom").await.status(), 500);

    // Los detalles internos nunca se envían al cliente.
    let res = client.get("/parse").await;
    assert_eq!(res.status(), 500);
    assert_eq!(res.text(), r#"{"error":"Internal Server Error"}"#);

    // El 404 por defecto.
    assert_eq!(client.get("/nope").await.text(), r#"{"error":"Cannot GET /nope"}"#);
}
```

Un `app.on_error` personalizado se comprueba de la misma forma:

```rust
#[tokio::test]
async fn paginas_de_error_personalizadas() {
    let mut app = App::new();
    app.on_error(|err: Error| {
        res::status(err.status()).html(format!("<h1>{}</h1>", err.message()))
    });
    let client = TestClient::new(app);

    let res = client.get("/nope").await;
    assert_eq!(res.status(), 404);
    assert_eq!(res.header("content-type"), Some("text/html; charset=utf-8"));
    assert_eq!(res.text(), "<h1>Cannot GET /nope</h1>");
}
```

> [!TIP]
> Comprueba los estados y tus propios mensajes. El texto de los mensajes que genera el propio Vitesse (JSON no válido, parámetro no válido…) puede cambiar de una versión a otra.

El *panic hook* de Rust sigue mostrando el mensaje del pánico en la salida de las pruebas: es normal. Todo sobre `Error` y `on_error` en [Errores](errors.md).

## Probar los middlewares

Mete el middleware en una mini aplicación con una ruta de prueba y comprueba los dos caminos: la petición que pasa y la que se detiene.

```rust
use vitesse::prelude::*;
use vitesse::test::TestClient;

async fn require_key(req: Request, next: Next) -> Response {
    match req.header("x-api-key") {
        Some("secret") => next.run(req).await,
        _ => Error::unauthorized("falta la clave de API").into_response(),
    }
}

#[tokio::test]
async fn require_key_bloquea_o_deja_pasar() {
    let mut app = App::new();
    app.middleware(require_key);
    app.get("/", |_| async { "ok" });
    let client = TestClient::new(app);

    let res = client.get("/").await;
    assert_eq!(res.status(), 401);
    assert_eq!(res.text(), r#"{"error":"falta la clave de API"}"#);

    let res = client.get("/").header("x-api-key", "secret").await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "ok");
}
```

Para comprobar lo que un middleware adjunta a la petición (con `req.set`), haz que el handler de prueba lo devuelva:

```rust
#[derive(Clone)]
struct CurrentUser(String);

async fn fake_auth(mut req: Request, next: Next) -> Response {
    req.set(CurrentUser("ada".into()));
    next.run(req).await
}

async fn me(req: Request) -> String {
    req.get::<CurrentUser>().map(|u| u.0.clone()).unwrap_or_default()
}

#[tokio::test]
async fn el_handler_ve_lo_que_adjunto_el_middleware() {
    let mut app = App::new();
    app.get("/me", me.with(fake_auth));
    let client = TestClient::new(app);

    assert_eq!(client.get("/me").await.text(), "ada");
}
```

Los middlewares incluidos se prueban igual; por ejemplo, `middleware::cors()` enviando una cabecera `origin` y comprobando `access-control-allow-origin`. Para escribir los tuyos, consulta [Middlewares](middleware.md).

## Probar detrás de un proxy

Todas las peticiones de `TestClient` vienen de `127.0.0.1`: `req.ip()` devuelve `127.0.0.1`, y `req.secure()` es `false`. Para probar lo que hace tu aplicación detrás de un proxy inverso, configura [`app.trust_proxy`](requests.md#detrás-de-un-proxy-trust_proxy) como en producción, y envía las cabeceras que añadiría tu proxy. Como vienen de `127.0.0.1`, se aceptan con `"loopback"`:

```rust
use vitesse::prelude::*;
use vitesse::test::TestClient;

#[tokio::test]
async fn lee_al_visitante_detras_del_proxy() {
    let mut app = App::new();
    app.trust_proxy("loopback");
    app.get("/whoami", |req: Request| async move {
        format!("{} {}", req.ip().unwrap(), req.protocol())
    });
    let client = TestClient::new(app);

    let res = client
        .get("/whoami")
        .header("x-forwarded-for", "203.0.113.9")
        .header("x-forwarded-proto", "https")
        .await;
    assert_eq!(res.text(), "203.0.113.9 https");

    // Sin las cabeceras, el visitante es el propio cliente de prueba.
    assert_eq!(client.get("/whoami").await.text(), "127.0.0.1 http");
}
```

El mismo truco sirve para probar [`middleware::rate_limit`](middleware.md#rate_limit) con varios clientes: sin `trust_proxy`, todas las peticiones de prueba comparten el contador de `127.0.0.1`; con él, cada valor de `x-forwarded-for` cuenta como un visitante distinto.

## Probar las sesiones

`TestClient` no guarda las cookies de una petición a la siguiente: la prueba hace de navegador. Lee la cabecera `Set-Cookie` de la respuesta que abrió la [sesión](sessions.md), y devuelve su parte `nombre=valor` en una cabecera `cookie`:

```rust
use vitesse::test::{TestClient, TestResponse};

/// La parte `nombre=valor` de la cabecera `Set-Cookie` de la respuesta.
fn cookie_of(res: &TestResponse) -> String {
    let set_cookie = res.header("set-cookie").expect("no hay cabecera Set-Cookie");
    set_cookie.split(';').next().unwrap().to_owned()
}

#[tokio::test]
async fn inicia_y_cierra_sesion() {
    let client = TestClient::new(my_api::app());

    let res = client
        .post("/login")
        .form(&[("email", "ada@example.com"), ("password", "secret")])
        .await;
    let cookie = cookie_of(&res); // "vitesse.sid=…"

    let res = client.get("/account").header("cookie", &cookie).await;
    assert_eq!(res.status(), 200);

    let res = client.post("/logout").header("cookie", &cookie).await;
    assert!(res.header("set-cookie").unwrap().contains("Max-Age=0")); // borrada en el navegador
    assert_eq!(client.get("/account").header("cookie", &cookie).await.status(), 401);
}
```

- `res.header("set-cookie")` devuelve la primera cabecera `Set-Cookie`. Cuando una respuesta crea varias cookies, recórrelas con `res.headers().get_all("set-cookie")`.
- Las [cookies firmadas](sessions.md#cookies-firmadas) se prueban de la misma forma. Cambia el valor para comprobar que se rechaza una cookie falsificada: `cookie.replacen("dark", "light", 1)` tiene que comportarse como si no hubiera cookie.
- Cada `TestClient::new(my_api::app())` empieza con un `MemoryStore` vacío, así que las pruebas no comparten sesiones.

## Probar las plantillas

Con `app.views`, una plantilla se compila la primera vez que se renderiza, así que un error de sintaxis en una página solo aparece cuando alguien la abre. Una prueba que renderiza cada página lo detecta antes de un despliegue:

```rust
use vitesse::test::TestClient;

#[tokio::test]
async fn todas_las_paginas_se_renderizan() {
    let client = TestClient::new(my_api::app());
    for path in ["/", "/users", "/login"] {
        let res = client.get(path).await;
        assert_eq!(res.status(), 200, "{path}");
        assert_eq!(res.header("content-type"), Some("text/html; charset=utf-8"));
    }
    assert!(client.get("/users").await.text().contains("<li>Ada</li>"));
}
```

- `cargo test` ejecuta las pruebas desde la raíz del paquete, así que `app.views("templates")` encuentra la misma carpeta que `cargo run`.
- Un renderizado fallido es un `500`, y su causa, con el nombre y la línea de la plantilla, se escribe en la salida de las pruebas (`[vitesse] error 500: …`).
- Para probar una plantilla por separado, dale a una pequeña aplicación de prueba un entorno con esa plantilla, `env.add_template(nombre, fuente)`, a través de [`app.templates(env)`](templates.md#filtros-funciones-y-plantillas-integradas-apptemplates).

## Probar Server-Sent Events

`TestClient` lee el cuerpo entero antes de devolver la respuesta, así que un [flujo de eventos](responses.md#server-sent-events) es fácil de probar siempre que termine: `res.text()` contiene entonces todos los eventos, exactamente como los recibiría el navegador.

```rust
use vitesse::prelude::*;
use vitesse::sse::Event;
use vitesse::test::TestClient;

#[tokio::test]
async fn envia_una_cuenta_atras() {
    let mut app = App::new();
    app.get("/countdown", |_| async {
        let (tx, stream) = sse::channel(8);
        tokio::spawn(async move {
            for n in (1..=3).rev() {
                let _ = tx.send(Event::new().event("count").data(n.to_string())).await;
            }
        }); // la tarea termina y descarta `tx`: la respuesta también termina
        stream
    });
    let client = TestClient::new(app);

    let res = client.get("/countdown").await;
    assert_eq!(res.header("content-type"), Some("text/event-stream"));
    assert_eq!(
        res.text(),
        "event: count\ndata: 3\n\nevent: count\ndata: 2\n\nevent: count\ndata: 1\n\n"
    );
}
```

Un flujo que no termina nunca (un reloj, un canal de notificaciones) haría que la prueba esperase para siempre. Hazlo finito en las pruebas (con un parámetro de la query string, por ejemplo), comprueba los eventos por separado (`Event::new().id("1").data("hi").to_string()` es `"id: 1\ndata: hi\n\n"`), o lee los primeros eventos [en un puerto real](#pruebas-de-integración-en-un-puerto-real), con un tiempo límite.

## Probar la subida de archivos

`TestClient` no tiene ninguna ayuda para los [formularios multipart](requests.md#subida-de-archivos-multipart): escribe el cuerpo a mano. Cada parte empieza con `--` y el delimitador (*boundary*), seguido de sus cabeceras, una línea vacía y su contenido; el último delimitador termina con `--`. Cada línea termina con `\r\n`:

```rust
// tests/upload.rs
use vitesse::json;
use vitesse::test::TestClient;

#[tokio::test]
async fn sube_un_archivo() {
    let client = TestClient::new(my_api::app());
    let body = "--XyZ\r\n\
        content-disposition: form-data; name=\"title\"\r\n\
        \r\n\
        Vacaciones\r\n\
        --XyZ\r\n\
        content-disposition: form-data; name=\"photo\"; filename=\"playa.txt\"\r\n\
        content-type: text/plain\r\n\
        \r\n\
        arena y mar\r\n\
        --XyZ--\r\n";

    let res = client
        .post("/upload")
        .header("content-type", "multipart/form-data; boundary=XyZ")
        .body(body)
        .await;
    assert_eq!(res.status(), 200);

    // Un cuerpo que no es multipart: 415.
    assert_eq!(client.post("/upload").json(&json!({})).await.status(), 415);
}
```

Una `\` al final de una línea continúa la cadena de Rust en la línea siguiente, sin el salto de línea ni la sangría. Para archivos binarios o muchas partes, escribe una pequeña función que monte las partes en un `Vec<u8>` y pásalo a `.body(...)`. Comprueba también que un cuerpo por encima del límite de la ruta recibe un `413`.

## Pruebas de integración en un puerto real

`TestClient` se salta el motor HTTP/1.1. Para lo que solo ocurre en una conexión real (`content-length`, keep-alive, pipelining, cuerpos `chunked`, `Expect: 100-continue`, límites de tamaño de las cabeceras, conversaciones [WebSocket](#probar-websockets)), o para usar un cliente HTTP de verdad, arranca el servidor en un puerto real:

- `app.bind("127.0.0.1:0")` abre el socket; el puerto `0` deja que el sistema operativo elija uno libre, así las pruebas en paralelo nunca chocan;
- `server.local_addr()` te da la dirección que se está usando de verdad;
- `tokio::spawn(server.run())` atiende peticiones en segundo plano mientras dura la prueba.

Con [reqwest](https://docs.rs/reqwest) como cliente:

```toml
[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt"] }
reqwest = { version = "0.12", default-features = false } # aquí basta con HTTP sin TLS
```

```rust
// tests/server.rs
#[tokio::test]
async fn sirve_por_tcp() {
    let server = my_api::app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr(); // p. ej. 127.0.0.1:49213
    tokio::spawn(server.run());

    let res = reqwest::get(format!("http://{addr}/hello/Ada")).await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["content-length"], "12"); // «¡» ocupa 2 bytes
    assert_eq!(res.text().await.unwrap(), "¡Hola, Ada!");
}
```

Para probar también el apagado, usa `with_graceful_shutdown` con un canal:

```rust
#[tokio::test]
async fn se_detiene_limpiamente() {
    let server = my_api::app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(server.with_graceful_shutdown(async {
        stopped.await.ok();
    }));

    let res = reqwest::get(format!("http://{addr}/hello/Ada")).await.unwrap();
    assert_eq!(res.status(), 200);

    stop.send(()).unwrap();
    handle.await.unwrap().unwrap(); // el servidor devolvió Ok(())
}
```

Para el comportamiento de bajo nivel, escribe HTTP en bruto sobre un `TcpStream`, sin ninguna dependencia adicional:

```rust
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn responde_en_orden_a_peticiones_en_pipeline() {
    let server = my_api::app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(
            b"GET /hello/A HTTP/1.1\r\nHost: test\r\n\r\n\
              GET /hello/B HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();

    let a = response.find("¡Hola, A!").unwrap();
    let b = response.find("¡Hola, B!").unwrap();
    assert!(a < b);
}
```

Consulta [Configuración del servidor](server.md) para `bind`, `Server` y el apagado ordenado.

## Probar WebSockets

`TestClient` no tiene un socket real, así que no puede mantener una conversación [WebSocket](websocket.md): en una ruta WebSocket solo ve la respuesta al handshake. Eso basta para probar los rechazos:

```rust
use vitesse::test::TestClient;

#[tokio::test]
async fn websocket_route_refuses_plain_http() {
    let client = TestClient::new(my_api::app());
    // Un GET normal en una ruta WebSocket: 426 Upgrade Required.
    assert_eq!(client.get("/echo").await.status(), 426);
}
```

Para intercambiar mensajes, arranca el servidor en un puerto real como arriba y conéctate con el cliente de [tokio-tungstenite](https://docs.rs/tokio-tungstenite), la biblioteca en la que se apoya el soporte de WebSocket de Vitesse:

```toml
[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt"] }
tokio-tungstenite = { version = "0.30", features = ["connect"] }
futures-util = "0.3"
```

```rust
// tests/ws.rs
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

#[tokio::test]
async fn echo_over_a_real_websocket() {
    let server = my_api::app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());

    // Un handshake con cabeceras adicionales (cookie, origen…), para las rutas que las comprueban.
    let mut request = format!("ws://{addr}/echo").into_client_request().unwrap();
    request.headers_mut().insert("cookie", "session=abc".parse().unwrap());
    let (mut socket, response) = connect_async(request).await.unwrap();
    assert_eq!(response.status(), 101);

    socket.send(Message::text("hola")).await.unwrap();
    assert_eq!(socket.next().await.unwrap().unwrap(), Message::text("eco: hola"));

    // El cliente cierra; el servidor responde al cierre y después el flujo termina.
    socket.close(None).await.unwrap();
    while let Some(Ok(_)) = socket.next().await {}
}
```

`connect_async` falla si el servidor rechaza el handshake: para comprobar el estado de un rechazo, usa `TestClient` como arriba.

## Probar como servicio de tower

Con la feature `tower` de Vitesse, `app.into_service()` convierte la aplicación en un `Service` de tower, la abstracción sobre la que están construidos axum y hyper. Así puedes probarla como lo hacen los usuarios de axum, con `tower::ServiceExt::oneshot`:

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tower"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt"] }
tower = { version = "0.5", features = ["util"] }
```

```rust
use tower::ServiceExt; // para oneshot
use vitesse::prelude::*;

#[tokio::test]
async fn funciona_como_servicio_de_tower() {
    let service = my_api::app().into_service();

    let req = vitesse::http::Request::get("/hello/Ada").body(Body::empty()).unwrap();
    let res = service.oneshot(req).await.unwrap(); // el tipo de error es Infallible
    assert_eq!(res.status(), 200);
    let body = res.into_body().to_bytes().await.unwrap();
    assert_eq!(body, "¡Hola, Ada!");
}
```

- Las peticiones son `http::Request` con cualquier tipo de cuerpo (`vitesse::Body`, el `Full` de http-body-util…), y las respuestas son `http::Response<vitesse::Body>`.
- El servicio es `Copy`: `oneshot` lo toma por valor, pero `service` sigue sirviendo para las peticiones siguientes.
- A diferencia de `TestClient`, no hay dirección de cliente salvo que pongas un `SocketAddr` en las extensiones de la petición (si no, `req.ip()` es `None`), y las rutas WebSocket responden `501`.

Para las pruebas del día a día, `TestClient` es más corto. El servicio destaca cuando la aplicación se ejecuta dentro de una pila de tower: servida por hyper, o anidada en un router de axum con `Router::nest_service`.

## Organizar tus pruebas

- **La carpeta `tests/`**: un archivo por área (`tests/users.rs`, `tests/auth.rs`…), todos a través de `TestClient`. Cada archivo se compila como un crate independiente; comparte las utilidades en `tests/common/mod.rs`:

  ```rust
  // tests/common/mod.rs
  use vitesse::test::TestClient;

  pub fn client() -> TestClient {
      TestClient::new(my_api::app())
  }
  ```

  ```rust
  // tests/users.rs
  mod common;

  #[tokio::test]
  async fn usuario_desconocido_404() {
      let client = common::client();
      assert_eq!(client.get("/users/42").await.status(), 404);
  }
  ```

- **Las pruebas unitarias** en un bloque `#[cfg(test)] mod tests` junto a tu código, para las funciones privadas. `Request` no tiene constructor público: prueba los handlers con `TestClient` y deja la lógica de negocio en funciones normales que puedas llamar directamente.
- **Estado nuevo**: construye una aplicación nueva en cada prueba (`TestClient::new(app())`). La aplicación congelada vive hasta que termina el programa de pruebas, algo despreciable. `TestClient` es `Copy`: pásalo sin problema a tus funciones auxiliares.
- **Comandos útiles**: `cargo test` lo ejecuta todo, `cargo test users` solo las pruebas cuyo nombre contiene `users`, y `cargo test -- --nocapture` muestra la salida de `println!` y las líneas de `middleware::logger()`.
