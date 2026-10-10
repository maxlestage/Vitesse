# Chuleta de la API

Toda la API pública de Vitesse en una sola página, agrupada por temas: cada línea indica el uso y lo que hace. Para las firmas exactas y todos los detalles, consulta la referencia en [docs.rs/vitesse](https://docs.rs/vitesse).

## Esqueleto

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());

    app.get("/", |_| async { "Hello World!" });
    app.get("/users/:id", |req: Request| async move {
        let id: u64 = req.param_as("id")?;
        Ok::<_, Error>(Json(json!({ "id": id })))
    });

    app.run(3000)
}
```

`use vitesse::prelude::*;` importa `App`, `Router`, `Request`, `Response`, `Next`, `Error`, `Json`, `Html`, `Redirect`, `Cookie`, `Body`, `ServeDir`, `StatusCode`, `Method`, `IntoResponse`, `HandlerExt`, `json!`, el módulo `res`, el módulo `middleware` y el módulo `ws` (WebSocket).

## Features de Cargo

| Feature | Por defecto | Añade |
|---|---|---|
| `ws` | Activada | `app.ws`, `router.ws` y el módulo `vitesse::ws` |
| `http3` | Desactivada | `app.http3`, `server.http3_addr` y el módulo `vitesse::http3` |

```toml
[dependencies]
vitesse = { version = "0.1", features = ["http3"] }        # añade HTTP/3
# vitesse = { version = "0.1", default-features = false } # sin WebSocket
```

Consulta [Instalación](installation.md#features-de-cargo).

## Aplicación

| Uso | Descripción |
|---|---|
| `App::new()` | Una aplicación vacía (`express()`) |
| `app.run(addr)` | Arranca el servidor con su propio runtime; bloquea; se detiene limpiamente con `Ctrl+C` / `SIGTERM` |
| `app.listen(addr).await` | Arranca el servidor en el runtime de tokio actual; se detiene limpiamente con `Ctrl+C` / `SIGTERM` |
| `app.bind(addr).await?` | Abre el puerto y devuelve un `Server`, sin empezar a servir |
| `app.middleware(mw)` | Middleware global (`app.use`): todas las peticiones, en orden, antes del enrutamiento |
| `app.fallback(handler)` | Handler al que se llama cuando ninguna ruta coincide (por defecto: `404 {"error":"Cannot GET /x"}`) |
| `app.on_error(f)` | Personaliza todas las respuestas de error; `f` es una `Fn(Error) -> impl IntoResponse` |
| `app.state(valor)` | Registra un estado global, uno por tipo (`app.locals`) |
| `app.body_limit(bytes)` | Tamaño máximo de un cuerpo leído en memoria (por defecto `DEFAULT_BODY_LIMIT`, 1 MiB) |
| `app.workers(n)` | Número de hilos para `run` (por defecto: uno por CPU) |
| `app.thread_per_core(bool)` | Modo de un hilo por núcleo para `run` (por defecto `true`, solo en Linux) |
| `app.http3(config)` | Sirve también la aplicación en HTTP/3 (QUIC, UDP), junto a HTTP/1.1 (feature `http3`) |

Todos estos ajustes devuelven `&mut App`, así que las llamadas se pueden encadenar. Consulta [Configuración del servidor](server.md).

## Enrutamiento

Estos métodos existen tanto en `App` como en `Router`, y se pueden encadenar.

| Uso | Descripción |
|---|---|
| `app.get(ruta, handler)` | Ruta `GET` (también se usa para `HEAD`) |
| `app.post(…)`, `.put(…)`, `.patch(…)`, `.delete(…)` | Rutas para esos métodos |
| `app.head(ruta, handler)` | Ruta `HEAD` explícita |
| `app.options(ruta, handler)` | Ruta `OPTIONS` explícita (por defecto: `204` con `Allow`) |
| `app.all(ruta, handler)` | Todos los métodos |
| `app.route(método, ruta, handler)` | Cualquier método, p. ej. `Method::from_bytes(b"PURGE").unwrap()` |
| `app.mount(prefijo, router)` | Monta un `Router` bajo un prefijo (`app.use('/api', router)`) |
| `app.static_dir(prefijo, carpeta)` | Sirve una carpeta bajo `prefijo` |
| `app.serve_dir(prefijo, serve_dir)` | Lo mismo, con un `ServeDir` configurado |
| `app.ws(ruta, handler)` | Ruta WebSocket (`GET`): `handler(req, socket)` se hace cargo de la conexión (express-ws) |

| Patrón | Coincide con |
|---|---|
| `/users` | Exactamente `/users` (y `/users/`) |
| `/users/:id` | Un segmento, que se lee con `req.param("id")`; nombres con letras, cifras y `_` |
| `/files/*path` | El resto de la ruta, que se lee con `req.param("path")`; también coincide con `/files` |
| `/*` | Todas las rutas; el parámetro se llama `*` |

Prioridad: estática > parámetro > comodín. Los parámetros se decodifican (`%C3%A9` → `é`), la coincidencia distingue mayúsculas y minúsculas, un método equivocado responde `405` con `Allow`, y una ruta no válida o duplicada provoca un pánico al arrancar. Consulta [Enrutamiento](routing.md).

## Handlers

| Uso | Descripción |
|---|---|
| `async fn show(req: Request) -> impl IntoResponse` | Un handler con nombre |
| `\|req: Request\| async move { … }` | Un closure que usa la petición |
| `\|_\| async { … }` | Un closure que ignora la petición |
| `Ok::<_, Error>(valor)` | Última expresión de un closure que usa `?` |
| `handler.with(mw)` | Añade un middleware a una sola ruta (`HandlerExt`); devuelve un `Chained` |
| `handler.with(a).with(b)` | Varios middlewares de ruta, ejecutados en ese orden |
| `impl Handler for MiTipo` | Handler personalizado: `fn call(&'static self, req: Request) -> BoxFuture<Response>` |

## Petición

| Uso | Descripción |
|---|---|
| `req.method()` | El método HTTP (`&Method`) |
| `req.path()` | La ruta, sin la query string |
| `req.uri()` | La URI completa (`&Uri`) |
| `req.version()` | La versión de HTTP |
| `req.set_uri(uri)` | Reemplaza la URI (reescritura de URL en un middleware global) |
| `req.header(nombre)` | Una cabecera como `Option<&str>`, sin distinguir mayúsculas (`req.get('host')`) |
| `req.header_all(nombre)` | Todos los valores de una cabecera repetida |
| `req.headers()`, `req.headers_mut()` | Todas las cabeceras (`HeaderMap`) |
| `req.content_type()` | El `Content-Type` |
| `req.is("json")` | Comprueba el tipo del cuerpo: tipo completo, subtipo o `text/*` |
| `req.hostname()` | El host solicitado, sin el puerto |
| `req.cookie(nombre)` | El valor de una cookie (`req.cookies.nombre`) |
| `req.param(nombre)` | Un parámetro de ruta, `Option<&str>` |
| `req.param_as::<T>(nombre)?` | Un parámetro convertido a `T`; `400` si falla |
| `req.params()` | Todos los parámetros `(nombre, valor)` |
| `req.query(nombre)` | Un valor decodificado de la query string, `Option<Cow<str>>` |
| `req.query_as::<T>()?` | La query string deserializada en un struct; `400` si falla |
| `req.query_pairs()` | Todos los pares de la query string |
| `req.query_string()` | La query string en bruto |
| `req.json::<T>().await?` | Cuerpo JSON (`express.json()`); `400` si no es válido, `413` si es demasiado grande |
| `req.form::<T>().await?` | Formulario codificado (`express.urlencoded()`) |
| `req.text().await?` | Cuerpo como texto UTF-8; `400` si no es válido |
| `req.bytes().await?` | Cuerpo en bruto (`Bytes`) |
| `req.take_body()` | El cuerpo como flujo (`Body`), sin el límite de `body_limit` |
| `req.set_body(cuerpo)` | Reemplaza el cuerpo |
| `req.state::<T>()` | El estado global (`&'static T`); entra en pánico, y por tanto da `500`, si falta |
| `req.try_state::<T>()` | Lo mismo, como `Option` |
| `req.set(valor)` | Adjunta un dato a la petición (`res.locals`); el tipo debe ser `Clone` |
| `req.get::<T>()` | Lee un dato adjuntado por un middleware |
| `req.extensions()`, `req.extensions_mut()` | Las `http::Extensions` en bruto |
| `req.ip()`, `req.remote_addr()` | La IP del cliente, y la IP con el puerto |

El cuerpo se lee bajo demanda y se guarda en caché: los métodos de lectura se pueden llamar más de una vez. Consulta [Peticiones](requests.md).

## Respuestas

### Lo que puede devolver un handler

| Valor devuelto | Respuesta |
|---|---|
| `&'static str`, `String`, `Cow<'static, str>` | `200`, `text/plain; charset=utf-8` |
| `Json(valor)`, `serde_json::Value` (`json!`) | `200`, `application/json` |
| `Html(cuerpo)` | `200`, `text/html; charset=utf-8` |
| `Bytes`, `Vec<u8>`, `&'static [u8]` | `200`, `application/octet-stream` |
| `()` | `200`, cuerpo vacío |
| `StatusCode::NOT_FOUND` | Ese estado, con su motivo como texto |
| `(estado, valor)` | `valor` con ese estado (`u16`, `i32` o `StatusCode`) |
| `Option<T>` | `T`, o `404` si es `None` |
| `Result<T, E>` | `T` o `E` (los dos deben ser respuestas) |
| `Error` | Su estado y `{"error": "mensaje"}` |
| `Redirect::to(url)` | `302` con `Location` |
| `Body` | `200` con ese cuerpo (p. ej. un flujo) |
| `Response`, `http::Response<Body>` | Tal como se construyó |

### Construir una `Response`

| Uso | Descripción |
|---|---|
| `Response::new()` | Una respuesta `200` vacía |
| `.status(código)` | Estado (`res.status()`) |
| `.header(nombre, valor)` | Define una cabecera reemplazando la existente (se ignoran nombres o valores no válidos) |
| `.append_header(nombre, valor)` | Añade una cabecera sin reemplazar |
| `.content_type(valor)` | `Content-Type` (`res.type()`) |
| `.text(cuerpo)`, `.html(cuerpo)`, `.json(valor)` | Cuerpo con el tipo de contenido correspondiente |
| `.send(cuerpo)` | Cuerpo, sin tocar el tipo de contenido |
| `.cookie(cookie)` | Añade un `Set-Cookie` (`res.cookie()`) |
| `.clear_cookie(nombre)` | Borra una cookie en el navegador (`res.clearCookie()`) |
| `.attachment(nombre_de_archivo)` | Descarga con ese nombre (`res.attachment()`) |
| `res.status_code()`, `res.get_header(nombre)` | Lee el estado, una cabecera |
| `res.set_status(código)`, `res.set_header(nombre, valor)` | Modifica en el sitio (`&mut`) |
| `res.headers()`, `res.headers_mut()` | Todas las cabeceras |
| `res.body()`, `res.body_mut()`, `res.into_body()` | El cuerpo |
| `res.error()`, `res.take_error()` | El `Error` del que procede la respuesta, si lo hay |
| `res.extensions()`, `res.extensions_mut()` | Datos tipados adjuntos a la respuesta |
| `res.into_http()`, `Response::from_http(r)` | Conversión desde y hacia `http::Response<Body>` |

### Los atajos de `res`

| Uso | Descripción |
|---|---|
| `res::status(código)` | Una `Response` con ese estado, para encadenar (`res::status(201).json(v)`) |
| `res::send(cuerpo)`, `res::text(t)`, `res::html(h)`, `res::json(v)` | Una respuesta `200` con ese cuerpo |
| `res::redirect(url)` | Redirección `302` |
| `res::send_status(código)` | El estado y su motivo como texto (`res.sendStatus()`) |
| `res::file(ruta).await` | Envía un archivo con `ETag`, `304` y `Range` (`206`); `404` si no existe (`res.sendFile()`) |
| `res::download(ruta, nombre).await` | Igual, como adjunto (`res.download()`) |

### Redirecciones, cookies y cuerpos

| Uso | Descripción |
|---|---|
| `Redirect::to(url)` | `302 Found` |
| `Redirect::permanent(url)` | `301 Moved Permanently` |
| `Redirect::see_other(url)` | `303 See Other` (después de un POST) |
| `Redirect::temporary(url)` | `307 Temporary Redirect` (conserva el método) |
| `Cookie::new(nombre, valor)` | Una cookie con `Path=/` |
| `.path(p)`, `.domain(d)`, `.max_age(duración)` | Atributos de la cookie |
| `.secure(bool)`, `.http_only(bool)`, `.same_site(SameSite::Lax)` | Atributos de la cookie (`vitesse::SameSite`: `Strict`, `Lax`, `None`) |
| `Body::empty()`, `Body::from(x)` | Cuerpo vacío, o a partir de `&'static str`, `String`, `Vec<u8>`, `Bytes`… |
| `Body::from_stream(flujo)` | Cuerpo enviado como flujo (`chunked`) a partir de un `Stream` de `Result<impl Into<Bytes>, E>` |
| `Body::wrap(cuerpo)` | Envuelve cualquier `http_body::Body` (proxies) |
| `body.size()`, `body.to_bytes().await` | Tamaño si se conoce; lo lee todo en memoria |

Consulta [Respuestas](responses.md).

## Errores

| Uso | Descripción |
|---|---|
| `Error::new(estado, mensaje)` | Un error con un estado y un mensaje para el cliente |
| `Error::from_status(estado)` | El mensaje es el motivo estándar (`Not Found`…) |
| `Error::bad_request(msg)` | `400` |
| `Error::unauthorized(msg)` | `401` |
| `Error::forbidden(msg)` | `403` |
| `Error::not_found(msg)` | `404` |
| `Error::conflict(msg)` | `409` |
| `Error::payload_too_large()` | `413` |
| `Error::unprocessable(msg)` | `422` |
| `Error::internal(msg)` | `500` |
| `.with_source(err)` | Adjunta la causa original (se registra en el log para los errores 5xx) |
| `err.status()`, `err.message()`, `err.source()` | Lee el error |
| `vitesse::Result<T>` | Alias de `Result<T, vitesse::Error>` |
| `?` sobre cualquier error estándar | `500 {"error":"Internal Server Error"}`; la causa solo va al log |
| Pánico en un handler o en un middleware | `500`; el servidor sigue funcionando |

Consulta [Errores](errors.md).

## Middlewares

| Uso | Descripción |
|---|---|
| `async fn mw(req: Request, next: Next) -> Response` | Un middleware como función |
| `\|req: Request, next: Next\| async move { … }` | Un middleware como closure |
| `next.run(req).await` | Continúa la cadena y devuelve la `Response` (`next()`) |
| Devolver una respuesta sin llamar a `next` | Detiene la cadena (autenticación, caché…) |
| `app.middleware(mw)` | Para todas las peticiones |
| `router.middleware(mw)` | Para las rutas de ese router, y las respuestas `404`, `405` y `OPTIONS` bajo su prefijo |
| `handler.with(mw)` | Para una sola ruta |
| `impl Middleware for MiTipo` | `fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response>` |

| Middleware incluido | Equivalente | Notas |
|---|---|---|
| `middleware::logger()` | `morgan('dev')` | Una línea por petición en la salida estándar |
| `middleware::cors()` | `cors()` | Todos los orígenes (`*`) por defecto |
| `.allow_origin("https://…")` | `origin` | Solo esos orígenes (acumulable) |
| `.allow_methods([Method::GET, …])` | `methods` | Por defecto: `GET, HEAD, PUT, PATCH, POST, DELETE` |
| `.allow_headers("…")`, `.expose_headers("…")` | `allowedHeaders`, `exposedHeaders` | Por defecto se permiten las cabeceras que pide el navegador |
| `.allow_credentials(true)` | `credentials` | Combínalo con `allow_origin`: si se permiten todos los orígenes, la respuesta dice `*` y los navegadores rechazan las cookies |
| `.max_age(duración)` | `maxAge` | Tiempo de caché de las peticiones de comprobación previa |
| `middleware::helmet()` | `helmet()` | Cabeceras de seguridad |
| `middleware::timeout(duración)` | `connect-timeout` | `503` si se supera el tiempo |
| `middleware::serve_static(carpeta)` | `express.static()` | Equivale a `ServeDir::new(carpeta)` |

Consulta [Middlewares](middleware.md).

## Routers

| Uso | Descripción |
|---|---|
| `Router::new()` | Un router vacío (`express.Router()`) |
| `router.get(…)`, `.post(…)`, … `.route(…)`, `.ws(…)` | Los mismos métodos de enrutamiento que `App` |
| `router.mount(prefijo, otro)` | Routers anidados |
| `router.static_dir(…)`, `router.serve_dir(…)` | Archivos estáticos dentro de un router |
| `router.middleware(mw)` | Middleware del router: sus rutas y cualquier otra petición bajo su prefijo (`router.use`) |
| `app.mount("/api", router)` | Lo monta; el prefijo puede contener parámetros (`/users/:id/posts`) |

Consulta [Routers](routers.md).

## Archivos estáticos

| Uso | Descripción |
|---|---|
| `ServeDir::new(carpeta)` | Sirve la carpeta `carpeta` |
| `.index(Some("inicio.html"))`, `.index(None)` | Archivo que se sirve para una carpeta (por defecto `index.html`), o ninguno |
| `.max_age(duración)` | Caché del navegador (`Cache-Control: max-age`) |
| `.dotfiles(true)` | Permite los archivos ocultos (rechazados por defecto) |
| `app.static_dir("/assets", "public")` | Como ruta: `GET /assets/*` |
| `app.serve_dir("/assets", ServeDir::new("public").max_age(d))` | Como ruta, con opciones |
| `app.middleware(ServeDir::new("public"))` | Como middleware: si el archivo no existe, la petición sigue hacia las rutas |

Incluido: tipos MIME, `index.html`, `ETag` y `Last-Modified` (`304`), peticiones parciales `Range` (`206`), envío en flujo de archivos grandes, protección contra `../` y los archivos ocultos. Consulta [Archivos estáticos](static-files.md).

## WebSocket

| Uso | Descripción |
|---|---|
| `app.ws("/chat/:room", \|req, mut socket\| async move { … })` | Ruta WebSocket; `426` para HTTP normal, `400` para una clave no válida, `405` para otro método |
| `socket.recv().await` | Siguiente mensaje: `Some(Ok(msg))`, `Some(Err(e))`, o `None` cuando está cerrado |
| `socket.send(valor).await` | `String` / `&str` como texto, `Vec<u8>` / `Bytes` / `&[u8]` como binario, o un `ws::Message` |
| `socket.close(1000, "bye").await` | Cierre con un código y un motivo |
| `socket.protocol()` | El subprotocolo elegido, si lo hay |
| `socket.split()` | `(WebSocketSender, WebSocketReceiver)`, para enviar y recibir desde dos tareas |
| `ws::Message::Text(String)`, `Binary(Bytes)`, `Ping(Bytes)`, `Pong(Bytes)`, `Close(Option<CloseFrame>)` | Los mensajes; los pings se responden automáticamente |
| `msg.as_text()`, `msg.as_bytes()`, `msg.is_close()` | Atajos |
| `err.is_closed()` | La conexión ya está cerrada (`ws::Error`) |
| `ws::Upgrade::new(&req)?` | En una ruta `GET`: comprueba el handshake (`426`, `400`, `405`) |
| `.protocols(["v2", "v1"])`, `.offered_protocols()` | Elige un subprotocolo ofrecido por el cliente; la lista ofrecida |
| `.max_message_size(bytes)` | Tamaño máximo de mensajes y tramas (por defecto `ws::DEFAULT_MAX_MESSAGE_SIZE`, 16 MiB) |
| `.on_upgrade(req, \|req, socket\| async move { … })` | Devuelve la respuesta `101 Switching Protocols` y después ejecuta el handler |

`WebSocket` implementa `Stream` y `Sink`; los middlewares globales y de router se ejecutan sobre el handshake. Consulta [WebSocket](websocket.md).

## Servidor

| Uso | Descripción |
|---|---|
| `3000`, `"3000"` | Escucha en `0.0.0.0:3000` |
| `"127.0.0.1:8080"`, `"[::]:3000"`, `"localhost:3000"` | Escucha en esa dirección |
| `String`, `SocketAddr`, `([127, 0, 0, 1], 8080)` | Otras formas aceptadas (`ListenAddr`) |
| `server.local_addr()` | La dirección que se usa de verdad (puerto `0`) |
| `server.http3_addr()` | La dirección UDP de HTTP/3, si está configurado |
| `server.run().await` | Sirve hasta `Ctrl+C` / `SIGTERM` y luego se detiene limpiamente |
| `server.with_graceful_shutdown(signal).await` | Sirve hasta que termina `signal`, en lugar de `Ctrl+C` / `SIGTERM`; las peticiones en curso tienen 10 s para acabar |

Consulta [Configuración del servidor](server.md).

## HTTP/3

| Uso | Descripción |
|---|---|
| `app.http3(Http3::from_pem_files("fullchain.pem", "privkey.pem")?)` | HTTP/3 por UDP, junto a HTTP/1.1 (`vitesse::http3::Http3`) |
| `Http3::from_pem(cadena, clave)` | Cadena de certificados y clave privada desde la memoria (PEM) |
| `Http3::from_rustls(config)` | Tu propio `rustls::ServerConfig` (TLS 1.3; ALPN `h3` añadido si falta) |
| `.port(443)` | Puerto UDP (por defecto: el mismo número que el puerto TCP) |
| `.alt_svc(false)` | Sin cabecera `alt-svc` en las respuestas HTTP/1.1 (activada por defecto) |
| `.alt_svc_port(443)` | Puerto público anunciado en `alt-svc` (detrás de un proxy, Docker o NAT) |
| `req.version()` | `HTTP/3.0` para una petición recibida por HTTP/3 |

Mismas rutas, middlewares y handlers en los dos protocolos; los cuerpos de las peticiones se leen enteros antes del handler. Consulta [HTTP/3 y QUIC](http3.md).

## Pruebas

| Uso | Descripción |
|---|---|
| `TestClient::new(app)` | Un cliente en memoria (`vitesse::test::TestClient`) |
| `client.get(uri)`, `.post(uri)`, `.put(uri)`, `.patch(uri)`, `.delete(uri)` | Empieza una petición |
| `client.request(Method::HEAD, uri)` | Cualquier método |
| `.header(nombre, valor)`, `.body(datos)` | Cabecera, cuerpo en bruto |
| `.json(&valor)`, `.form(&valor)` | Cuerpo JSON, formulario codificado |
| `.await`, `.send().await` | La envía; devuelve una `TestResponse` |
| `res.status()`, `res.header(nombre)`, `res.headers()` | Estado y cabeceras |
| `res.text()`, `res.bytes()`, `res.json::<T>()` | El cuerpo |
| `client.get("/ruta-ws")` | En una ruta WebSocket: solo la respuesta al handshake (`426` para HTTP normal); prueba las conversaciones en un puerto real |

Consulta [Pruebas](testing.md).

## Reexportaciones

| Elemento | Descripción |
|---|---|
| `vitesse::Bytes` | El tipo `bytes::Bytes` |
| `vitesse::http`, `HeaderMap`, `Method`, `StatusCode`, `header` | El crate `http` y sus tipos habituales |
| `vitesse::serde_json`, `json!` | El crate `serde_json` y su macro |
| `vitesse::tokio` | El crate `tokio`, con las features que usa Vitesse |
| `vitesse::http3::rustls` | El crate `rustls` que usa HTTP/3 (feature `http3`) |
| `vitesse::DEFAULT_BODY_LIMIT` | `1024 * 1024` bytes |
| `Handler`, `Middleware`, `IntoResponse`, `IntoStatus`, `HandlerExt`, `ListenAddr` | Los traits públicos |
| `BoxFuture<T>`, `BoxError`, `Chained`, `Next`, `Server`, `SameSite`, `Cookie`, `Body` | Los demás tipos públicos |
