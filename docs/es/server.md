# Configuración del servidor

Esta página cubre todo lo relacionado con el propio servidor: las tres formas de arrancarlo, las direcciones de escucha, los hilos, el tamaño de los cuerpos, los límites integrados en el motor HTTP y el apagado ordenado. Los valores por defecto están pensados para producción, así que la mayoría de las aplicaciones solo necesitan `app.run(port)`.

## Tres formas de arrancar el servidor

| | `app.run(addr)` | `app.listen(addr).await` | `app.bind(addr).await?` + `Server` |
|---|---|---|---|
| Necesita `#[tokio::main]` | No | Sí | Sí |
| Hilos | Uno por núcleo, ver `workers` y `thread_per_core` | Los de tu runtime | Los de tu runtime |
| Se detiene limpiamente con `Ctrl+C` / `SIGTERM` | Sí | No | Sí, con `with_graceful_shutdown` |
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

    app.listen(3000).await // se ejecuta para siempre, en el runtime actual
}
```

Para esto necesitas tokio en tu `Cargo.toml` (`tokio = { version = "1", features = ["full"] }`).

> [!WARNING]
> `listen` sirve para siempre: no captura `Ctrl+C` ni `SIGTERM`, así que el proceso muere sin más, junto con las peticiones en curso. `app.workers` y `app.thread_per_core` tampoco tienen efecto: los hilos son los de tu runtime. Para un apagado limpio en tu propio runtime, usa `bind` y `with_graceful_shutdown`.

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

    server.with_graceful_shutdown(shutdown_signal()).await
}

/// Termina con Ctrl+C o, en Unix, con SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        signal(SignalKind::terminate())
            .expect("no se puede escuchar SIGTERM")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
```

| Método de `Server` | Función |
|---|---|
| `server.local_addr()` | La dirección que se usa de verdad (útil con el puerto `0`) |
| `server.run().await` | Sirve para siempre |
| `server.with_graceful_shutdown(signal).await` | Sirve hasta que termina el futuro `signal` y luego se detiene limpiamente |

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

El cuerpo solo se lee si el handler lo pide, así que las rutas que nunca lo leen no se ven afectadas. `req.take_body()`, que te da el flujo en bruto (subidas de archivos, proxies), no tiene límite: contar los bytes queda en tus manos. El ajuste es global para toda la aplicación. Consulta [Peticiones](requests.md) para leer cuerpos.

## Límites integrados en el motor

El motor HTTP/1.1 protege el servidor de peticiones mal formadas o abusivas. Estos valores son fijos:

| Situación | Comportamiento |
|---|---|
| Cabecera de la petición (línea de petición y cabeceras) de más de 60 KiB | `431 Request Header Fields Too Large`, conexión cerrada |
| Más de 64 cabeceras | `431`, conexión cerrada |
| Petición mal formada | `400 Bad Request`, conexión cerrada |
| `Content-Length` y `Transfer-Encoding` a la vez, o dos `Content-Length` distintos | `400` (protección contra *request smuggling*) |
| `Transfer-Encoding` cuya última codificación no es `chunked` | `501 Not Implemented` |
| Cuerpo mayor que `body_limit`, al leerlo | `413 Payload Too Large` |
| Conexión keep-alive sin peticiones durante unos 60 s | Conexión cerrada |
| Cabecera de la petición aún incompleta tras unos 30 s | `408 Request Timeout`, conexión cerrada |
| `Expect: 100-continue` | Se envía `100 Continue` automáticamente antes de leer el cuerpo |
| Cuerpo de petición `chunked` | Se decodifica de forma transparente |
| Cuerpo de hasta 64 KiB | Se lee antes de llamar al handler; los cuerpos más grandes llegan al handler como flujo, a medida que se reciben |
| Cuerpo grande que el handler no lee | La respuesta sale con `connection: close` y después se descarta el resto del envío (2 s y 8 MiB como máximo) para que el cliente reciba la respuesta |
| Peticiones en pipeline | Se responden en orden; las respuestas de un lote salen en una sola llamada al sistema |
| HTTP/1.0 | Conexión cerrada tras la respuesta, salvo que el cliente pida keep-alive |
| Pánico en un handler | `500`, y el servidor sigue funcionando |

En las respuestas, el motor calcula `content-length`, añade la cabecera `date`, envía los cuerpos de tamaño desconocido (flujos) en `chunked`, no envía cuerpo en las respuestas `204`, `304` ni en las peticiones `HEAD`, y cierra la conexión tras la respuesta si tu handler pone una cabecera `connection: close`.

> [!IMPORTANT]
> No hay límite de tiempo para el propio handler: un handler que espera para siempre mantiene su petición abierta para siempre. Añade el middleware `timeout`, que responde `503 Service Unavailable` pasado el tiempo indicado.

```rust
app.middleware(middleware::timeout(Duration::from_secs(30))); // std::time::Duration
```

## Apagado ordenado

Con `app.run`, `Ctrl+C` (SIGINT) y, en Unix, `SIGTERM` inician un apagado ordenado:

1. el servidor deja de aceptar conexiones nuevas;
2. las peticiones en curso tienen hasta **10 segundos** para terminar, y sus respuestas llevan `connection: close`;
3. las conexiones restantes (keep-alive inactivas, peticiones que siguen en curso tras 10 s) se cierran y `run` devuelve `Ok(())`.

Es justo lo que esperan Docker, Kubernetes o Heroku: envían `SIGTERM` y esperan un rato antes de forzar la parada del proceso. Con `bind`, `with_graceful_shutdown(signal)` sigue los mismos pasos en cuanto termina tu futuro `signal`, así que tú decides qué provoca el apagado. `app.listen()` y `Server::run()` nunca se detienen por sí solos. El periodo de gracia de 10 segundos no se puede configurar.

## Solo HTTP/1.1

Vitesse habla HTTP/1.1 (y HTTP/1.0) sobre TCP sin cifrar. Para HTTPS y HTTP/2, ponlo detrás de un proxy inverso (Nginx, Caddy o el balanceador de carga de tu plataforma) que termine el TLS y reenvíe las peticiones a Vitesse en HTTP/1.1, como se suele hacer con Express. El proxy también puede encargarse de la compresión.

Detrás de un proxy, `req.ip()` devuelve la dirección del proxy: la del cliente está en la cabecera `X-Forwarded-For`. Todo esto se explica en [Producción](production.md).
