# Configuración del servidor

Esta página cubre todo lo relacionado con el propio servidor: las tres formas de arrancarlo, las direcciones de escucha y los sockets Unix, los hilos, el tamaño de los cuerpos, los tiempos de espera y los límites de conexiones, los límites integrados en el motor HTTP, el apagado ordenado y los protocolos que habla. Los valores por defecto están pensados para producción, así que la mayoría de las aplicaciones solo necesitan `app.run(port)`.

## Tres formas de arrancar el servidor

| | `app.run(addr)` | `app.listen(addr).await` | `app.bind(addr).await?` + `Server` |
|---|---|---|---|
| Necesita `#[tokio::main]` | No | Sí | Sí |
| Hilos | Uno por núcleo, ver `workers` y `thread_per_core` | Los de tu runtime | Los de tu runtime |
| Se detiene limpiamente con `Ctrl+C` / `SIGTERM` | Sí | Sí | Sí con `run()`, o con tu propia señal con `with_graceful_shutdown` |
| Conoce el puerto antes de servir | No | No | Sí, con `local_addr()` |
| Uso típico | La mayoría de las aplicaciones, producción | Una aplicación tokio existente | Pruebas, apagado personalizado, puerto `0` |

### `app.run`: la opción por defecto

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "Hello World!" });

    app.run(3000) // se bloquea hasta Ctrl+C / SIGTERM
}
```

`run` crea su propio runtime de tokio, arranca un hilo por núcleo y se bloquea hasta que el servidor se detiene. Devuelve un `io::Result<()>`: si el puerto ya está ocupado, recibes el error de inmediato. Además es el modo más rápido (consulta [Rendimiento](performance.md)).

### `app.listen`: dentro de tu propio runtime

Si ya tienes un runtime de tokio, por ejemplo para hacer una inicialización asíncrona antes de arrancar, usa `listen`:

```rust
use vitesse::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Inicialización asíncrona antes de arrancar (base de datos, configuración...).
    let greeting = tokio::fs::read_to_string("greeting.txt")
        .await
        .unwrap_or_else(|_| "Hello World!".to_string());

    let mut app = App::new();
    app.state(greeting);
    app.get("/", |req: Request| async move { req.state::<String>().clone() });

    app.listen(3000).await // hasta Ctrl+C / SIGTERM, en el runtime actual
}
```

Para esto necesitas tokio en tu `Cargo.toml` (`tokio = { version = "1", features = ["full"] }`).

> [!NOTE]
> Igual que `run`, `listen` se detiene limpiamente con `Ctrl+C` o `SIGTERM` y deja terminar las peticiones en curso (consulta [Apagado ordenado](#apagado-ordenado)). En cambio, `app.workers` y `app.thread_per_core` no tienen efecto: los hilos son los de tu runtime.

### `app.bind` y `Server`: control total

`bind` abre el puerto sin empezar a servir todavía y devuelve un `Server`:

```rust
use vitesse::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "ok" });

    let server = app.bind("127.0.0.1:0").await?; // puerto 0: el sistema elige uno libre
    println!("Escuchando en http://{}", server.local_addr());

    server.run().await // hasta Ctrl+C / SIGTERM, como app.run
}
```

| Método de `Server` | Función |
|---|---|
| `server.local_addr()` | La dirección que se usa de verdad (útil con el puerto `0`) |
| `server.http3_addr()` | La dirección UDP de [HTTP/3](http3.md), si `app.http3` está configurado (feature `http3`) |
| `server.run().await` | Sirve hasta `Ctrl+C` / `SIGTERM` y luego se detiene limpiamente |
| `server.with_graceful_shutdown(signal).await` | Sirve hasta que termina el futuro `signal` y luego se detiene limpiamente |

`with_graceful_shutdown` sustituye `Ctrl+C` / `SIGTERM` por el futuro que elijas: un canal en las [pruebas](testing.md), o las señales de siempre seguidas de tu propio código, como en [Puesta en producción](production.md#apagado-ordenado).

Es el modo que se usa en las [pruebas de integración](testing.md) sobre un puerto real.

## Direcciones de escucha

Los tres métodos aceptan cualquier valor que implemente el trait `ListenAddr`:

| Valor | Escucha en |
|---|---|
| `3000` (cualquier entero) | `0.0.0.0:3000`: todas las interfaces IPv4 |
| `"3000"` | Lo mismo |
| `"127.0.0.1:8080"` | Solo esta máquina |
| `"[::]:3000"` | Todas las interfaces IPv6 |
| `"localhost:3000"` | Se resuelve el nombre y se usa la primera dirección que funcione |
| `String`, `&String` | Igual que el `&str` equivalente |
| `SocketAddr` | Exactamente esa dirección |
| `([127, 0, 0, 1], 8080)`, `(Ipv4Addr::LOCALHOST, 8080)` | Un par `(IP, puerto)` |

Un puerto a secas escucha en todas las interfaces, como `app.listen(3000)` en Express: es lo que necesitas en un contenedor o en un servidor. Usa `127.0.0.1` para aceptar solo conexiones de la propia máquina. Un puerto fuera de `0..=65535` devuelve un error al arrancar.

> [!TIP]
> Mejor una IP explícita que `localhost`: según el sistema, `localhost` puede resolverse primero como `::1` (IPv6) y entonces el servidor no respondería en `127.0.0.1`.

### Leer el puerto del entorno

Los servicios de hosting (Heroku y otros) te dan el puerto en la variable `PORT`. Como un `String` que contiene solo un puerto es una dirección válida, basta con esto:

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "¡Hola!" });

    // PORT=8080 → 0.0.0.0:8080; sin PORT → 0.0.0.0:3000
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    app.run(port)
}
```

