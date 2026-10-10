# Archivos estáticos

Vitesse sirve los archivos de una carpeta (CSS, JavaScript, imágenes, el build de un front-end…) con todo lo que esperas de `express.static`: tipos MIME, `index.html`, caché del navegador con `ETag` y respuestas `304`, peticiones parciales para vídeo, envío por partes de archivos grandes y protección contra la salida de la carpeta.

## Servir una carpeta bajo un prefijo

```js
app.use('/assets', express.static('public'));
```

```rust
use vitesse::prelude::*;

let mut app = App::new();
app.static_dir("/assets", "public");
```

`public/css/app.css` se sirve ahora en `/assets/css/app.css`, y `public/index.html` en `/assets/`. Internamente, `static_dir` registra dos rutas, `GET` y `HEAD` sobre `/assets/*`. Un archivo que no existe responde `404 {"error":"Cannot GET /assets/nope.css"}`, que pasa por [`app.on_error`](errors.md) como cualquier otro error.

> [!NOTE]
> Una carpeta relativa se resuelve desde la carpeta en la que se lanza el servidor (normalmente la raíz del proyecto con `cargo run`), no desde el archivo fuente. En producción, lanza el binario desde la carpeta correcta o usa una ruta absoluta.

## En la raíz, antes de tus rutas

Para servir archivos en la raíz del sitio, usa `ServeDir` como **middleware**: si el archivo pedido existe, se envía; si no, la petición sigue hacia tus rutas. Es exactamente `app.use(express.static('public'))`:

```rust
app.middleware(ServeDir::new("public"));
app.get("/api/hello", |_| async { "¡Hola!" }); // sigue accesible
```

`middleware::serve_static("public")` es un sinónimo. En este modo solo se tienen en cuenta las peticiones `GET` y `HEAD`, y un archivo inexistente o rechazado no produce ningún error: la petición simplemente continúa.

> [!TIP]
> Como middleware global, `ServeDir` consulta el sistema de archivos en cada petición `GET`, antes del enrutamiento. Para una API con mucho tráfico, mejor usa un prefijo (`app.static_dir("/assets", ...)`), que solo cuesta algo a las peticiones bajo ese prefijo.

`app.static_dir("/", "public")` también funciona, pero con otro equilibrio: declara una ruta comodín `GET /*`. Tus demás rutas siguen ganando (un comodín tiene la prioridad más baja), pero cualquier ruta `GET` desconocida recibe entonces el `404` de los archivos estáticos y nunca llega a `app.fallback`.

## Opciones: `ServeDir`

Para tener más control, configura un `ServeDir` y móntalo con `serve_dir` (o con `app.middleware(...)`):

```rust
use std::time::Duration;

app.serve_dir(
    "/assets",
    ServeDir::new("public")
        .max_age(Duration::from_secs(3600))
        .index(None)
        .dotfiles(false),
);
```

| Opción | Por defecto | Efecto |
|---|---|---|
| `index(Some("inicio.html"))` / `index(None)` | `Some("index.html")` | archivo que se sirve para una carpeta; `None` lo desactiva |
| `max_age(duración)` | `0` | duración de la caché del navegador: `Cache-Control: public, max-age=...` |
| `dotfiles(true)` | `false` | permite archivos y carpetas ocultos (`.well-known`…) |

`static_dir`, `serve_dir` y `ServeDir` también están disponibles en un [`Router`](routers.md), lo que te permite proteger archivos con los middlewares del router:

```rust
let mut private = Router::new();
private.middleware(require_login); // tu middleware de autenticación
private.static_dir("/", "archivos-privados");
app.mount("/private", private); // GET /private/informe.pdf
```

## Carpetas e `index.html`

Una petición a una carpeta sirve su archivo índice: `/assets/docs/` devuelve `public/docs/index.html`. Igual que en Express, una petición sin la barra final (`/assets/docs`) se redirige con un `301` a `/assets/docs/` (conservando la query string), para que los enlaces relativos de la página sigan funcionando. Con `index(None)`, las carpetas no se sirven.

## Caché del navegador: `ETag`, `Last-Modified` y `304`

Cada archivo se envía con:

- `Content-Type`, deducido de la extensión;
- `ETag`, un validador débil calculado a partir del tamaño y la fecha de modificación (`W/"1a2b-65f0c3d1"`);
- `Last-Modified`;
- `Cache-Control: public, max-age=...` (`max-age=0` por defecto: el navegador consulta al servidor antes de reutilizar su copia);
- `Accept-Ranges: bytes`.

Cuando el navegador ya tiene la versión correcta (`If-None-Match` o `If-Modified-Since`), Vitesse responde `304 Not Modified`, sin cuerpo:

```http
GET /assets/app.css HTTP/1.1
If-None-Match: W/"1a2b-65f0c3d1"

HTTP/1.1 304 Not Modified
etag: W/"1a2b-65f0c3d1"
```

> [!TIP]
> Si el build de tu front-end pone un hash en los nombres de archivo (`app.3f9c2b.js`), esos archivos nunca cambian: sírvelos con una caché larga (`max_age(Duration::from_secs(31_536_000))`, un año), y deja `index.html`, que los referencia, con el `max-age=0` por defecto. `max_age` se aplica a todo un `ServeDir`, así que usa dos, uno por carpeta.

## Peticiones parciales

