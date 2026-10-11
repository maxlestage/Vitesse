# Puesta en producción

Esta página reúne lo importante cuando una aplicación Vitesse sale de tu computadora: una compilación optimizada, la configuración, HTTPS (servido por Vitesse o por un proxy inverso), la dirección real del cliente, WebSocket y HTTP/3, un gestor de servicios, el apagado ordenado, los límites, los registros, la seguridad y la monitorización. Para contenedores, consulta también [Docker](docker.md); para Heroku, [Desplegar en Heroku desde el móvil](heroku-mobile.md).

## Compilar en modo release

Despliega siempre una compilación release: una compilación de depuración es muchísimo más lenta.

```sh
cargo build --release
```

Para obtener el mejor rendimiento, añade a **tu** `Cargo.toml` el perfil que usan los benchmarks de Vitesse:

```toml
[profile.release]
lto = "fat"
codegen-units = 1
```

Cargo solo lee los perfiles del paquete raíz (o del workspace), así que la configuración del `Cargo.toml` de Vitesse no se aplica a tu aplicación. La compilación tarda más y el binario es más rápido. Opcionalmente, `strip = true` reduce el tamaño del binario y `debug = "line-tables-only"` conserva trazas y perfiles legibles.

> [!WARNING]
> No uses `panic = "abort"`. Vitesse captura los pánicos de los handlers y los convierte en respuestas `500`, de modo que una petición defectuosa no puede tumbar el servidor. Con `abort`, cualquier pánico mata todo el proceso.

El binario solo depende de la biblioteca C del sistema (glibc): compílalo en la misma distribución Linux que el servidor (o en una más antigua), o compílalo en Docker. `RUSTFLAGS="-C target-cpu=native"` puede ayudar, pero solo si el binario se ejecuta en la máquina que lo compiló (o en una CPU idéntica).

## Configuración desde el entorno

Lee la configuración de variables de entorno, así el mismo binario funciona en todas partes:

```rust
use std::time::Duration;

use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.middleware(middleware::logger()); // una línea por petición en stdout
    app.middleware(middleware::helmet()); // cabeceras de seguridad
    app.middleware(middleware::timeout(Duration::from_secs(15))); // 503 si tarda demasiado
    app.body_limit(256 * 1024); // 256 KiB en lugar de 1 MiB
    app.trust_proxy("loopback"); // un proxy inverso en esta máquina (ver más abajo)

    app.get("/health", |_| async { "ok" });
    app.get("/", |_| async { "¡Hola desde producción!" });

    // La configuración viene del entorno.
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
    if let Some(n) = std::env::var("WORKERS").ok().and_then(|n| n.parse().ok()) {
        app.workers(n);
    }

    app.run(format!("{host}:{port}"))
}
```

- **`PORT`** es la convención que usan la mayoría de las plataformas (Heroku, Render, Cloud Run…).
- **`HOST`**: `0.0.0.0` en un contenedor o en una plataforma; `127.0.0.1` detrás de un proxy inverso en la misma máquina, para que la aplicación no sea accesible directamente desde fuera. Usa `[::]` para IPv6.
- Un número de puerto a secas (`app.run(3000)`, o una cadena formada solo por dígitos) escucha en todas las interfaces IPv4.

`HOST` y `WORKERS` son solo nombres elegidos para este ejemplo: Vitesse no lee ninguna variable de entorno por sí mismo.

## HTTPS: directamente o a través de un proxy

Hay dos configuraciones que funcionan bien:

- **Vitesse sirve HTTPS por sí mismo**, con las features `tls` y `http2` (y `http3` si quieres): un solo binario en los puertos 80 y 443, sin ningún otro componente. La renovación de los certificados corre de tu cuenta (certbot y después un reinicio). Consulta [HTTPS y HTTP/2](https.md).
- **Un proxy inverso delante** (Caddy, Nginx): se encarga de HTTPS, de los certificados y de HTTP/2, puede alojar varias aplicaciones en la misma máquina y repartir la carga entre instancias, y habla con Vitesse en HTTP/1.1 simple. Las secciones siguientes describen esta configuración.