Consulta [Heroku](heroku-mobile.md) y [Docker](docker.md) para despliegues completos.

## Sockets de dominio Unix

Detrás de un proxy inverso en la misma máquina, Vitesse puede escuchar en un socket de dominio Unix, un archivo como `/run/mi-app/app.sock`, en lugar de en un puerto TCP. Sin puerto que elegir ni que cerrar en el cortafuegos: los permisos del archivo deciden quién puede conectarse. Tres métodos se corresponden con los de TCP:

| Método | Como | Notas |
|---|---|---|
| `app.run_unix(path)` | `app.run` | Crea su propio runtime de tokio: el multihilo, con `workers` hilos (sin modo de un hilo por núcleo) |
| `app.listen_unix(path).await` | `app.listen` | En tu runtime |
| `app.bind_unix(path).await?` | `app.bind` | Devuelve un `UnixServer`: `path()`, `run().await`, `with_graceful_shutdown(signal).await` |

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.trust_proxy(1); // un proxy delante (Nginx): él indica la dirección del cliente
    app.get("/", |req: Request| async move { format!("¡Hola {:?}!", req.ip()) });

    app.run_unix("/run/mi-app/app.sock") // hasta Ctrl+C / SIGTERM
}
```

- **El archivo del socket**: si una ejecución anterior dejó el archivo, se reemplaza; si un servidor todavía responde en él, recibes en cambio un error `AddrInUse`, y un archivo normal en esa ruta nunca se borra. El archivo se elimina cuando el servidor se detiene. La carpeta debe existir y la aplicación debe poder escribir en ella.
- **Permisos**: el archivo se crea con la umask del proceso. Para que el usuario del proxy pueda conectarse, cámbialos con `bind_unix`, antes de servir: `std::fs::set_permissions(server.path(), Permissions::from_mode(0o660))` (consulta [Puesta en producción](production.md#a-través-de-un-socket-unix) para una configuración completa).
- **Sin dirección del cliente**: un socket Unix no tiene IP, así que `req.ip()` y `req.remote_addr()` son `None`. Con `app.trust_proxy(...)` (normalmente un número de saltos: `1` para un solo proxy), el otro extremo cuenta como un proxy de confianza: `req.ip()` lee `X-Forwarded-For`, y `req.protocol()` y `req.hostname()` leen `X-Forwarded-Proto` y `X-Forwarded-Host`.
- **HTTP/3** no se abre con un socket Unix (QUIC necesita un puerto UDP): usa `app.run` para ello.
- **Solo Unix**: estos métodos no existen en Windows. No necesitan ninguna feature de Cargo.

En el lado del proxy, Nginx se conecta con `proxy_pass http://unix:/run/mi-app/app.sock;` y Caddy con `reverse_proxy unix//run/mi-app/app.sock`.

## Hilos: `workers` y `thread_per_core`

Por defecto, `app.run` arranca un hilo por cada CPU disponible para el proceso. Cómo se reparten el trabajo depende del modo:

- **Un hilo por núcleo** (el modo por defecto en Linux): cada hilo tiene su propio bucle de eventos y su propio socket de escucha (`SO_REUSEPORT`). El kernel reparte las conexiones nuevas entre los hilos y una conexión nunca cambia de hilo, sin ninguna sincronización entre núcleos. Es el modo más rápido.
- **Runtime multihilo** (`thread_per_core(false)`, y siempre en sistemas distintos de Linux, como macOS y Windows): un único socket y el runtime de tokio con robo de tareas (*work stealing*), con `workers` hilos. Los hilos libres se encargan del trabajo pendiente de los ocupados.