Los reproductores de vídeo y audio, y los gestores de descargas, piden trozos de un archivo con la cabecera `Range`. Vitesse admite un rango por petición:

| Cabecera `Range` | Respuesta |
|---|---|
| `bytes=0-499`, `bytes=500-`, `bytes=-500` (los últimos 500 bytes) | `206 Partial Content` con `Content-Range: bytes 0-499/1234` |
| un rango más allá del final del archivo | `416 Range Not Satisfiable` con `Content-Range: bytes */1234` |
| varios rangos (`bytes=0-1,5-6`) | el archivo completo, `200` |

Para reanudar una descarga, un cliente puede añadir `If-Range`: el rango solo se sirve si `If-Range` es igual a la fecha `Last-Modified` del archivo. Si no, el archivo ha cambiado y se envía completo con un `200`. Los ETag de Vitesse son débiles, así que un ETag en `If-Range` nunca coincide y también recibe el archivo completo.

## Archivos grandes

Los archivos de hasta 256 KiB se leen de una vez; los más grandes se envían por partes de 64 KiB, con su `Content-Length` exacto. Así, el uso de memoria se mantiene constante, incluso con un vídeo de varios gigabytes. Para una petición `HEAD`, solo se envían las cabeceras.

## Tipos MIME

| Extensiones | `Content-Type` |
|---|---|
| `html`, `htm` | `text/html; charset=utf-8` |
| `css` | `text/css; charset=utf-8` |
| `js`, `mjs`, `cjs` | `text/javascript; charset=utf-8` |
| `json`, `map` | `application/json` |
| `txt`, `log`, `md`, `csv` | `text/plain`, `text/markdown`, `text/csv` (`charset=utf-8`) |
| `xml`, `webmanifest` | `application/xml`, `application/manifest+json` |
| `svg`, `png`, `jpg`/`jpeg`, `gif`, `webp`, `avif`, `ico`, `bmp` | `image/...` (`image/svg+xml` para SVG, `image/x-icon` para `ico`) |
| `woff`, `woff2`, `ttf`, `otf` | `font/...` |
| `pdf`, `zip`, `gz`, `tar`, `wasm` | `application/pdf`, `application/zip`, `application/gzip`, `application/x-tar`, `application/wasm` |
| `mp3`, `ogg`, `wav`, `mp4`, `webm` | `audio/mpeg`, `audio/ogg`, `audio/wav`, `video/mp4`, `video/webm` |
| cualquier otra | `application/octet-stream` |

## Seguridad

`ServeDir` solo sirve archivos que están dentro de su carpeta:

- se rechazan los segmentos `..`, las barras invertidas y los bytes nulos, incluso codificados (`%2e%2e%2f`);
- se rechazan los archivos y carpetas ocultos (`.env`, `.git/`, `.htpasswd`…), salvo que actives `dotfiles(true)`;
- solo se sirven `GET` y `HEAD`.

Una ruta rechazada se trata como un archivo inexistente: `404` con `static_dir`, siguiente middleware o ruta con `app.middleware(ServeDir::new(...))`.

> [!WARNING]
> Sirve una carpeta dedicada (`public/`, `dist/`), nunca la raíz del proyecto: tu `Cargo.toml`, tu código fuente o un archivo de configuración quedarían descargables. Los enlaces simbólicos dentro de la carpeta se siguen, así que no pongas en ella enlaces a ubicaciones sensibles.

Vitesse no comprime las respuestas (gzip, brotli). En producción, pon un proxy inverso o una CDN delante del servidor para la compresión y el TLS (ver [Producción](production.md)).

## Enviar un archivo concreto

Para enviar un archivo concreto desde un handler, usa `res::file(ruta).await`, o `res::download(ruta, nombre).await` para forzar una descarga (ver [Responder](responses.md)). Como `res.sendFile` en Express, estos helpers gestionan la caché y los rangos exactamente igual que `ServeDir`: cabeceras `ETag` y `Last-Modified`, `304 Not Modified`, `206 Partial Content` (con `If-Range`) y `416`, como se describe más arriba. `res::download` conserva su cabecera `Content-Disposition: attachment` también en las respuestas parciales. Solo la duración de la caché no se puede configurar: siempre envían `max-age=0`.

## Aplicaciones de una sola página (SPA)

Una aplicación React, Vue o Svelte gestiona sus propias rutas en el navegador: `/users/42/profile` debe devolver `index.html`, mientras que los archivos reales (`/assets/app.js`) y la API funcionan con normalidad. Combina el middleware `ServeDir` con un handler de respaldo:

```rust
let mut api = Router::new();
api.get("/users", |_| async { Json(vec!["ada", "grace"]) });

let mut app = App::new();
app.mount("/api", api);
app.middleware(ServeDir::new("dist"));
app.fallback(|req: Request| async move {
    // Las peticiones a la API y los demás métodos conservan un 404 de verdad.
    if req.method() != Method::GET || req.path().starts_with("/api/") {
        return Error::not_found(format!("Cannot {} {}", req.method(), req.path())).into_response();
    }
    res::file("dist/index.html").await
});
```

Para una petición, el orden es: primero los archivos de `dist/` (el middleware se ejecuta antes del enrutamiento), luego las rutas de la API, y después `index.html` para todo lo demás. Por eso, un archivo llamado `dist/api/...` ocultaría una ruta de la API.
