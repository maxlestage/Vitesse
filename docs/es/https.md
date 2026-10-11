# HTTPS y HTTP/2

Vitesse puede servir HTTPS por sí mismo, con TLS sobre TCP, y hablar HTTP/2 con los clientes que lo ofrecen: todos los navegadores lo hacen. No hace falta un proxy inverso para ninguno de los dos, aunque un proxy sigue siendo una buena opción (consulta [el final de esta página](#tls-directamente-o-un-proxy-inverso)). Las rutas, los middlewares y los handlers no cambian: solo cambia la configuración.

## Activar las features `tls` y `http2`

Las dos son features de Cargo que se activan explícitamente:

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tls", "http2"] }
```

| Feature | Añade | Incorpora |
|---|---|---|
| `tls` | `app.tls(...)` y el módulo `vitesse::tls`: HTTPS, TLS 1.2 y 1.3 | [rustls](https://crates.io/crates/rustls) (con la biblioteca criptográfica ring) y tokio-rustls: todo en Rust, sin OpenSSL que instalar |
| `http2` | HTTP/2: a través de TLS con `tls`, y sin cifrar para los clientes que empiezan en HTTP/2 | [h2](https://crates.io/crates/h2), la implementación de HTTP/2 que usa hyper |

Las dos features son independientes. `tls` sola sirve HTTPS en HTTP/1.1; `http2` sola sirve HTTP/2 sin TLS ([h2c](#http2-sin-cifrar-h2c)); juntas, dan a los navegadores lo que esperan de un sitio moderno.

## Un ejemplo completo

```rust
use vitesse::prelude::*;
use vitesse::tls::Tls;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());
    app.get("/", |req: Request| async move {
        // "¡Hola en HTTP/2.0 (https)!" en un navegador
        format!("¡Hola en {:?} ({})!", req.version(), req.protocol())
    });

    // Un certificado y su clave privada, en PEM.
    app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?);

    app.run(443) // HTTPS en el puerto TCP 443
}
```

Una vez configurado `app.tls(...)`, cada conexión TCP en el puerto empieza con un handshake TLS (un cliente tiene 10 segundos para completarlo). Con la feature `http2`, los clientes que ofrecen HTTP/2 durante el handshake (ALPN) lo obtienen, y los demás siguen en HTTP/1.1, en el mismo puerto.

HTTPS funciona con las tres formas de arrancar el servidor: `app.run(addr)`, `app.listen(addr).await` y `app.bind(addr).await?` (consulta [Configuración del servidor](server.md#tres-formas-de-arrancar-el-servidor)). El puerto solo habla TLS: una petición `http://` sin cifrar no recibe ninguna respuesta. Para redirigir HTTP a HTTPS, abre también el puerto 80 ([más abajo](#redirigir-http-a-https)).

## Certificados

Vitesse necesita un certificado para tu dominio y su clave privada. Tres constructores:

| Constructor | Uso |
|---|---|
| `Tls::from_pem_files(cadena, clave)` | Dos archivos PEM, por ejemplo `fullchain.pem` y `privkey.pem` de Let's Encrypt |
| `Tls::from_pem(bytes_cadena, bytes_clave)` | Lo mismo, desde la memoria (un gestor de secretos, una variable de entorno…) |
| `Tls::from_rustls(config)` | Tu propio `rustls::ServerConfig`: certificados de cliente, varios dominios… |

`tls.config()` devuelve la configuración de rustls en uso. La cadena contiene primero el certificado del servidor y después los certificados intermedios: justo lo que guarda `fullchain.pem`. La clave privada puede estar en formato PKCS#8, SEC1 (EC) o PKCS#1 (RSA), en PEM. Si un archivo no se puede leer o analizar, `from_pem_files` devuelve un `io::Error` antes de que arranque el servidor. `from_pem` y `from_pem_files` aceptan TLS 1.2 y 1.3.

### Let's Encrypt