```rust
let mut app = App::new();
app.workers(4)              // 4 hilos en lugar de uno por núcleo
    .thread_per_core(false) // runtime multihilo de tokio, con robo de tareas
    .body_limit(10 * 1024 * 1024); // acepta cuerpos de hasta 10 MiB
```

Cuándo cambiar estos ajustes:

- **`workers(n)`** cuando el servidor comparte la máquina con otros servicios (una base de datos, un proceso de tareas en segundo plano), cuando quieres dejar núcleos libres o cuando el número de CPU detectado en un contenedor no coincide con la cuota que realmente recibe. `n` vale como mínimo `1`.
- **`thread_per_core(false)`** cuando algunos handlers hacen cálculos bloqueantes largos (en el modo de un hilo por núcleo, frenan todas las conexiones de su hilo) o cuando hay pocas conexiones y muy desiguales: por ejemplo, un proxy inverso que mantiene abiertas unas pocas conexiones keep-alive, que podrían caer todas en el mismo hilo.
- En cualquier otro caso, deja los valores por defecto.

Estos dos ajustes solo se aplican a `app.run`. Para cambiarlos sin recompilar, léelos del entorno:

```rust
let mut app = App::new();
if let Some(n) = std::env::var("WORKERS").ok().and_then(|w| w.parse().ok()) {
    app.workers(n);
}
```

> [!TIP]
> Antes de cambiar de modo por culpa de handlers lentos, mueve el trabajo bloqueante a `tokio::task::spawn_blocking`: consulta [Rendimiento](performance.md).

## Tamaño del cuerpo de las peticiones: `body_limit`

```rust
app.body_limit(10 * 1024 * 1024); // 10 MiB
```

El valor por defecto es `vitesse::DEFAULT_BODY_LIMIT`, 1 MiB. El límite se aplica cada vez que un handler lee el cuerpo con `req.bytes()`, `req.text()`, `req.json()` o `req.form()`: si lo supera, la petición falla con `413 Payload Too Large`. Si el `Content-Length` anunciado ya es demasiado grande, se devuelve el 413 sin leer nada.

