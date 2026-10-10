# WebSocket

Un WebSocket mantiene una conexión abierta en los dos sentidos, así que el servidor puede enviar mensajes al cliente en cualquier momento: chats, notificaciones, paneles en directo, juegos multijugador. Vitesse lo admite de serie, al estilo de `express-ws`: `app.ws(ruta, handler)` y después un bucle que lee y escribe mensajes.

## Preparación

El soporte de WebSocket es la feature de Cargo `ws`, **activada por defecto**: con `vitesse = "0.1"` no necesitas nada más. Se apoya en [tokio-tungstenite](https://github.com/snapview/tokio-tungstenite), la implementación en Rust validada con la batería de pruebas Autobahn. El handshake (RFC 6455) y la entrega de la conexión TCP tras la respuesta `101 Switching Protocols` los realiza el propio motor HTTP/1.1 de Vitesse.

Si no necesitas WebSocket, desactiva las features por defecto para compilar un poco menos:

```toml
[dependencies]
vitesse = { version = "0.1", default-features = false }
```

## Una primera ruta WebSocket

Un servidor de eco, que devuelve cada mensaje de texto:

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.ws("/echo", |_req, mut socket| async move {
        while let Some(Ok(message)) = socket.recv().await {
            if let ws::Message::Text(text) = message {
                if socket.send(format!("eco: {text}")).await.is_err() {
                    break; // el cliente se ha ido
                }
            }
        }
        // El bucle termina cuando el cliente cierra la conexión.
    });

    app.run(3000)
}
```

El handler recibe la petición y el socket abierto, y la conexión vive mientras el handler se ejecute. `ws` está en el preludio, de ahí `ws::Message`. Para probarlo, usa [websocat](https://github.com/vi/websocat), un cliente WebSocket para la terminal escrito en Rust (`cargo install websocat`):

```sh
websocat ws://localhost:3000/echo
```

Escribe una línea y el servidor responde `eco: …`.

### Qué hace `app.ws`

`app.ws(ruta, handler)` registra una ruta `GET`, con los mismos patrones que cualquier ruta (`/rooms/:room`, `/files/*path`…). Las peticiones que no son un handshake WebSocket válido reciben un error, que pasa por [`app.on_error`](errors.md) como cualquier otro:

| Petición | Respuesta |
|---|---|
| Un handshake WebSocket (`GET` con `Upgrade: websocket`, `Sec-WebSocket-Version: 13` y una clave válida) | `101 Switching Protocols`, y después el handler se hace cargo de la conexión |
| Una petición HTTP normal (un navegador que abre la URL, `curl`) | `426 Upgrade Required`: «this route only accepts WebSocket connections» |
| Otra versión del protocolo (`Sec-WebSocket-Version` distinta de `13`) | `426 Upgrade Required` |
| Una `Sec-WebSocket-Key` no válida | `400 Bad Request` |
| Otro método, `HEAD` incluido | `405 Method Not Allowed` |

`ws` también existe en `Router`, así que una ruta WebSocket puede vivir en un router con sus middlewares y su prefijo (ver [Middlewares y autenticación](#middlewares-y-autenticación)).

## Leer y escribir mensajes

| Método de `WebSocket` | Efecto |
|---|---|
| `socket.recv().await` | El siguiente mensaje: `Some(Ok(message))`, `Some(Err(error))`, o `None` cuando la conexión está cerrada |
| `socket.send(valor).await` | Envía un mensaje: un `String` o un `&str` como texto, un `Vec<u8>`, `Bytes` o un `&[u8]` como binario, o un `ws::Message` |
| `socket.close(código, motivo).await` | Inicia el cierre (`1000`: cierre normal) |
| `socket.protocol()` | El subprotocolo elegido con [`Upgrade::protocols`](#más-control-wsupgrade), si lo hay |
| `socket.split()` | Dos mitades, para enviar desde una tarea mientras otra recibe (ver [más abajo](#enviar-y-recibir-a-la-vez-split)) |

Un mensaje es un `ws::Message`:

| Variante | Contenido |
|---|---|
| `Message::Text(String)` | Un mensaje de texto (UTF-8 válido) |
| `Message::Binary(Bytes)` | Un mensaje binario |
| `Message::Ping(Bytes)`, `Message::Pong(Bytes)` | Mensajes de control: los pings se responden automáticamente |
| `Message::Close(Option<CloseFrame>)` | El otro lado cierra la conexión; `CloseFrame { code: u16, reason: String }` |

Tres atajos te ahorran un `match`: `message.as_text()` (un `Option<&str>`), `message.as_bytes()` (el contenido de un mensaje de texto o binario) y `message.is_close()`.

`recv` devuelve `Some(Err(…))` cuando la conexión se corta, cuando el cliente incumple el protocolo o envía un mensaje demasiado grande; después ya no queda nada que leer, así que `while let Some(Ok(message))` simplemente se detiene en el primer error. `send` falla una vez cerrada la conexión: `error.is_closed()` te lo indica.

### Mensajes JSON

Los mensajes suelen ser JSON. Analízalos con serde y responde con `json!`:

```rust
use serde::Deserialize;
use vitesse::prelude::*;

#[derive(Deserialize)]
struct Move {
    x: i32,
    y: i32,
}

let mut app = App::new();
app.ws("/game", |_req, mut socket| async move {
    while let Some(Ok(message)) = socket.recv().await {
        let Some(text) = message.as_text() else {
            continue; // binario, ping, pong...
        };
        let reply = match vitesse::serde_json::from_str::<Move>(text) {
            Ok(m) => json!({ "ok": true, "x": m.x, "y": m.y }),
            Err(e) => json!({ "ok": false, "error": e.to_string() }),
        };
        if socket.send(reply.to_string()).await.is_err() {
            break;
        }
    }
});
```

## Parámetros, cabeceras y estado

El handler recibe primero la petición y la conserva durante toda la conexión: parámetros de ruta, query string, cabeceras, cookies y [estado compartido](state.md) se leen como en cualquier handler.

```rust
app.ws("/rooms/:room", |req, mut socket| async move {
    let room = req.param("room").unwrap_or("vestíbulo").to_owned();
    let nickname = req.query("nickname").unwrap_or("anónimo".into()).into_owned();

    let _ = socket.send(format!("{nickname} entró en {room}")).await;
    while let Some(Ok(message)) = socket.recv().await {
        if message.is_close() {
            break;
        }
    }
});
```

> [!NOTE]
> Los navegadores no pueden añadir cabeceras como `Authorization` a una conexión WebSocket. Lo que sí hacen es enviar las cookies del sitio con el handshake: autentica con una cookie de sesión, o con un token de corta duración en la query string (`ws://…/live?token=…`).

## Middlewares y autenticación

Los middlewares globales y los de los routers se ejecutan sobre la petición del handshake, como sobre cualquier petición: registro, autenticación, limitación de peticiones… Un middleware que responde `401` rechaza la conexión antes de que se abra, y las cabeceras que añade un middleware aparecen en la respuesta `101`.

```rust
async fn require_session(req: Request, next: Next) -> Response {
    match req.cookie("session") {
        Some(_) => next.run(req).await,
        None => Error::unauthorized("inicia sesión").into_response(),
    }
}

let mut live = Router::new();
live.middleware(require_session);
live.ws("/notifications", |_req, mut socket| async move {
    let _ = socket.send("has iniciado sesión").await;
});

let mut app = App::new();
app.mount("/live", live); // ws://example.com/live/notifications
```

Un middleware ve el handshake, no la conversación: `middleware::logger()` escribe una línea, `GET /live/notifications 101`, en cuanto se acepta la conexión, y `middleware::timeout(…)` solo limita el tiempo de respuesta al handshake, nunca la vida de la conexión.

## Más control: `ws::Upgrade`

Para subprotocolos, un límite de tamaño o una comprobación antes de aceptar la conexión, usa `vitesse::ws::Upgrade` en una ruta `GET` normal:

```rust
use vitesse::prelude::*;
use vitesse::ws::Upgrade;

let mut app = App::new();
app.get("/chat", |req: Request| async move {
    // Rechaza las conexiones abiertas por páginas de otros sitios.
    if req.header("origin") != Some("https://chat.example.com") {
        return Err(Error::forbidden("origen desconocido"));
    }
    let upgrade = Upgrade::new(&req)? // 426, 400 o 405 si no es un handshake
        .protocols(["chat.v2", "chat.v1"])
        .max_message_size(64 * 1024); // 64 KiB en lugar de 16 MiB
    Ok(upgrade.on_upgrade(req, |_req, mut socket| async move {
        let protocol = socket.protocol().unwrap_or("ninguno").to_owned();
        let _ = socket.send(format!("bienvenido, protocolo: {protocol}")).await;
        // ... la conversación, como con app.ws
    }))
});
```

| Uso | Efecto |
|---|---|
| `Upgrade::new(&req)?` | Comprueba que la petición es un handshake WebSocket; si no, `426`, `400` o `405`, como con `app.ws` |
| `.offered_protocols()` | Los subprotocolos que ofrece el cliente (`Sec-WebSocket-Protocol`), como `&[String]` |
| `.protocols([...])` | Elige el primer subprotocolo ofrecido por el cliente que el servidor admite; `socket.protocol()` lo devuelve |
| `.max_message_size(bytes)` | Tamaño máximo de un mensaje recibido, y de una trama (por defecto `ws::DEFAULT_MAX_MESSAGE_SIZE`, 16 MiB); si se supera, se cierra la conexión |
| `.on_upgrade(req, handler)` | Devuelve la respuesta `101 Switching Protocols`; después `handler(req, socket)` se hace cargo de la conexión |

Si ningún subprotocolo coincide, la conexión se acepta sin subprotocolo. Para rechazarla en su lugar, revisa `offered_protocols()` y devuelve un error antes de `on_upgrade`.

> [!IMPORTANT]
> Comprueba la cabecera `Origin` antes de aceptar una conexión si autenticas con cookies. Los navegadores envían las cookies con el handshake, pero la política del mismo origen y CORS no se aplican a WebSocket: sin esta comprobación, una página de cualquier otro sitio podría abrir una conexión en nombre de tus usuarios con sesión iniciada (*cross-site WebSocket hijacking*). Los clientes fuera del navegador (apps móviles, `websocat`, otros servidores) normalmente no envían `Origin`.

## Enviar y recibir a la vez: `split`

`recv` y `send` toman prestado el socket, así que un único bucle no puede esperar al cliente mientras envía sus propios mensajes. `socket.split()` devuelve dos mitades: un `WebSocketSender` (`send`, `close`) y un `WebSocketReceiver` (`recv`), que pueden vivir en dos tareas.

```rust
use std::time::Duration;

app.ws("/clock", |_req, socket| async move {
    let (mut tx, mut rx) = socket.split();

    // Una tarea envía un mensaje cada segundo...
    let ticker = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        for n in 1.. {
            interval.tick().await;
            if tx.send(format!("tic {n}")).await.is_err() {
                break; // el cliente se ha ido
            }
        }
    });

    // ...mientras esta lee hasta que el cliente se va.
    while let Some(Ok(message)) = rx.recv().await {
        if message.is_close() {
            break;
        }
    }
    ticker.abort();
});
```

Este ejemplo usa tokio directamente: añádelo a tus dependencias, o usa la reexportación, `vitesse::tokio::spawn`.

### `Stream` y `Sink`

`WebSocket` implementa `futures::Stream<Item = Result<Message, ws::Error>>` y `Sink<Message>`, y `WebSocketReceiver` implementa `Stream`. Por tanto, funcionan todos los combinadores de [futures-util](https://docs.rs/futures-util) (`StreamExt`, `SinkExt`), por ejemplo para enviar todo un flujo de mensajes:

```rust
use futures_util::{StreamExt, stream};

app.ws("/countdown", |_req, socket| async move {
    let messages = ["3", "2", "1", "¡despegue!"].map(|text| Ok(ws::Message::from(text)));
    // Envía cada mensaje y después cierra la conexión.
    let _ = stream::iter(messages).forward(socket).await;
});
```

## Una sala de chat

El ejemplo [`examples/chat.rs`](https://github.com/maxlestage/Vitesse/blob/master/examples/chat.rs) del repositorio es una sala de chat completa: cada mensaje que envía un cliente se difunde a todos los demás. Esta es su ruta WebSocket:

```rust
use tokio::sync::broadcast;
use vitesse::prelude::*;
use vitesse::ws::Message;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    // Cada cliente conectado escucha este canal.
    let (room, _) = broadcast::channel::<String>(256);
    app.state(room);

    app.ws("/chat/:name", |req, socket| async move {
        let name = req.param("name").unwrap_or("anónimo").to_owned();
        let room = req.state::<broadcast::Sender<String>>();
        let mut inbox = room.subscribe();
        let (mut tx, mut rx) = socket.split();

        let _ = room.send(format!("* {name} ha entrado"));
        // Reenvía los mensajes de la sala a este cliente...
        let forward = tokio::spawn(async move {
            while let Ok(line) = inbox.recv().await {
                if tx.send(line).await.is_err() {
                    break;
                }
            }
        });
        // ...y difunde lo que dice este cliente.
        while let Some(Ok(message)) = rx.recv().await {
            match message {
                Message::Text(text) => {
                    let _ = room.send(format!("{name}: {}", text.trim_end()));
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        forward.abort();
        let _ = room.send(format!("* {name} se ha ido"));
    });

    app.run(3000)
}
```

Cómo funciona:

- un canal [`broadcast`](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html) de tokio hace de sala: se registra como [estado](state.md), y cada mensaje que se envía por él llega a todos sus suscriptores;
- cada conexión se suscribe (`room.subscribe()`) y divide su socket: una tarea reenvía al cliente los mensajes de la sala, mientras el bucle del handler difunde lo que dice el cliente;
- cuando el cliente se va, el bucle termina: el handler detiene la tarea de reenvío y anuncia la salida.

Pruébalo desde un clon del repositorio, con una terminal por participante:

```sh
cargo run --release --example chat

# En otras dos terminales:
websocat ws://localhost:3000/chat/ada
websocat ws://localhost:3000/chat/grace
```

> [!TIP]
> Un canal `broadcast` guarda los últimos 256 mensajes para los suscriptores lentos. Un cliente que se retrasa más recibe un error `Lagged`, que en este ejemplo termina su bucle de reenvío. En una aplicación real, gestiona `RecvError::Lagged` (saltando los mensajes perdidos o desconectando al cliente) para que un cliente lento nunca deje de recibir sin darse cuenta.

## Probar una ruta WebSocket

[`TestClient`](testing.md) no tiene un socket real: en una ruta WebSocket solo ve la respuesta al handshake. Basta para probar los rechazos (`426` para HTTP normal, `401` de tu middleware, `403` para un origen desconocido). Para probar una conversación, arranca el servidor en un puerto real y conéctate con el cliente de tokio-tungstenite:

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

#[tokio::test]
async fn echoes_text_messages() {
    let server = my_api::app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());

    let (mut socket, response) = connect_async(format!("ws://{addr}/echo")).await.unwrap();
    assert_eq!(response.status(), 101);

    socket.send(Message::text("hola")).await.unwrap();
    let reply = socket.next().await.unwrap().unwrap();
    assert_eq!(reply, Message::text("eco: hola"));

    socket.close(None).await.unwrap();
}
```

Más ejemplos en [Pruebas](testing.md#probar-websockets).

## Detrás de un proxy inverso

El proxy debe dejar pasar el `Upgrade` y después mantener la conexión abierta:

- **Nginx** necesita que se lo indiques, en la `location` que sirve tus rutas WebSocket:

  ```nginx
  location /chat/ {
      proxy_pass http://127.0.0.1:3000;
      proxy_http_version 1.1;
      proxy_set_header Upgrade $http_upgrade;
      proxy_set_header Connection "upgrade";
      proxy_set_header Host $host;
      proxy_read_timeout 1h;   # por defecto, Nginx cierra una conexión en silencio a los 60 s
  }
  ```

- **Caddy** gestiona WebSocket automáticamente con `reverse_proxy`: no hay nada que añadir.
- **Heroku**: su router admite WebSocket (ver [Heroku](heroku-mobile.md#otras-sorpresas)).

Los proxies y las plataformas cierran las conexiones que pasan demasiado tiempo en silencio. Para una conexión que puede quedarse inactiva (notificaciones), envía un `Message::Ping` desde el servidor cada 30 segundos más o menos: los clientes, navegadores incluidos, responden automáticamente con un pong. La configuración completa de Nginx está en [Puesta en producción](production.md#websocket-detrás-de-nginx).

## Límites

- **Solo HTTP/1.1.** WebSocket no está disponible sobre [HTTP/3](http3.md): los clientes abren sus conexiones WebSocket sobre una conexión TCP normal, aunque el resto del sitio use HTTP/3.
- **Apagado ordenado.** Con `Ctrl+C` o `SIGTERM`, el servidor no espera a las conexiones WebSocket abiertas: se cortan en cuanto terminan las peticiones HTTP en curso, y como muy tarde al final del periodo de gracia de 10 segundos (ver [Configuración del servidor](server.md#apagado-ordenado)). Se cortan sin cierre ordenado, y los clientes ven un cierre anómalo (código `1006`). Haz que tus clientes se reconecten automáticamente, algo que necesitan de todos modos para los cortes de red.
- **Tamaño de los mensajes.** 16 MiB por mensaje por defecto; redúcelo con `Upgrade::max_message_size` en los endpoints públicos.
- **Hilos.** En el modo de un hilo por núcleo, una conexión se queda en el hilo que la aceptó: como en cualquier handler, no lo bloquees (ver [Rendimiento](performance.md)).

## Viniendo de Express

Con [express-ws](https://github.com/HenningM/express-ws):

```js
const expressWs = require('express-ws');
expressWs(app);

app.ws('/echo', (ws, req) => {
  ws.on('message', (msg) => ws.send(msg));
  ws.on('close', () => console.log('adiós'));
});
```

Con Vitesse:

```rust
app.ws("/echo", |_req, mut socket| async move {
    while let Some(Ok(message)) = socket.recv().await {
        match message {
            ws::Message::Text(_) | ws::Message::Binary(_) => {
                if socket.send(message).await.is_err() {
                    break;
                }
            }
            _ => {}
        }
    }
    println!("adiós");
});
```

| express-ws | Vitesse |
|---|---|
| `app.ws('/echo', (ws, req) => …)` | `app.ws("/echo", \|req, socket\| async move { … })`: la petición va **primero** |
| `ws.on('message', (msg) => …)` | `while let Some(Ok(msg)) = socket.recv().await { … }` |
| `ws.on('close', …)` | El código después del bucle |
| `ws.send(data)` | `socket.send(data).await` |
| `ws.close(1000, 'bye')` | `socket.close(1000, "bye").await` |
| `req.params.room` | `req.param("room")` |
| `router.ws(…)` | `router.ws(…)` |
| `wss.clients.forEach(…)` (difusión) | Un canal `broadcast` en el estado (ver [Una sala de chat](#una-sala-de-chat)) |

En lugar de callbacks, un WebSocket es un bucle: cada conexión se ejecuta en su propia tarea, y `.await` espera el siguiente mensaje sin bloquear nada.
