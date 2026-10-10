# Puesta en producción

Esta página reúne lo importante cuando una aplicación Vitesse sale de tu computadora: una compilación optimizada, la configuración, un proxy inverso para HTTPS y HTTP/2, WebSocket y HTTP/3, un gestor de servicios, el apagado ordenado, los límites, los registros, la seguridad y la monitorización. Para contenedores, consulta también [Docker](docker.md); para Heroku, [Desplegar en Heroku desde el móvil](heroku-mobile.md).

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

## Detrás de un proxy inverso

Vitesse habla HTTP/1.1 sin TLS sobre TCP. En producción, coloca un proxy inverso delante: se encarga de HTTPS y los certificados, de HTTP/2 (y HTTP/3, salvo que Vitesse lo sirva por sí mismo: consulta [HTTP/3](#http3)) y de la compresión, y habla con Vitesse en HTTP/1.1 simple mediante conexiones keep-alive locales.

Vitesse cierra una conexión tras unos 60 segundos sin peticiones (unos 30 segundos si un cliente nunca termina de enviar sus cabeceras). Deja que el proxy reutilice sus conexiones hacia la aplicación y haz que cierre las inactivas un poco antes.

### Caddy

[Caddy](https://caddyserver.com) es la opción más sencilla: obtiene los certificados HTTPS automáticamente y activa HTTP/2 y HTTP/3 por defecto. Un `Caddyfile` completo:

```text
ejemplo.com {
	encode zstd gzip

	reverse_proxy 127.0.0.1:3000 {
		header_up X-Real-IP {remote_host}
		transport http {
			keepalive 30s
		}
	}
}
```

Caddy reutiliza por defecto sus conexiones hacia la aplicación y añade por sí mismo `X-Forwarded-For`, `X-Forwarded-Proto` y `X-Forwarded-Host`. Aplica los cambios con `sudo systemctl reload caddy`.

### Nginx

Con [Nginx](https://nginx.org), el keep-alive hacia la aplicación requiere un bloque `upstream`, `proxy_http_version 1.1` y una cabecera `Connection` vacía:

```nginx
upstream vitesse {
    server 127.0.0.1:3000;
    keepalive 64;            # conexiones inactivas abiertas hacia la app
    keepalive_timeout 30s;   # se cierran antes de que Vitesse lo haga (~60 s)
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
        proxy_set_header X-Real-IP $remote_addr;
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
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_read_timeout 1h;
    proxy_send_timeout 1h;
}
```

Cada WebSocket mantiene abierta su propia conexión entre Nginx y Vitesse durante toda su vida, fuera del pool `keepalive`. Para las conexiones que pueden pasar mucho tiempo en silencio, haz que el servidor envíe un ping con regularidad (consulta [WebSocket](websocket.md#detrás-de-un-proxy-inverso)).

### Obtener la IP del cliente

Detrás de un proxy, `req.ip()` devuelve la dirección del proxy (`127.0.0.1`). Las dos configuraciones anteriores envían la dirección real en `X-Real-IP`:

```rust
app.get("/ip", |req: Request| async move {
    // La pone el proxy inverso; si no, la dirección TCP del cliente.
    req.header("x-real-ip")
        .map(str::to_owned)
        .or_else(|| req.ip().map(|ip| ip.to_string()))
        .unwrap_or_default()
});
```

> [!IMPORTANT]
> Confía en esta cabecera solo si la aplicación es accesible **únicamente** a través del proxy (escuchando en `127.0.0.1`). Si no, cualquier cliente puede enviar un `X-Real-IP` falso.

Si Vitesse también sirve HTTP/3 (más abajo), esas peticiones le llegan directamente, sin pasar por el proxy: `req.ip()` es entonces la dirección real del cliente, y una cabecera `X-Real-IP` solo puede venir del cliente. Lee la cabecera solo en las peticiones HTTP/1.1 (`req.version()`).

## HTTP/3

Los navegadores usan HTTP/3 cuando el sitio lo anuncia. Hay dos formas de ofrecerlo:

- **Que se encargue el proxy.** Caddy activa HTTP/3 por defecto, y Nginx con `listen 443 quic` (versión 1.25 o posterior). Para Vitesse no cambia nada.
- **Que se encargue Vitesse**, con la feature `http3`: el proxy conserva el puerto TCP 443, y Vitesse recibe directamente el puerto UDP 443, con el mismo certificado. Sus respuestas HTTP/1.1 llevan la cabecera `alt-svc`, que el proxy pasa a los navegadores.

```rust
use vitesse::http3::Http3;

app.http3(
    Http3::from_pem_files("/etc/mi-app/tls/fullchain.pem", "/etc/mi-app/tls/privkey.pem")?
        .port(443), // HTTP/3 en UDP 443, anunciado a los navegadores con alt-svc
);
app.run("0.0.0.0:3000") // TCP 3000 para el proxy; el socket UDP usa la misma IP
```

En el segundo caso:

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
# Vitesse da hasta 10 s a las peticiones en curso tras SIGTERM.
TimeoutStopSec=15
# Solo para HTTP/3 en el puerto UDP 443:
# AmbientCapabilities=CAP_NET_BIND_SERVICE
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
2. deja terminar las peticiones en curso durante **10 segundos** como máximo;
3. cierra las conexiones restantes y `app.run` (o `app.listen`) retorna.

Las conexiones HTTP/3 se detienen a la vez, tras un `GOAWAY`, con los mismos 10 segundos para sus peticiones. Las conexiones WebSocket se cierran como muy tarde al final del periodo de gracia: haz que tus clientes se reconecten automáticamente.

Este periodo de gracia de 10 segundos es fijo. Asegúrate de que tu plataforma espera algo más antes de matar el proceso: systemd espera 90 segundos por defecto (`TimeoutStopSec`), Kubernetes 30 segundos (`terminationGracePeriodSeconds`), Heroku 30 segundos, pero Docker solo 10 segundos (usa `--stop-timeout 15`).

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

¿Necesitas registros JSON para un recolector? Escribe tu propio middleware (consulta [Middlewares](middleware.md)):

```rust
app.middleware(|req: Request, next: Next| async move {
    let start = std::time::Instant::now();
    let (method, path) = (req.method().clone(), req.path().to_owned());
    let res = next.run(req).await;
    println!(
        "{}",
        json!({
            "method": method.as_str(),
            "path": path,
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
| Cuerpo leído en memoria (`json`, `form`, `text`, `bytes`) | 1 MiB, después `413` | `app.body_limit(bytes)` |
| Cabecera de la petición (línea de petición y cabeceras) | 60 KiB y 64 cabeceras, después `431` | Fijo |
| Cabecera de la petición incompleta | Unos 30 segundos, después `408` | Fijo |
| Conexión keep-alive inactiva | Se cierra tras unos 60 segundos | Fijo |
| Tiempo dentro de un handler | Ilimitado | `middleware::timeout(duración)`, que responde `503` |

Reduce `body_limit` a lo que tus rutas necesiten de verdad y mantén coherente el límite del proxy (`client_max_body_size` en Nginx).

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

- **HTTPS en todas partes**, gestionado por el proxy, con una redirección de HTTP a HTTPS.
- **La aplicación no está expuesta directamente**: escucha en `127.0.0.1` (o en una red privada) y el cortafuegos solo abre los puertos 80 y 443 (TCP, más UDP 443 si Vitesse sirve [HTTP/3](#http3): entonces escucha en todas las interfaces, y el cortafuegos mantiene cerrado su puerto TCP).
- **Un usuario sin privilegios** ejecuta el proceso (`User=` en systemd; la imagen Docker incluida ya usa un usuario `vitesse`).
- **Cabeceras de seguridad** con `middleware::helmet()`: `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, `Strict-Transport-Security`… Esta última indica a los navegadores que usen solo HTTPS durante un año, subdominios incluidos: actívala cuando HTTPS funcione para el dominio y sus subdominios.
- **CORS** con una lista explícita de orígenes permitidos, y **cookies** marcadas como `http_only`, `secure` y `same_site` (ver más abajo).
- **Límites**: un `body_limit` adaptado a tus rutas, un `timeout` y, si hace falta, limitación de peticiones en el proxy (por ejemplo `limit_req` en Nginx): Vitesse no incluye un limitador de peticiones.
- **Mensajes de error**: los detalles de los errores `5xx` nunca se envían a los clientes, pero los mensajes de los errores que creas tú (`Error::bad_request("…")`) sí: no pongas nada sensible en ellos.
- **Archivos estáticos**: `ServeDir` rechaza `../` y los archivos ocultos (`.env`, `.git`) por defecto; aun así, sirve una carpeta dedicada, nunca la raíz del proyecto.
- **Los secretos** viven en variables de entorno o en un gestor de secretos, nunca en el repositorio ni en la imagen.
- **Dependencias**: actualízalas con regularidad (`cargo update`) y revisa los avisos de seguridad con [cargo-audit](https://github.com/rustsec/rustsec/tree/main/cargo-audit).

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

Prometheus puede recoger este formato de texto. No expongas `/metrics` públicamente: bloquéala en el proxy.