El cuerpo solo se lee si el handler lo pide, así que las rutas que nunca lo leen no se ven afectadas. `req.take_body()`, que te da el flujo en bruto (proxies), no tiene límite: contar los bytes queda en tus manos. Los [formularios multipart](requests.md#subida-de-archivos-multipart) (`req.multipart()`) también tienen límite, aunque se lean como flujo.

`app.body_limit` fija el valor por defecto de toda la aplicación. Para aceptar cuerpos más grandes solo en algunas rutas (subidas de archivos), o más pequeños, cambia el límite por ruta o por router con el middleware `body_limit`:

```rust
async fn upload(req: Request) -> vitesse::Result<String> {
    let bytes = req.bytes().await?;
    Ok(format!("{} bytes", bytes.len()))
}

let mut app = App::new(); // 1 MiB en todas partes...
app.post("/upload", upload.with(middleware::body_limit(50 * 1024 * 1024))); // ...50 MiB aquí
```

`req.body_limit()` devuelve el límite que se aplica a una petición, y `req.set_body_limit(bytes)` lo cambia desde tu propio middleware. Se aplica igual sobre HTTP/1.1, [HTTP/2](https.md) y [HTTP/3](http3.md). Consulta [Límite de tamaño del cuerpo y 413](requests.md#límite-de-tamaño-del-cuerpo-y-413) para los detalles.

## Tiempos de espera y conexiones

Cuatro ajustes controlan las propias conexiones, como el `HttpServer` de actix-web:

```rust
use std::time::Duration;

let mut app = App::new();
app.keep_alive(Duration::from_secs(75))        // conexiones keep-alive inactivas (60 s por defecto)
    .header_timeout(Duration::from_secs(10))   // tiempo para enviar la cabecera de la petición (30 s por defecto)
    .shutdown_timeout(Duration::from_secs(25)) // periodo de gracia al apagar (10 s por defecto)
    .max_connections(10_000);                  // conexiones abiertas por bucle de aceptación (por defecto: sin límite)
```

| Ajuste | Por defecto | Efecto |
|---|---|---|
| `keep_alive(duration)` | 60 s | Una conexión keep-alive sin peticiones se cierra pasado este tiempo (HTTP/2 incluido). `Duration::ZERO` desactiva el keep-alive: todas las respuestas llevan `connection: close` |
| `header_timeout(duration)` | 30 s | Un cliente que en ese plazo no ha enviado una cabecera de petición completa recibe `408 Request Timeout`, y la conexión se cierra: protección contra los clientes que abren conexiones y no envían nada (slowloris) |
| `shutdown_timeout(duration)` | 10 s | El tiempo que se da a las peticiones en curso durante el [apagado ordenado](#apagado-ordenado), HTTP/3 incluido |
| `max_connections(n)` | Sin límite | Conexiones abiertas por bucle de aceptación; las conexiones de más esperan en la cola del sistema hasta que se cierre una |

Los plazos los comprueba un temporizador por conexión: se respetan con un margen de la mitad del menor de `keep_alive` y `header_timeout`.

Cuándo cambiarlos:

- **`keep_alive`**: detrás de un proxy o de un balanceador de carga, mantenlo por encima del tiempo de inactividad del propio proxy, para que sea siempre el proxy quien cierre primero y nunca reutilice una conexión que Vitesse está cerrando (un balanceador de carga de AWS espera 60 s por defecto: usa 65 s o más).
- **`header_timeout`**: bájalo (10 s) cuando la aplicación está expuesta directamente a internet, sin proxy.
- **`shutdown_timeout`**: súbelo si algunas peticiones (informes, subidas de archivos) tardan más de 10 s, y mantenlo por debajo del tiempo que tu plataforma espera antes de matar el proceso.
- **`max_connections`**: para limitar la memoria y los descriptores de archivo que usan las conexiones. Con `app.run` en modo de un hilo por núcleo hay un bucle de aceptación por hilo, así que el total es `n` × el número de hilos; en los demás modos, `n` es el total. Cuenta conexiones, no peticiones: una conexión HTTP/2 puede llevar muchas peticiones a la vez. Para limitar las peticiones que se están atendiendo, usa [`middleware::concurrency_limit`](middleware.md#concurrency_limit).

## Límites integrados en el motor

El motor HTTP/1.1 protege el servidor de peticiones mal formadas o abusivas. Los límites de tamaño son fijos; los plazos se pueden cambiar (consulta [Tiempos de espera y conexiones](#tiempos-de-espera-y-conexiones)):

| Situación | Comportamiento |
|---|---|
| Cabecera de la petición (línea de petición y cabeceras) de más de 60 KiB | `431 Request Header Fields Too Large`, conexión cerrada |
| Más de 64 cabeceras | `431`, conexión cerrada |
| Petición mal formada | `400 Bad Request`, conexión cerrada |
| `Content-Length` y `Transfer-Encoding` a la vez, o dos `Content-Length` distintos | `400` (protección contra *request smuggling*) |
| `Transfer-Encoding` cuya última codificación no es `chunked` | `501 Not Implemented` |
| Cuerpo mayor que el límite (`body_limit`), al leerlo | `413 Payload Too Large` |
| Conexión keep-alive sin peticiones durante unos 60 s (`keep_alive`) | Conexión cerrada |
| Cabecera de la petición aún incompleta tras unos 30 s (`header_timeout`) | `408 Request Timeout`, conexión cerrada |
| Handshake TLS sin completar en 10 s ([HTTPS](https.md)) | Conexión cerrada |
| `Expect: 100-continue` | Se envía `100 Continue` automáticamente antes de leer el cuerpo |
| Cuerpo de petición `chunked` | Se decodifica de forma transparente |
| Cuerpo de hasta 64 KiB | Se lee antes de llamar al handler; los cuerpos más grandes llegan al handler como flujo, a medida que se reciben |
| Cuerpo grande que el handler no lee | La respuesta sale con `connection: close` y después se descarta el resto del envío (2 s y 8 MiB como máximo) para que el cliente reciba la respuesta |
| Peticiones en pipeline | Se responden en orden; las respuestas de un lote salen en una sola llamada al sistema |
| HTTP/1.0 | Conexión cerrada tras la respuesta, salvo que el cliente pida keep-alive |
| Pánico en un handler o en un middleware | `500`, y el servidor sigue funcionando |

En las respuestas, el motor calcula `content-length`, añade la cabecera `date`, envía los cuerpos de tamaño desconocido (flujos) en `chunked`, no envía cuerpo en las respuestas `204`, `304` ni en las peticiones `HEAD`, y cierra la conexión tras la respuesta si tu handler pone una cabecera `connection: close`. HTTP/2 tiene sus propios límites: consulta [HTTPS y HTTP/2](https.md#límites-y-qué-no-se-admite).

> [!IMPORTANT]
> No hay límite de tiempo para el propio handler: un handler que espera para siempre mantiene su petición abierta para siempre. Añade el middleware `timeout`, que responde `503 Service Unavailable` pasado el tiempo indicado.

```rust
app.middleware(middleware::timeout(Duration::from_secs(30))); // std::time::Duration
```

Para proteger el servidor de los picos de carga y de los clientes abusivos, consulta también [`middleware::concurrency_limit`](middleware.md#concurrency_limit) y [`middleware::rate_limit`](middleware.md#rate_limit).

## Apagado ordenado

Con `app.run`, `app.listen` y `Server::run` (y sus versiones para sockets Unix), `Ctrl+C` (SIGINT) y, en Unix, `SIGTERM` inician un apagado ordenado:

1. el servidor deja de aceptar conexiones nuevas;
2. las peticiones en curso tienen hasta **10 segundos** (`shutdown_timeout`) para terminar, y sus respuestas llevan `connection: close`; las conexiones HTTP/2 reciben un `GOAWAY`;
3. las conexiones restantes (keep-alive inactivas, peticiones que siguen en curso al acabar el periodo de gracia) se cierran y `run` (o `listen`) devuelve `Ok(())`.

Con [HTTP/3](http3.md), el lado UDP se detiene a la vez: las conexiones abiertas reciben un `GOAWAY`, y sus peticiones en curso disponen del mismo periodo de gracia. Las conexiones [WebSocket](websocket.md) no se esperan: se cierran como muy tarde al final del periodo de gracia, así que los clientes deben reconectarse.

Es justo lo que esperan Docker, Kubernetes o Heroku: envían `SIGTERM` y esperan un rato antes de forzar la parada del proceso. Con `bind`, `with_graceful_shutdown(signal)` sigue los mismos pasos en cuanto termina tu futuro `signal`, así que tú decides qué provoca el apagado. `Ctrl+C` y `SIGTERM` dejan entonces de vigilarse: inclúyelos en tu futuro si todavía los necesitas.

El periodo de gracia es de 10 segundos por defecto. Cámbialo con `shutdown_timeout`, dejándolo un poco por debajo del tiempo que tu plataforma espera antes de matar el proceso (30 segundos en Kubernetes y Heroku, 10 segundos en Docker):

```rust
app.shutdown_timeout(Duration::from_secs(25)); // std::time::Duration
```

## Protocolos y HTTPS

| Protocolo | Feature de Cargo | Cómo |
|---|---|---|
| HTTP/1.1 (y HTTP/1.0) | Integrado | Siempre, sobre TCP o un [socket Unix](#sockets-de-dominio-unix) |
| [WebSocket](websocket.md) | `ws` (activada por defecto) | `app.ws(...)`, sobre HTTP/1.1 (`ws://`, o `wss://` con TLS) y, con `http2`, [sobre HTTP/2](websocket.md#websocket-sobre-http2) |
| [HTTPS](https.md) (TLS 1.2 y 1.3) | `tls` | `app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?)`: todas las conexiones TCP hablan TLS |
| [HTTP/2](https.md) sobre TLS | `tls` y `http2` | Se negocia durante el handshake TLS (ALPN): es lo que usan los navegadores |
| [HTTP/2 sin cifrar](https.md#http2-sin-cifrar-h2c) (h2c) | `http2` | Para los clientes que empiezan directamente en HTTP/2 («prior knowledge»), en el mismo puerto que HTTP/1.1 |
| [HTTP/3](http3.md) sobre QUIC | `http3` | `app.http3(...)`, en un puerto UDP junto al TCP; `server.http3_addr()` devuelve su dirección |

Las mismas rutas, middlewares y handlers responden a todos los protocolos, y `req.version()` los distingue. Con `tls` y `http3` juntas, un solo binario sirve HTTPS, HTTP/2 y HTTP/3 a los navegadores, sin proxy delante: consulta [HTTPS y HTTP/2](https.md).

### Detrás de un proxy inverso

También puedes dejar el TLS a un proxy inverso (Nginx, Caddy o el balanceador de carga de tu plataforma) que reenvíe las peticiones a Vitesse en HTTP/1.1, como se suele hacer con Express: [HTTPS y HTTP/2](https.md#tls-directamente-o-un-proxy-inverso) compara las dos opciones.

Detrás de un proxy, `req.ip()` devuelve la dirección del proxy para las peticiones que reenvía, y `req.protocol()` dice `"http"`: la dirección y el protocolo del cliente están en las cabeceras `X-Forwarded-For` y `X-Forwarded-Proto`. Declara el proxy con `app.trust_proxy(...)` y Vitesse las lee por ti:

```rust
app.trust_proxy("loopback"); // un proxy en la misma máquina
```

Todo esto se explica en [Puesta en producción](production.md#obtener-la-ip-del-cliente).