En una plataforma (Heroku, Cloud Run, Render…), HTTPS ya se gestiona en el borde de la plataforma: no necesitas ninguna de las dos.

## Detrás de un proxy inverso

En esta configuración, el proxy se encarga de HTTPS y los certificados, de HTTP/2 (y HTTP/3, salvo que Vitesse lo sirva por sí mismo: consulta [HTTP/3](#http3)), y habla con Vitesse en HTTP/1.1 simple mediante conexiones keep-alive locales. La compresión puede hacerse en cualquiera de los dos lados (`encode` en Caddy, `gzip` en Nginx, o `middleware::compress()` en Vitesse).

Vitesse cierra una conexión tras unos 60 segundos sin peticiones (`app.keep_alive`), y tras unos 30 segundos si un cliente nunca termina de enviar sus cabeceras (`app.header_timeout`). Deja que el proxy reutilice sus conexiones hacia la aplicación y haz que cierre las inactivas un poco antes. Declara el proxy con `app.trust_proxy(...)` para que `req.ip()` devuelva la dirección del cliente ([más abajo](#obtener-la-ip-del-cliente)).

### Caddy

[Caddy](https://caddyserver.com) es la opción más sencilla: obtiene los certificados HTTPS automáticamente y activa HTTP/2 y HTTP/3 por defecto. Un `Caddyfile` completo:

```text
ejemplo.com {
	encode zstd gzip

	reverse_proxy 127.0.0.1:3000 {
		transport http {
			keepalive 30s
		}
	}
}
```

Caddy reutiliza por defecto sus conexiones hacia la aplicación y añade por sí mismo `X-Forwarded-For`, `X-Forwarded-Proto` y `X-Forwarded-Host` (reemplazando los valores que haya podido enviar un cliente). Aplica los cambios con `sudo systemctl reload caddy`.

### Nginx

Con [Nginx](https://nginx.org), el keep-alive hacia la aplicación requiere un bloque `upstream`, `proxy_http_version 1.1` y una cabecera `Connection` vacía:

```nginx
upstream vitesse {
    server 127.0.0.1:3000;
    keepalive 64;            # conexiones inactivas abiertas hacia la app
    keepalive_timeout 30s;   # se cierran antes de que Vitesse lo haga (app.keep_alive, 60 s)
}

server {
    listen 80;
    listen [::]:80;
    server_name ejemplo.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    listen [::]:443 ssl;
    http2 on;                # Nginx < 1.25.1: usa "listen 443 ssl http2;"
    server_name ejemplo.com;

    ssl_certificate     /etc/letsencrypt/live/ejemplo.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/ejemplo.com/privkey.pem;

    client_max_body_size 1m; # coherente con app.body_limit

    location / {
        proxy_pass http://vitesse;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

Los certificados pueden venir de Let's Encrypt, por ejemplo con `sudo certbot --nginx -d ejemplo.com`. Comprueba la configuración con `sudo nginx -t` y aplícala con `sudo systemctl reload nginx`.

### WebSocket detrás de Nginx

Caddy deja pasar las conexiones [WebSocket](websocket.md) sin ninguna configuración. Nginx necesita reenviar explícitamente las cabeceras `Upgrade` y `Connection`, y un `proxy_read_timeout` largo: por defecto, cierra una conexión que lleva 60 segundos en silencio. Añade una `location` para tus rutas WebSocket (aquí, todo lo que está bajo `/ws/`) al bloque `server` anterior:

```nginx
location /ws/ {
    proxy_pass http://vitesse;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_read_timeout 1h;
    proxy_send_timeout 1h;
}
```

Cada WebSocket mantiene abierta su propia conexión entre Nginx y Vitesse durante toda su vida, fuera del pool `keepalive`. Para las conexiones que pueden pasar mucho tiempo en silencio, haz que el servidor envíe un ping con regularidad (consulta [WebSocket](websocket.md#detrás-de-un-proxy-inverso)).

### A través de un socket Unix

Cuando el proxy se ejecuta en la misma máquina, Vitesse puede escuchar en un socket de dominio Unix en lugar de en un puerto TCP (Linux y macOS): no hay puerto que mantener cerrado, y los permisos del archivo deciden quién puede conectarse. Ábrelo con `bind_unix`, para poder fijar los permisos antes de servir:

```rust
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;

use vitesse::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.trust_proxy(1); // un proxy delante: la última dirección de X-Forwarded-For es el cliente
    app.get("/", |req: Request| async move { format!("¡Hola {:?}!", req.ip()) });

    let server = app.bind_unix("/run/mi-app/app.sock").await?;
    // Lectura y escritura para el usuario de la aplicación y su grupo: añade el usuario de Nginx a ese grupo.
    std::fs::set_permissions(server.path(), Permissions::from_mode(0o660))?;
    server.run().await // hasta SIGTERM; después se borra el archivo del socket
}
```

En el lado de Nginx, apunta el `upstream` al socket y conserva el resto de la configuración anterior:

```nginx
upstream vitesse {
    server unix:/run/mi-app/app.sock;
    keepalive 64;
    keepalive_timeout 30s;
}
```

(Sin bloque `upstream`, `proxy_pass http://unix:/run/mi-app/app.sock;` también funciona, sin keep-alive.) Caddy se conecta con `reverse_proxy unix//run/mi-app/app.sock`.

- Con systemd, `RuntimeDirectory=mi-app` crea `/run/mi-app`, propiedad del usuario del servicio, en cada arranque; añade el usuario de Nginx al grupo de la aplicación con `sudo usermod -aG vitesse www-data`.
- Un socket Unix no tiene dirección de cliente: sin `trust_proxy`, `req.ip()` es `None`. Con él, las cabeceras del proxy dan la dirección, el protocolo y el host del cliente.
- Un archivo de socket que haya quedado tras un fallo se reemplaza en el siguiente arranque, y el archivo se borra en un apagado limpio. [HTTP/3](#http3) necesita un puerto UDP: no se abre con un socket Unix.

Consulta [Configuración del servidor](server.md#sockets-de-dominio-unix) para los detalles.

### Obtener la IP del cliente

Detrás de un proxy, la conexión viene del proxy: por defecto, `req.ip()` devuelve su dirección (`127.0.0.1`), y `req.protocol()` dice `"http"` incluso para los visitantes que usan HTTPS. El proxy pasa los valores reales en las cabeceras `X-Forwarded-For`, `X-Forwarded-Proto` y `X-Forwarded-Host`. Indica a Vitesse en qué proxies confiar con `app.trust_proxy`, como el ajuste `trust proxy` de Express:

```rust
let mut app = App::new();
app.trust_proxy("loopback"); // Caddy o Nginx en la misma máquina

app.get("/whoami", |req: Request| async move {
    Json(json!({
        "ip": req.ip(),             // el cliente, sacado de X-Forwarded-For
        "protocol": req.protocol(), // "https", sacado de X-Forwarded-Proto
        "host": req.hostname(),     // de X-Forwarded-Host, o de Host
    }))
});
```

| `trust_proxy(...)` | Confía en | Uso |
|---|---|---|
| `false` (por defecto) | Nadie: `req.ip()` es la dirección de la conexión | Sin proxy |
| `"loopback"` | Los proxies en `127.0.0.1` o `::1` | Un proxy en la misma máquina |
| `"10.0.0.0/8, 192.168.1.10"` | Los proxies en estas direcciones o en estas redes (CIDR) | Un balanceador de carga en una red privada |
| `"uniquelocal"`, `"linklocal"` | Las redes privadas (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `fc00::/7`), las direcciones de enlace local | Contenedores, redes privadas en la nube |
| `1`, `2`… | Los `n` proxies más cercanos a la aplicación, sea cual sea su dirección | Una cadena de longitud conocida, o un [socket Unix](#a-través-de-un-socket-unix) |
| `true` | Todos los proxies: el cliente es la dirección más a la izquierda de `X-Forwarded-For` | Solo detrás de un proxy que reemplaza `X-Forwarded-For` (Caddy) en lugar de añadirle valores |

Los nombres y las redes se pueden combinar: `"loopback, 10.0.0.0/8"`. Una dirección no válida detiene la aplicación al arrancar; para gestionar tú mismo el error, analiza el valor con `"…".parse::<vitesse::TrustProxy>()`.

Qué cambia para las peticiones que llegan a través de un proxy de confianza:

- `req.ip()` recorre `X-Forwarded-For` desde la derecha, saltándose los proxies de confianza: la primera dirección que no es uno de ellos es el cliente. `middleware::rate_limit` cuenta entonces por cliente, no por proxy;
- `req.protocol()` y `req.secure()` siguen a `X-Forwarded-Proto`;
- `req.host()` y `req.hostname()` leen `X-Forwarded-Host`, cuando el proxy la envía (si no, la cabecera `Host`, que la configuración de Nginx anterior reenvía);
- `req.remote_addr()` sigue siendo la dirección de la propia conexión: la del proxy.

> [!IMPORTANT]
> Confía solo en tus propios proxies. Estas cabeceras son texto plano: un cliente también puede enviarlas. Con `true` o un número de saltos, asegúrate de que la aplicación es accesible **únicamente** a través del proxy (escuchando en `127.0.0.1` o en un socket Unix); una lista de direcciones es más segura, porque las cabeceras de cualquier otro interlocutor se ignoran.

Si Vitesse también sirve HTTP/3 (más abajo), esas peticiones le llegan directamente, sin pasar por el proxy: su interlocutor es el propio cliente. Con una lista de direcciones como `"loopback"`, eso se resuelve solo: el cliente no es un proxy de confianza, así que sus cabeceras `X-Forwarded-*` se ignoran y `req.ip()` es su dirección real.

## HTTP/3

Los navegadores usan HTTP/3 cuando el sitio lo anuncia. Hay tres formas de ofrecerlo:

- **Que se encargue el proxy.** Caddy activa HTTP/3 por defecto, y Nginx con `listen 443 quic` (versión 1.25 o posterior). Para Vitesse no cambia nada.
- **Que se encargue Vitesse detrás del proxy**, con la feature `http3`: el proxy conserva el puerto TCP 443, y Vitesse recibe directamente el puerto UDP 443, con el mismo certificado. Sus respuestas HTTP/1.1 llevan la cabecera `alt-svc`, que el proxy pasa a los navegadores.
- **Que Vitesse se encargue de todo**, con las features `tls` y `http3`: HTTPS en TCP 443 y HTTP/3 en UDP 443, sin proxy (consulta [HTTPS y HTTP/3 juntos](https.md#https-y-http3-juntos)).

El segundo caso tiene este aspecto:

```rust
use vitesse::http3::Http3;

app.http3(
    Http3::from_pem_files("/etc/mi-app/tls/fullchain.pem", "/etc/mi-app/tls/privkey.pem")?
        .port(443), // HTTP/3 en UDP 443, anunciado a los navegadores con alt-svc
);
app.run("0.0.0.0:3000") // TCP 3000 para el proxy; el socket UDP usa la misma IP
```

En ese caso:

- abre el puerto **UDP** 443 en el cortafuegos (`sudo ufw allow 443/udp`) y mantén cerrado el puerto TCP 3000;
- con systemd, añade `AmbientCapabilities=CAP_NET_BIND_SERVICE` para que el usuario del servicio pueda abrir el puerto 443;
- Nginx no debe escuchar en `443 quic`, y Caddy necesita `protocols h1 h2` en sus opciones globales, para dejar el puerto UDP 443 a Vitesse;
- el certificado se carga al arrancar: reinicia la aplicación tras cada renovación.

Todo se explica en detalle en [HTTP/3 y QUIC](http3.md#en-producción-detrás-de-caddy-o-nginx).

## Ejecutarla como servicio con systemd

En un servidor Linux, systemd arranca la aplicación al iniciar el sistema, la reinicia si se cae y recoge sus registros. Crea un usuario dedicado y copia el binario:

```sh
sudo useradd --system --no-create-home vitesse
sudo mkdir -p /opt/mi-app
sudo cp target/release/mi-app /opt/mi-app/server
```

Luego crea `/etc/systemd/system/mi-app.service`:

```ini
[Unit]
Description=Mi aplicación Vitesse
After=network.target

[Service]
User=vitesse
Group=vitesse
WorkingDirectory=/opt/mi-app
ExecStart=/opt/mi-app/server
Environment=HOST=127.0.0.1
Environment=PORT=3000
# O guarda la configuración en un archivo aparte:
# EnvironmentFile=/etc/mi-app.env
Restart=on-failure
RestartSec=2
# Vitesse da hasta 10 s a las peticiones en curso tras SIGTERM
# (app.shutdown_timeout): deja este valor unos segundos por encima.
TimeoutStopSec=15
# Solo para abrir puertos por debajo de 1024 (HTTPS en el 443, HTTP/3 en UDP 443):
# AmbientCapabilities=CAP_NET_BIND_SERVICE
# Solo para un socket Unix en /run/mi-app:
# RuntimeDirectory=mi-app
# Un descriptor de archivo por conexión.
LimitNOFILE=65536
# Endurecimiento (añade ReadWritePaths=... si la aplicación escribe archivos).
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now mi-app
systemctl status mi-app
journalctl -u mi-app -f        # seguir los registros
```

Para desplegar una nueva versión, sustituye el binario (cópialo junto al antiguo y luego renómbralo: copiar encima de un binario en ejecución falla) y reinicia:

```sh
sudo cp target/release/mi-app /opt/mi-app/server.new
sudo mv /opt/mi-app/server.new /opt/mi-app/server
sudo systemctl restart mi-app
```

Un reinicio dura una fracción de segundo, pero las conexiones que lleguen en ese momento fallan. Para desplegar sin cortes, ejecuta dos instancias en dos puertos detrás del proxy y reinícialas una tras otra.

## Apagado ordenado

`app.run`, `app.listen(port).await` y `Server::run` vigilan `Ctrl+C` (`SIGINT`) y `SIGTERM`, que systemd, Docker, Kubernetes o Heroku envían antes de detener una aplicación. Entonces Vitesse:

1. deja de aceptar conexiones nuevas;
2. deja terminar las peticiones en curso durante **10 segundos** como máximo, por defecto;
3. cierra las conexiones restantes y `app.run` (o `app.listen`) retorna.

Las conexiones HTTP/2 y HTTP/3 se detienen a la vez, tras un `GOAWAY`, con el mismo periodo de gracia para sus peticiones. Las conexiones WebSocket se cierran como muy tarde al final del periodo de gracia: haz que tus clientes se reconecten automáticamente.

Cambia el periodo de gracia con `app.shutdown_timeout`, por ejemplo si algunas peticiones tardan más de 10 segundos:

```rust
app.shutdown_timeout(Duration::from_secs(25)); // std::time::Duration
```

Asegúrate de que tu plataforma espera algo más que eso antes de matar el proceso: systemd espera 90 segundos por defecto (`TimeoutStopSec`), Kubernetes 30 segundos (`terminationGracePeriodSeconds`), Heroku 30 segundos, pero Docker solo 10 segundos (`--stop-timeout 15` con el periodo de gracia por defecto, `--stop-timeout 30` con los 25 segundos de arriba).

Para ejecutar tu propio código cuando llega la señal (una línea de registro, vaciar un búfer), o para detenerte con otro evento, abre el puerto con `app.bind` y pasa tu propio futuro a `with_graceful_shutdown`. Sustituye a `Ctrl+C` y `SIGTERM`, así que inclúyelos si todavía los necesitas:

```rust
use tokio::signal::unix::{SignalKind, signal};
use vitesse::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "ok" });

    let server = app.bind("0.0.0.0:3000").await?;
    println!("escuchando en http://{}", server.local_addr());

    let mut sigterm = signal(SignalKind::terminate())?;
    server
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = sigterm.recv() => {}
            }
            println!("apagando…");
        })
        .await
}
```

Este ejemplo (solo Unix) necesita `tokio = { version = "1", features = ["macros", "rt-multi-thread", "signal"] }` en tus dependencias.

## Comprobaciones de salud

Una ruta ligera que responda rápido basta para los balanceadores de carga, las sondas de Kubernetes y los servicios de monitorización:

```rust
app.get("/health", |_| async { "ok" });
```

Si la aplicación depende de una base de datos, añade una ruta aparte (por ejemplo `/ready`) que la compruebe, y mantén `/health` independiente: una caída de la base de datos no debería provocar el reinicio de todas las instancias. Los middlewares globales, como el logger, también se ejecutan para estas peticiones.

## Registros

- `middleware::logger()` escribe una línea por petición en la salida estándar, como `GET /users/42 200 0.084 ms`. Solo usa colores cuando la salida es una terminal, así los registros de journald, Docker y las plataformas quedan limpios.
- Cuando un error `5xx` tiene una causa (un error de Rust convertido con `?`, o `Error::with_source`), esa causa se imprime en la salida de error con el prefijo `[vitesse]`; el cliente solo recibe un mensaje genérico.
- Un pánico en un handler o en un middleware se convierte en un `500`, y Rust imprime el mensaje del pánico en la salida de error.
- `middleware::request_id()` da un id a cada petición: el `X-Request-Id` que envía el proxy o el servicio que llama, o un UUID nuevo. Se devuelve en la cabecera `X-Request-Id` de la respuesta, así que un usuario que informa de un error puede darte el id que buscar en los registros.

¿Necesitas registros JSON para un recolector? Escribe tu propio middleware (consulta [Middlewares](middleware.md)). `req.route()` da el patrón de la ruta elegida (`/users/:id` en lugar de `/users/42`), más fácil de agrupar y de buscar que la ruta en bruto (consulta [La ruta elegida](routing.md#la-ruta-elegida-reqroute)):

```rust
use vitesse::middleware::RequestId;

app.middleware(middleware::request_id()); // primero, para que el siguiente middleware vea el id
app.middleware(|req: Request, next: Next| async move {
    let start = std::time::Instant::now();
    let method = req.method().clone();
    let path = req.path().to_owned();
    let route = req.route(); // antes de next.run, que se queda con la petición
    let id = req.get::<RequestId>().map(|id| id.to_string());
    let res = next.run(req).await;
    println!(
        "{}",
        json!({
            "id": id,
            "method": method.as_str(),
            "path": path,
            "route": route,
            "status": res.status_code().as_u16(),
            "ms": start.elapsed().as_secs_f64() * 1000.0,
        })
    );
    res
});
```

## Límites

| Qué | Por defecto | Cómo cambiarlo |
|---|---|---|
| Cuerpo leído en memoria (`json`, `form`, `text`, `bytes`, `multipart`) | 1 MiB, después `413` | `app.body_limit(bytes)`; por ruta o por router: `middleware::body_limit(bytes)` |
| Cabecera de la petición (línea de petición y cabeceras) | 60 KiB y 64 cabeceras, después `431` | Fijo |
| Cabecera de la petición incompleta | Unos 30 segundos, después `408` | `app.header_timeout(duración)` |
| Conexión keep-alive inactiva | Se cierra tras unos 60 segundos | `app.keep_alive(duración)` |
| Conexiones abiertas | Ilimitadas | `app.max_connections(n)`, por bucle de aceptación (por thread en el modo de un thread por núcleo) |
| Tiempo dentro de un handler | Ilimitado | `middleware::timeout(duración)`, que responde `503` |
| Peticiones atendidas a la vez | Ilimitadas | `middleware::concurrency_limit(n)`: las demás esperan, o reciben `503` con `.max_wait(duración)` |
| Peticiones por cliente | Ilimitadas | `middleware::rate_limit(max, periodo)`, que responde `429` con `Retry-After` |
| Periodo de gracia al apagar | 10 segundos | `app.shutdown_timeout(duración)` |

```rust
use std::time::Duration;

app.header_timeout(Duration::from_secs(10)); // expuesta a internet sin proxy
app.middleware(middleware::rate_limit(300, Duration::from_secs(60))); // 300 peticiones por minuto y por IP
app.post("/upload", upload.with(middleware::body_limit(20 * 1024 * 1024))); // 20 MiB solo en esta ruta
app.get(
    "/report",
    report.with(middleware::concurrency_limit(4).max_wait(Duration::from_secs(2))),
);
```

Reduce `body_limit` a lo que tus rutas necesiten de verdad, súbelo solo en las rutas que reciben archivos y mantén coherente el límite del proxy (`client_max_body_size` en Nginx).

`rate_limit` identifica a los clientes por `req.ip()`: detrás de un proxy, configura [`trust_proxy`](#obtener-la-ip-del-cliente), o todos los clientes compartirán la dirección del proxy y su cuota. Sus contadores viven en la memoria del proceso: con varias instancias, cada una cuenta por su cuenta; para un límite compartido entre instancias, limita en el proxy (`limit_req` en Nginx) o en un almacén compartido. Consulta [Middlewares](middleware.md) para todas las opciones.

## Workers y threads

- Por defecto, `app.run` arranca **un thread por núcleo**. En Linux, cada thread tiene su propio bucle de eventos y su propio socket `SO_REUSEPORT`: el kernel reparte las conexiones entre los threads y una petición nunca cambia de thread. Es el modo más rápido.
- `app.workers(n)` fija explícitamente el número de threads: útil en un contenedor cuya cuota de CPU es inferior al número de núcleos que ve, o para dejar núcleos libres a otros procesos (una base de datos en la misma máquina, por ejemplo).
- `app.thread_per_core(false)` cambia al runtime multihilo de tokio, que reequilibra el trabajo entre threads. Úsalo si los handlers hacen cálculos largos, o si las conexiones son pocas y muy desiguales. Detrás de un proxy, todo el tráfico llega por el grupo de conexiones keep-alive del proxy: dale suficientes conexiones (`keepalive 64` en el ejemplo de Nginx) para que se repartan entre todos los threads, o desactiva este modo si un núcleo se satura mientras los demás están ociosos.

Nunca bloquees un thread dentro de un handler (`std::thread::sleep`, un cálculo largo, un driver de base de datos síncrono…): todas las conexiones atendidas por ese thread quedarían esperando. Mueve ese trabajo al pool de threads bloqueantes de tokio:

```rust
app.get("/informe", |_| async {
    let informe = vitesse::tokio::task::spawn_blocking(calculo_costoso)
        .await
        .map_err(|e| Error::internal("la tarea falló").with_source(e))?;
    Ok::<_, Error>(informe)
});
```

Consulta [Configuración del servidor](server.md) y [Rendimiento](performance.md) para profundizar.

## Lista de comprobación de seguridad

- **HTTPS en todas partes**, servido por [el propio Vitesse](https.md) o por el proxy, con una redirección de HTTP a HTTPS.
- **Detrás de un proxy, la aplicación no está expuesta directamente**: escucha en `127.0.0.1`, en una red privada o en un [socket Unix](#a-través-de-un-socket-unix), y el cortafuegos solo abre los puertos 80 y 443 (TCP, más UDP 443 si Vitesse sirve [HTTP/3](#http3): entonces escucha en todas las interfaces, y el cortafuegos mantiene cerrado su puerto TCP).
- **`trust_proxy` solo enumera tus proxies**, para que nadie pueda elegir su propio `req.ip()` con un `X-Forwarded-For` falso (ver [más arriba](#obtener-la-ip-del-cliente)).
- **Un usuario sin privilegios** ejecuta el proceso (`User=` en systemd; la imagen Docker incluida ya usa un usuario `vitesse`).
- **Cabeceras de seguridad** con `middleware::helmet()`: `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, `Strict-Transport-Security`… Esta última indica a los navegadores que usen solo HTTPS durante un año, subdominios incluidos: actívala cuando HTTPS funcione para el dominio y sus subdominios.
- **CORS** con una lista explícita de orígenes permitidos, y **cookies** marcadas como `http_only`, `secure` y `same_site` (ver más abajo).
- **Límites**: un `body_limit` adaptado a tus rutas (subido solo donde se suben archivos), un `timeout` y [limitación de peticiones](#límites) con `middleware::rate_limit` en las rutas sensibles (inicio de sesión, restablecimiento de contraseña, API costosas), o en el proxy (`limit_req` en Nginx).
- **Autenticación**: `middleware::basic_auth` y `middleware::bearer_auth` envían la contraseña o el token con cada petición: úsalos solo sobre HTTPS. Comparan las credenciales en tiempo constante; guarda las propias credenciales en variables de entorno. Pon un `rate_limit` delante de ellos para frenar los intentos de adivinarlas.
- **Ids de petición** con `middleware::request_id()`, para poder relacionar el informe de error de un usuario con tus registros (consulta [Registros](#registros)).
- **Mensajes de error**: los detalles de los errores `5xx` nunca se envían a los clientes, pero los mensajes de los errores que creas tú (`Error::bad_request("…")`) sí: no pongas nada sensible en ellos.
- **Archivos estáticos**: `ServeDir` rechaza `../` y los archivos ocultos (`.env`, `.git`) por defecto; aun así, sirve una carpeta dedicada, nunca la raíz del proyecto.
- **Los secretos** viven en variables de entorno o en un gestor de secretos, nunca en el repositorio ni en la imagen.
- **Dependencias**: actualízalas con regularidad (`cargo update`) y revisa los avisos de seguridad con cargo-audit.

### CORS y cookies

```rust
use std::time::Duration;

use vitesse::SameSite;
use vitesse::prelude::*;

let mut app = App::new();

// Solo este origen puede llamar a la API con las cookies del usuario.
app.middleware(
    middleware::cors()
        .allow_origin("https://app.ejemplo.com")
        .allow_credentials(true)
        .max_age(Duration::from_secs(600)),
);

app.post("/login", |_| async {
    Response::new()
        .cookie(
            Cookie::new("session", "abc123")
                .http_only(true)
                .secure(true)
                .same_site(SameSite::Lax),
        )
        .json(json!({ "ok": true }))
});
```

> [!WARNING]
> `cors().allow_credentials(true)` **sin** `allow_origin` acepta todos los orígenes: cualquier sitio web podría enviar peticiones con las cookies de tus usuarios.

## Monitorización

- **Registros**: `journalctl -u mi-app`, `docker logs` o el visor de tu plataforma; envíalos a un servicio de registros si necesitas búsquedas y alertas.
- **Disponibilidad**: un servicio de monitorización externo que llame a `/health` cada minuto.
- **Sistema**: vigila la CPU, la memoria y el número de descriptores de archivo abiertos (uno por conexión, limitado por `LimitNOFILE` en systemd).

Vitesse no trae métricas integradas, pero un middleware y unos cuantos contadores atómicos cubren lo básico:

```rust
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

#[derive(Default)]
struct Metrics {
    requests: AtomicU64,
    server_errors: AtomicU64,
}

app.state(Metrics::default());

app.middleware(|req: Request, next: Next| async move {
    let metrics = req.state::<Metrics>();
    let res = next.run(req).await;
    metrics.requests.fetch_add(1, Relaxed);
    if res.status_code().is_server_error() {
        metrics.server_errors.fetch_add(1, Relaxed);
    }
    res
});

app.get("/metrics", |req: Request| async move {
    let m = req.state::<Metrics>();
    format!(
        "http_requests_total {}\nhttp_server_errors_total {}\n",
        m.requests.load(Relaxed),
        m.server_errors.load(Relaxed),
    )
});
```

Prometheus puede recoger este formato de texto. No expongas `/metrics` públicamente: bloquéala en el proxy, o protege la ruta con `middleware::bearer_auth(token)`, añadido con `.with()`, y dale el token a Prometheus.

Para contar peticiones o medir duraciones por ruta, etiquétalas con `req.route()`: `/users/:id` da una sola serie para todos los usuarios, donde la ruta en bruto crearía una serie nueva por cada id. Léelo antes de `next.run(req)`; `None` significa que ninguna ruta coincidió (un `404`). Consulta [La ruta elegida](routing.md#la-ruta-elegida-reqroute).
