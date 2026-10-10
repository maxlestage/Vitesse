# HTTP/3 y QUIC

HTTP/3 lleva HTTP sobre QUIC, un transporte construido sobre UDP con TLS 1.3 integrado: las conexiones se abren más rápido, un paquete perdido solo retrasa la petición a la que pertenece, y una conexión sobrevive a un cambio de red (de la wifi a los datos móviles, por ejemplo). Vitesse puede servir tu aplicación en HTTP/3 junto a HTTP/1.1: las mismas rutas, los mismos middlewares y los mismos handlers, con una línea más de configuración.

## Activar la feature `http3`

HTTP/3 es una feature de Cargo que se activa explícitamente:

```toml
[dependencies]
vitesse = { version = "0.1", features = ["http3"] }
```

Incorpora [quinn](https://github.com/quinn-rs/quinn) (QUIC), [h3](https://github.com/hyperium/h3) (HTTP/3) y [rustls](https://github.com/rustls/rustls) (TLS 1.3, con la biblioteca criptográfica ring): todo en Rust, sin OpenSSL que instalar. La feature está desactivada por defecto porque añade un tiempo de compilación que la mayoría de las aplicaciones detrás de un proxy no necesitan.

## Un ejemplo completo

```rust
use vitesse::http3::Http3;
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());
    app.get("/", |req: Request| async move {
        format!("¡Hola en {:?}!", req.version()) // HTTP/1.1 o HTTP/3.0
    });

    // QUIC siempre usa TLS: un certificado y su clave privada, en PEM.
    app.http3(Http3::from_pem_files("fullchain.pem", "privkey.pem")?);

    // HTTP/1.1 en el puerto TCP 443, HTTP/3 en el puerto UDP 443.
    app.run(443)
}
```

`app.http3(...)` añade un socket UDP junto al socket TCP. Cada petición, sea cual sea su protocolo, pasa por los mismos middlewares globales, las mismas rutas, los mismos middlewares de router y el mismo `app.on_error`. En un handler, `req.version()` es `HTTP/3.0` para una petición que llegó por HTTP/3, y la cabecera `Host` se reconstruye a partir de la pseudocabecera `:authority`, así que `req.header("host")` y `req.hostname()` funcionan como en HTTP/1.1.

HTTP/3 funciona con las tres formas de arrancar el servidor: `app.run(addr)`, `app.listen(addr).await` y `app.bind(addr).await?` (ver [más abajo](#bind-y-http3_addr)). El socket UDP escucha en la misma dirección IP que el TCP.

## Certificados

QUIC no funciona sin TLS, así que Vitesse necesita un certificado para tu dominio y su clave privada. Tres constructores:

| Constructor | Uso |
|---|---|
| `Http3::from_pem_files(cadena, clave)` | Dos archivos PEM, por ejemplo `fullchain.pem` y `privkey.pem` de Let's Encrypt |
| `Http3::from_pem(bytes_cadena, bytes_clave)` | Lo mismo, desde la memoria (un gestor de secretos, una variable de entorno…) |
| `Http3::from_rustls(config)` | Tu propio `rustls::ServerConfig` |

La cadena contiene primero el certificado del servidor y después los certificados intermedios: justo lo que guarda `fullchain.pem`. La clave privada puede estar en formato PKCS#8, SEC1 (EC) o PKCS#1 (RSA), en PEM. Si un archivo no se puede leer o analizar, `from_pem_files` devuelve un `io::Error` antes de que arranque el servidor.

> [!IMPORTANT]
> El certificado se carga una sola vez, al arrancar. Let's Encrypt renueva los certificados más o menos cada dos meses: reinicia la aplicación tras cada renovación, por ejemplo desde un hook de certbot (`certbot renew --deploy-hook "systemctl restart mi-app"`).

### Tu propia configuración de rustls

Para certificados de cliente (TLS mutuo), varios dominios en un mismo servidor o recargar los certificados sin reiniciar, construye tú mismo la configuración de rustls. `vitesse::http3::rustls` reexporta la versión de rustls que usa Vitesse, así que los tipos siempre coinciden:

```rust
use std::sync::Arc;

use vitesse::http3::rustls::pki_types::pem::PemObject;
use vitesse::http3::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use vitesse::http3::{Http3, rustls};
use vitesse::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let chain = CertificateDer::pem_file_iter("fullchain.pem")?.collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_file("privkey.pem")?;

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])? // QUIC exige TLS 1.3
        .with_no_client_auth()
        .with_single_cert(chain, key)?;

    let mut app = App::new();
    app.get("/", |_| async { "¡Hola!" });
    app.http3(Http3::from_rustls(tls)); // el protocolo ALPN "h3" se añade si falta
    app.run(443)?;
    Ok(())
}
```

### En desarrollo

Para probar HTTP/3 en tu máquina, genera un certificado autofirmado para `localhost`:

- en Rust, con el crate [rcgen](https://crates.io/crates/rcgen), al arrancar (como dependencia de desarrollo o en un ejemplo);
- o con [mkcert](https://github.com/FiloSottile/mkcert), que crea un certificado en el que confía tu propia máquina: `mkcert -install` y después `mkcert localhost`, que escribe `localhost.pem` y `localhost-key.pem`.

```rust
// Cargo.toml: rcgen = "0.14"
let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
    .map_err(std::io::Error::other)?;
app.http3(Http3::from_pem(
    cert.cert.pem().as_bytes(),
    cert.signing_key.serialize_pem().as_bytes(),
)?);
```

El ejemplo [`examples/http3.rs`](https://github.com/maxlestage/Vitesse/blob/master/examples/http3.rs) del repositorio hace exactamente esto, y escucha en el puerto 4433. Pruébalo con un curl compilado con soporte de HTTP/3 (`curl --version` incluye `HTTP3` entre sus funcionalidades):

```sh
cargo run --example http3 --features http3

curl -k --http3-only https://localhost:4433/   # Hello over HTTP/3.0!
curl -i http://localhost:4433/                 # HTTP/1.1, con la cabecera alt-svc
```

`-k` acepta el certificado autofirmado.

## Opciones

| Método de `Http3` | Por defecto | Efecto |
|---|---|---|
| `.port(u16)` | El mismo número que el puerto TCP | El puerto UDP en el que escuchar |
| `.alt_svc(bool)` | `true` | Añade la cabecera `alt-svc` a las respuestas HTTP/1.1 |
| `.alt_svc_port(u16)` | El puerto UDP que realmente se abre | El puerto público anunciado en `alt-svc`, cuando los clientes llegan al servidor por otro puerto (detrás de un proxy, Docker o NAT: normalmente `443`) |

```rust
app.http3(
    Http3::from_pem_files("fullchain.pem", "privkey.pem")?
        .port(8443)          // Vitesse escucha en UDP 8443...
        .alt_svc_port(443),  // ...al que los clientes llegan por el puerto 443 (NAT, Docker)
);
```

## Cómo pasan los navegadores a HTTP/3

Un navegador nunca empieza con HTTP/3. Primero se conecta por TCP con HTTPS, y pasa a HTTP/3 para las peticiones siguientes si la respuesta le indica que el servicio existe, con la cabecera `Alt-Svc`. Vitesse añade esta cabecera a cada respuesta HTTP/1.1:

```http
alt-svc: h3=":443"; ma=86400
```

Así, el navegador recuerda durante un día (`ma=86400`) que este sitio responde en HTTP/3 en el puerto UDP 443.

Los navegadores solo confían en `Alt-Svc` en un sitio HTTPS. Vitesse no hace TLS sobre TCP, así que un navegador que llega directamente a él en HTTP sin cifrar nunca cambia de protocolo. En producción, la organización es por tanto:

1. un proxy inverso TLS (Caddy, Nginx) en el puerto TCP 443, que reenvía las peticiones a Vitesse en HTTP/1.1;
2. Vitesse directamente en el puerto UDP 443, con el mismo certificado que el proxy;
3. `alt-svc` anunciando el puerto `443`: el proxy pasa la cabecera a los navegadores, que después hablan directamente con Vitesse en HTTP/3.

> [!TIP]
> Caddy sirve HTTP/3 por defecto, con sus propios certificados. Si Caddy es tu proxy, lo más sencillo es dejar que haga HTTP/3 él mismo y no activar la feature. Actívala cuando tu proxy no haga HTTP/3, o cuando quieras que Vitesse responda a los clientes HTTP/3 sin pasar por el proxy.

Los clientes fuera del navegador (`curl --http3`, apps móviles, otros servicios) no necesitan `Alt-Svc`: pueden usar HTTP/3 directamente si saben que el servidor lo admite.

## En producción detrás de Caddy o Nginx

Vitesse sirve HTTP/1.1 al proxy en un puerto TCP privado, y HTTP/3 a internet en el puerto UDP 443:

```rust
use vitesse::http3::Http3;
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());
    app.get("/", |_| async { "¡Hola!" });

    // El mismo certificado que el proxy (copias que el usuario del servicio pueda leer).
    app.http3(
        Http3::from_pem_files("/etc/mi-app/tls/fullchain.pem", "/etc/mi-app/tls/privkey.pem")?
            .port(443),         // HTTP/3: UDP 443, directamente desde internet
    );

    // HTTP/1.1 para el proxy en TCP 3000. El socket UDP usa la misma IP,
    // así que escucha en 0.0.0.0 y mantén TCP 3000 cerrado en el cortafuegos.
    app.run("0.0.0.0:3000")
}
```

`alt-svc` anuncia el puerto UDP que realmente está abierto, aquí `443`. Si los clientes llegan a Vitesse a través de una traducción de puertos (Docker `-p 443:8443/udp`, un NAT, una redirección del cortafuegos), escucha en el puerto interno con `.port(8443)` y anuncia el público con `.alt_svc_port(443)`.

En el lado del proxy, mantén la configuración habitual (ver [Puesta en producción](production.md#detrás-de-un-proxy-inverso)):

- **Nginx**: TLS en TCP 443 y `proxy_pass` hacia `127.0.0.1:3000`, sin `listen 443 quic`, para que el puerto UDP 443 quede libre para Vitesse. Nginx pasa a los navegadores la cabecera `alt-svc` de Vitesse.
- **Caddy**: desactiva su propio HTTP/3 para liberar el puerto UDP 443, en las opciones globales al principio del `Caddyfile`:

  ```text
  {
  	servers {
  		protocols h1 h2
  	}
  }
  ```

Con systemd, un usuario sin privilegios no puede abrir el puerto 443: añade `AmbientCapabilities=CAP_NET_BIND_SERVICE` a la sección `[Service]`. Las claves de Let's Encrypt solo las puede leer root: cópialas a una carpeta que el usuario del servicio pueda leer (desde el hook de certbot que también reinicia la aplicación).

> [!WARNING]
> Las peticiones HTTP/3 no pasan por el proxy. `req.ip()` es entonces la dirección real del cliente, pero las cabeceras `X-Real-IP` y `X-Forwarded-For` vienen directamente del cliente: confía en ellas solo en las peticiones HTTP/1.1 (`req.version()`). Del mismo modo, lo que hace el proxy (compresión, limitación de peticiones, registros de acceso, tamaño máximo de los cuerpos) no se aplica a las peticiones HTTP/3: `app.body_limit` sí.

## `bind` y `http3_addr`

Con `app.bind`, `server.http3_addr()` devuelve la dirección UDP (`None` sin `app.http3`), algo útil con el puerto `0`:

```rust
use vitesse::http3::Http3;
use vitesse::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "ok" });
    app.http3(Http3::from_pem_files("cert.pem", "key.pem")?);

    let server = app.bind("127.0.0.1:0").await?;
    println!("HTTP/1.1 en tcp://{}", server.local_addr());
    if let Some(udp) = server.http3_addr() {
        println!("HTTP/3 en udp://{udp}"); // el mismo número de puerto que en TCP
    }

    server.run().await
}
```

`server.run()` y `server.with_graceful_shutdown(signal)`, igual que `app.run` y `app.listen`, detienen los dos protocolos a la vez: al apagar, las conexiones HTTP/3 reciben un `GOAWAY` (el cliente ya no abre peticiones nuevas en ellas) y las peticiones en curso tienen hasta 10 segundos para terminar, como en HTTP/1.1 (ver [Configuración del servidor](server.md#apagado-ordenado)).

## Qué cambia y qué no se admite (todavía)

- **Los cuerpos de las peticiones** se leen enteros antes de ejecutar el handler, hasta `app.body_limit` (`413` si se supera). Por eso `req.take_body()` da un cuerpo que ya está en memoria, no un flujo: leer por partes los cuerpos de las peticiones en HTTP/3 todavía no se admite.
- **Las respuestas** se envían por partes como en HTTP/1.1 (`Body::from_stream`, archivos…). Vitesse añade `date` y `content-length` (cuando se conoce el tamaño), y quita las cabeceras propias de HTTP/1.1 (`connection`, `transfer-encoding`, `upgrade`, `keep-alive`).
- **Los hilos**: con `app.run` en modo de un hilo por núcleo (el predeterminado en Linux), HTTP/3 lo sirve un solo hilo, junto a los hilos de HTTP/1.1. Si la mayor parte de tu tráfico es HTTP/3, compara con `app.thread_per_core(false)`.
- **No se admite**: [WebSocket](websocket.md) sobre HTTP/3 (los clientes abren sus conexiones WebSocket en HTTP/1.1), HTTP/2, 0-RTT (*early data*) ni la lectura por partes de los cuerpos de las peticiones.

## Cortafuegos y alojamiento

QUIC usa **UDP**: abrir el puerto TCP 443 no basta.

- **Cortafuegos**: abre el puerto UDP, por ejemplo con `sudo ufw allow 443/udp`, o con una regla UDP en el grupo de seguridad de tu proveedor cloud.
- **Docker**: declara el puerto con `EXPOSE 443/udp` y publícalo con `-p 443:443/udp` (ver [Docker](docker.md#http3-publicar-el-puerto-udp)).
- **Heroku**: su router no reenvía UDP a los dynos, así que HTTP/3 no está disponible en Heroku. Deja la feature desactivada allí.
- **Cloud Run, Render y plataformas similares** terminan HTTP/3 en su borde, cuando lo ofrecen, y hablan HTTP/1.1 con tu contenedor: tampoco necesitas la feature allí.

Si HTTP/3 está bloqueado en algún punto del camino (un cortafuegos corporativo que descarta el UDP, por ejemplo), los navegadores vuelven sin avisar a HTTP/1.1 o HTTP/2 a través del proxy: activar HTTP/3 nunca deja a nadie sin servicio.