[certbot](https://certbot.eff.org) obtiene certificados gratuitos de Let's Encrypt. Como Vitesse ocupa el puerto 80 (para la [redirección](#redirigir-http-a-https)), usa el modo webroot: certbot escribe un archivo en una carpeta, y Let's Encrypt lo descarga por HTTP.

```sh
sudo certbot certonly --webroot -w /var/www/acme -d example.com -d www.example.com
```

La aplicación de redirección de más abajo sirve esa carpeta bajo `/.well-known/acme-challenge/`. El certificado cubre todos los nombres indicados con `-d`, y se guarda en `/etc/letsencrypt/live/example.com/` (`fullchain.pem` y `privkey.pem`).

> [!IMPORTANT]
> El certificado se carga una sola vez, al arrancar. Let's Encrypt renueva los certificados más o menos cada dos meses: reinicia la aplicación tras cada renovación, desde un hook de certbot (`certbot renew --deploy-hook "systemctl restart mi-app"`). Las claves de Let's Encrypt solo las puede leer root: cópialas a una carpeta que el usuario del servicio pueda leer, desde el mismo hook.

### Tu propia configuración de rustls

Para certificados de cliente (TLS mutuo), un certificado por dominio o recargar los certificados sin reiniciar, construye tú mismo la configuración de rustls. `vitesse::tls::rustls` reexporta la versión de rustls que usa Vitesse, así que los tipos siempre coinciden. Aquí, solo entran los clientes con un certificado firmado por tu propia autoridad:

```rust
use std::sync::Arc;

use vitesse::prelude::*;
use vitesse::tls::rustls::pki_types::pem::PemObject;
use vitesse::tls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use vitesse::tls::rustls::server::WebPkiClientVerifier;
use vitesse::tls::{Tls, rustls};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let chain = CertificateDer::pem_file_iter("fullchain.pem")?.collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_file("privkey.pem")?;

    // La autoridad que firma los certificados de tus clientes.
    let mut client_ca = rustls::RootCertStore::empty();
    for cert in CertificateDer::pem_file_iter("clients-ca.pem")? {
        client_ca.add(cert?)?;
    }

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier =
        WebPkiClientVerifier::builder_with_provider(Arc::new(client_ca), provider.clone()).build()?;
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()? // TLS 1.2 y 1.3
        .with_client_cert_verifier(verifier) // sin certificado válido, no hay conexión
        .with_single_cert(chain, key)?;

    let mut app = App::new();
    app.get("/", |_| async { "¡Hola, cliente de confianza!" });
    app.tls(Tls::from_rustls(config)); // ALPN "h2" y "http/1.1" se añaden si faltan
    app.run(443)?;
    Ok(())
}
```

Si tu configuración no indica ningún protocolo ALPN, `Tls::from_rustls` ofrece `h2` (con la feature `http2`) y `http/1.1`. Si defines tú mismo `alpn_protocols`, incluye `b"h2"` para conservar HTTP/2.

**Varios dominios.** Un solo certificado puede cubrir varios nombres (`-d example.com -d example.org` con certbot): es la opción más sencilla. Para usar un certificado por dominio, deja que rustls lo elija a partir del nombre que pide el cliente (SNI):

```rust
use std::sync::Arc;

use vitesse::prelude::*;
use vitesse::tls::rustls::pki_types::pem::PemObject;
use vitesse::tls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use vitesse::tls::rustls::server::ResolvesServerCertUsingSni;
use vitesse::tls::rustls::sign::CertifiedKey;
use vitesse::tls::{Tls, rustls};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let load = |dir: &str| -> Result<CertifiedKey, Box<dyn std::error::Error>> {
        let chain = CertificateDer::pem_file_iter(format!("{dir}/fullchain.pem"))?
            .collect::<Result<Vec<_>, _>>()?;
        let key = PrivateKeyDer::from_pem_file(format!("{dir}/privkey.pem"))?;
        Ok(CertifiedKey::from_der(chain, key, &provider)?)
    };

    let mut certificates = ResolvesServerCertUsingSni::new();
    certificates.add("example.com", load("/etc/mi-app/tls/example.com")?)?;
    certificates.add("example.org", load("/etc/mi-app/tls/example.org")?)?;

    let config = rustls::ServerConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(certificates));

    let mut app = App::new();
    app.get("/", |req: Request| async move {
        format!("¡Hola desde {}!", req.hostname().unwrap_or("?"))
    });
    app.tls(Tls::from_rustls(config));
    app.run(443)?;
    Ok(())
}
```

Para recargar los certificados sin reiniciar, escribe tu propio resolver: un tipo que implemente `rustls::server::ResolvesServerCert` y vuelva a leer los archivos cuando cambien.

### En desarrollo

Para probar HTTPS en tu máquina, genera un certificado autofirmado para `localhost`:

- en Rust, con el crate [rcgen](https://crates.io/crates/rcgen), al arrancar (como dependencia de desarrollo o en un ejemplo);
- o con mkcert, que crea un certificado en el que confía tu propia máquina: `mkcert -install` y después `mkcert localhost`, que escribe `localhost.pem` y `localhost-key.pem`, para cargarlos con `Tls::from_pem_files("localhost.pem", "localhost-key.pem")`.

```rust
// Cargo.toml: rcgen = "0.14"
let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
    .map_err(std::io::Error::other)?;
app.tls(Tls::from_pem(
    cert.cert.pem().as_bytes(),
    cert.signing_key.serialize_pem().as_bytes(),
)?);
```

## El puerto 443 y la redirección de HTTP a HTTPS

Los navegadores esperan HTTPS en el puerto **443**, y envían a quien escribe `example.com` al puerto **80**, en HTTP sin cifrar. Sirve la aplicación en el 443, y ejecuta en el puerto 80 una segunda aplicación, diminuta, que redirija todo a HTTPS.

### Redirigir HTTP a HTTPS

```rust
use vitesse::prelude::*;
use vitesse::tls::Tls;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // La aplicación, en HTTPS.
    let mut app = App::new();
    app.middleware(middleware::helmet()); // Strict-Transport-Security, entre otras
    app.get("/", |_| async { "¡Hola en HTTPS!" });
    app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?);

    // Una aplicación diminuta en el puerto 80 que envía a todo el mundo a HTTPS.
    let mut redirect = App::new();
    // Los desafíos de Let's Encrypt (certbot --webroot -w /var/www/acme).
    redirect.static_dir("/.well-known/acme-challenge", "/var/www/acme/.well-known/acme-challenge");
    redirect.fallback(|req: Request| async move {
        let host = req.hostname().unwrap_or("example.com").to_owned();
        let path = req.uri().path_and_query().map_or("/", |pq| pq.as_str());
        Redirect::permanent(format!("https://{host}{path}"))
    });

    // Las dos se detienen juntas con Ctrl+C / SIGTERM; si una no puede arrancar, se devuelve el error.
    tokio::try_join!(app.listen(443), redirect.listen(80))?;
    Ok(())
}
```

Para esto necesitas tokio en tu `Cargo.toml` (`tokio = { version = "1", features = ["full"] }`). `app.listen` ejecuta la aplicación en tu runtime; con `app.bind(...).await?`, obtienes dos `Server` y esperas `https.run()` y `http.run()` juntos de la misma forma.

> [!TIP]
> `listen` no usa el modo de un hilo por núcleo de `app.run`. Para conservarlo en la aplicación principal, arranca en cambio la redirección en su propio hilo: `std::thread::spawn(move || redirect.run(80));`, y después `app.run(443)`.

`helmet()` añade `Strict-Transport-Security`: los navegadores van entonces directamente a HTTPS durante un año, sin pasar por la redirección.

### Puertos por debajo de 1024

En Linux, solo root puede abrir los puertos por debajo de 1024. No ejecutes la aplicación como root: con systemd, añade `AmbientCapabilities=CAP_NET_BIND_SERVICE` a la sección `[Service]` (consulta [Puesta en producción](production.md#ejecutarla-como-servicio-con-systemd)). En un contenedor, también puedes escuchar en puertos sin privilegios, como el 8443 y el 8080, y publicarlos en el 443 y el 80 (`docker run -p 443:8443 -p 80:8080`).

## HTTP/2 sobre TLS: los navegadores

Los navegadores solo usan HTTP/2 sobre TLS, y lo eligen durante el handshake TLS mediante ALPN. Con las features `tls` y `http2`, no hay nada más que configurar: Vitesse ofrece `h2` y `http/1.1`, y cada cliente elige el mejor que conoce.

Lo que HTTP/2 cambia para tu aplicación:

- varias peticiones viajan a la vez por una sola conexión (multiplexación): una página carga sus estilos, scripts e imágenes en paralelo, sin abrir seis conexiones;
- cada petición se atiende en su propia tarea, con los mismos middlewares globales, rutas, middlewares de router y `app.on_error` que en HTTP/1.1;
- `req.version()` es `HTTP/2.0` (`vitesse::http::Version::HTTP_2`), y la cabecera `Host` se reconstruye a partir de la pseudocabecera `:authority`, así que `req.header("host")`, `req.host()` y `req.hostname()` funcionan como en HTTP/1.1;
- los cuerpos de las peticiones llegan al handler como flujo, como en HTTP/1.1, dentro de `app.body_limit` (o del [límite de la ruta](requests.md#límite-de-tamaño-del-cuerpo-y-413));
- las respuestas también se envían como flujo ([Server-Sent Events](responses.md#server-sent-events), archivos, `Body::from_stream`); Vitesse añade `date` y `content-length` (cuando se conoce el tamaño), y quita las cabeceras propias de HTTP/1.1 (`connection`, `transfer-encoding`, `upgrade`, `keep-alive`).

## HTTP/2 sin cifrar (h2c)

Con la feature `http2` y sin TLS, un cliente que abre la conexión directamente en HTTP/2 (con el preámbulo de HTTP/2, «prior knowledge») obtiene HTTP/2, en el mismo puerto que HTTP/1.1. Los navegadores nunca lo hacen, pero algunos clientes y proxies sí:

- los clientes gRPC, y Envoy;
- Caddy con un upstream `h2c://` (`reverse_proxy h2c://127.0.0.1:3000`), Nginx con `grpc_pass`;
- las plataformas que hablan HTTP/2 con el contenedor, como la opción de HTTP/2 de extremo a extremo de Cloud Run;
- `curl --http2-prior-knowledge`.

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |req: Request| async move { format!("{:?}", req.version()) });
    app.run(3000) // HTTP/1.1 y h2c en el mismo puerto (feature `http2`)
}
```

```sh
curl http://localhost:3000/                            # HTTP/1.1
curl --http2-prior-knowledge http://localhost:3000/    # HTTP/2.0
```

La otra forma de empezar HTTP/2 sin cifrar, una petición HTTP/1.1 con `Upgrade: h2c`, no se admite: esas peticiones simplemente se responden en HTTP/1.1, algo que el protocolo permite.

## Lo que te dice la petición

| | `req.version()` | `req.protocol()` | `req.secure()` |
|---|---|---|---|
| HTTP/1.1 | `HTTP/1.1` | `"http"` | `false` |
| HTTPS, HTTP/1.1 | `HTTP/1.1` | `"https"` | `true` |
| HTTPS, HTTP/2 | `HTTP/2.0` | `"https"` | `true` |
| HTTP/2 sin cifrar (h2c) | `HTTP/2.0` | `"http"` | `false` |
| [HTTP/3](http3.md) | `HTTP/3.0` | `"https"` | `true` |

Detrás de un proxy inverso que gestiona el TLS, la conexión entre el proxy y Vitesse es HTTP sin cifrar: declara el proxy con `app.trust_proxy(...)`, y `req.protocol()` y `req.secure()` siguen su cabecera `X-Forwarded-Proto` (consulta [Puesta en producción](production.md#obtener-la-ip-del-cliente)).

## WebSocket sobre TLS

Las rutas [WebSocket](websocket.md) funcionan sobre HTTPS sin ningún cambio: los clientes se conectan con `wss://` en lugar de `ws://`.

```js
const socket = new WebSocket("wss://example.com/chat");
```

Con la feature `http2`, los navegadores que lo admiten (Chrome y Firefox, por ejemplo) abren el WebSocket en un stream de la conexión HTTP/2 que ya tienen con el sitio (RFC 8441), en lugar de una conexión HTTP/1.1 aparte. Tus handlers no cambian: el handshake se enruta como un `GET`, recibe un `200` en lugar de un `101`, y `req.version()` es `HTTP/2.0` en el handler. Sin la feature `http2`, o con un navegador que no lo hace, el WebSocket usa su propia conexión HTTP/1.1 dentro de TLS. Consulta [WebSocket sobre HTTP/2](websocket.md#websocket-sobre-http2).

Cualquier otro cambio de protocolo (una respuesta `101 Switching Protocols` a una petición que no es un handshake WebSocket) es imposible sobre HTTP/2 y se convierte en `505 HTTP Version Not Supported`.

## HTTPS y HTTP/3 juntos

Con la feature `http3` además, un solo binario sirve todos los protocolos. Las respuestas enviadas por HTTPS (HTTP/1.1 y HTTP/2) llevan entonces la cabecera `alt-svc`, y los navegadores pasan a [HTTP/3](http3.md) directamente, sin ningún proxy delante:

```toml
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tls", "http2", "http3"] }
```

```rust
use vitesse::http3::Http3;
use vitesse::prelude::*;
use vitesse::tls::Tls;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |req: Request| async move {
        format!("¡Hola en {:?}!", req.version()) // HTTP/1.1, HTTP/2.0 o HTTP/3.0
    });

    // El mismo certificado para los dos.
    let (cert, key) = ("/etc/mi-app/tls/fullchain.pem", "/etc/mi-app/tls/privkey.pem");
    app.tls(Tls::from_pem_files(cert, key)?); // TCP 443: HTTPS, HTTP/2 y HTTP/1.1
    app.http3(Http3::from_pem_files(cert, key)?); // UDP 443: HTTP/3

    app.run(443)
}
```

Abre el puerto TCP 443 **y** el puerto UDP 443 en el cortafuegos (`sudo ufw allow 443`, que cubre los dos). Consulta [HTTP/3 y QUIC](http3.md) para las opciones.

## Límites y qué no se admite

| HTTP/2 | Valor |
|---|---|
| Peticiones en curso en una conexión | 256 |
| Tamaño de las cabeceras de una petición | 64 KiB |
| Cuerpo que un cliente puede enviar antes de que el handler lo lea (control de flujo) | 256 KiB por petición, 1 MiB por conexión |
| Conexión inactiva | Se cierra pasado el plazo de keep-alive ([`app.keep_alive`](server.md#tiempos-de-espera-y-conexiones), 60 s por defecto) |
| Apagado ordenado | El cliente recibe un `GOAWAY`, y las peticiones en curso disponen del [`shutdown_timeout`](server.md#apagado-ordenado) para terminar |
| Handshake TLS | 10 segundos como máximo |

`app.max_connections` cuenta conexiones, no peticiones: con HTTP/2, una conexión puede llevar 256 peticiones a la vez. Para limitar las propias peticiones, usa [`middleware::concurrency_limit`](middleware.md#concurrency_limit).

No se admite:

- **`Upgrade: h2c`**: la RFC 9113, la especificación actual de HTTP/2, eliminó este mecanismo, y esas peticiones simplemente se responden en HTTP/1.1; usa prior knowledge, o TLS;
- **el server push**: Chrome lo eliminó en 2022 y Firefox en 2024, y axum y actix-web tampoco lo ofrecen.

[WebSocket](websocket.md#websocket-sobre-http2) funciona sobre HTTP/2, con la feature `http2`, además de sobre HTTP/1.1.

## Pruébalo en local

El ejemplo `examples/https.rs` del repositorio genera un certificado autofirmado para `localhost` al arrancar y escucha en el puerto 3443:

```sh
cargo run --example https --features tls,http2

curl -k https://localhost:3443/            # Hello over HTTP/2.0 (https), from 127.0.0.1!
curl -k --http1.1 https://localhost:3443/  # Hello over HTTP/1.1 (https), from 127.0.0.1!
```

`-k` acepta el certificado autofirmado, y `--http1.1` prohíbe HTTP/2. Con `curl -v`, el handshake muestra el protocolo elegido (`ALPN: server accepted h2`). En un navegador, abre `https://localhost:3443/` y acepta el aviso.

## ¿TLS directamente o un proxy inverso?

Las dos opciones funcionan; elige pensando en tu alojamiento.

**Vitesse sirve HTTPS por sí mismo** (esta página):

- un proceso menos, un salto de red menos, y nada más que configurar;
- HTTPS, HTTP/2 y HTTP/3 desde un solo binario, con `alt-svc` enviado directamente a los navegadores;
- `req.ip()` es la dirección real del cliente, sin `trust_proxy`;
- pero los certificados corren de tu cuenta: renovación con certbot y después un reinicio (una fracción de segundo, durante la que fallan las conexiones nuevas), o tu propio resolver de rustls para recargarlos;
- y también la redirección desde HTTP, los puertos privilegiados, y la limitación de peticiones y la compresión con los middlewares de Vitesse (`rate_limit`, `compress`).

**Un proxy inverso (Caddy, Nginx) delante**:

- Caddy obtiene y renueva los certificados automáticamente, sin reiniciar nada;
- una máquina puede alojar varias aplicaciones y dominios detrás de los mismos puertos 80 y 443;
- el proxy puede repartir la carga entre varias instancias y permitir despliegues sin cortes;
- pero es un componente más, y Vitesse necesita [`trust_proxy`](production.md#obtener-la-ip-del-cliente) para conocer la dirección del cliente.

**En una plataforma** (Heroku, Cloud Run, Render, Fly.io…), el TLS se gestiona en el borde de la plataforma: deja la feature `tls` desactivada, y escucha en el `PORT` que te indique.

La configuración con proxy inverso se describe en [Puesta en producción](production.md#detrás-de-un-proxy-inverso).
