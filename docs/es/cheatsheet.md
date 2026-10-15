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

`use vitesse::prelude::*;` importa `App`, `Router`, `Request`, `Response`, `Next`, `Error`, `Json`, `Html`, `Redirect`, `Cookie`, `Body`, `ServeDir`, `StatusCode`, `Method`, `IntoResponse`, `HandlerExt`, `json!`, el módulo `res`, el módulo `middleware`, el módulo `sse` (Server-Sent Events) y el módulo `ws` (WebSocket).

## Features de Cargo

| Feature | Por defecto | Añade |
|---|---|---|
| `ws` | Activada | `app.ws`, `router.ws` y el módulo `vitesse::ws` |
| `tls` | Desactivada | `app.tls` y el módulo `vitesse::tls` (HTTPS) |
| `http2` | Desactivada | HTTP/2: sobre TLS (ALPN) con `tls`, y sin cifrar (h2c, prior knowledge) |
| `http3` | Desactivada | `app.http3`, `server.http3_addr` y el módulo `vitesse::http3` |
| `multipart` | Desactivada | `req.multipart()` y el módulo `vitesse::multipart` |
| `compress` | Desactivada | `middleware::compress()` y `middleware::decompress()` |
| `session` | Desactivada | `app.cookie_secret`, las cookies firmadas, `middleware::session` y el módulo `vitesse::session` |
| `templates` | Desactivada | `app.views`, `app.templates`, `req.render` y el módulo `vitesse::templates` (minijinja) |
| `schemars` | Desactivada | `Operation::body_type::<T>()`, `validated_body`, `response_type`, `query_type` y `OpenApi::schema_type` |
| `tower` | Desactivada | `middleware::layer`, `app.into_service()` y el módulo `vitesse::tower` |

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tls", "http2"] } # añade HTTPS y HTTP/2
# vitesse = { git = "https://github.com/maxlestage/Vitesse", default-features = false } # sin WebSocket
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
| `app.on_request(\|req: &mut Request\| …)` | Hook antes de los middlewares y del enrutamiento (`onRequest`); `Err(error)` detiene la petición |
| `app.pre_handler(\|req: &mut Request\| …)` | Hook después del enrutamiento, antes de los middlewares y el handler de la ruta (`preHandler`) |
| `app.on_response(\|res: &mut Response\| …)` | Hook en cada respuesta antes de enviarla (`onSend`) |
| `app.register(plugin)` | Añade un plugin: una `FnOnce(&mut App)` o un `impl Plugin` (el `register` de Fastify) |
| `app.openapi("/openapi.json", OpenApi::new(título, versión))` | Sirve el documento OpenAPI 3.1 de las rutas; `.swagger_ui("/docs")` añade Swagger UI |
| `app.openapi_document(&spec)` | El mismo documento como `serde_json::Value`, sin servirlo |
| `app.state(valor)` | Registra un estado global, uno por tipo (`app.locals`) |
| `app.body_limit(bytes)` | Tamaño máximo de un cuerpo leído en memoria (por defecto `DEFAULT_BODY_LIMIT`, 1 MiB); por ruta: `middleware::body_limit` |
| `app.trust_proxy(valor)` | Proxies inversos de confianza (`app.set('trust proxy')`): `false` (por defecto), `true`, un número de saltos o `"loopback, 10.0.0.0/8"` |
| `app.case_sensitive(false)` | Las rutas coinciden sin importar las mayúsculas, como en Express por defecto (por defecto en Vitesse: `true`) |
| `app.cookie_secret(secreto)` | El secreto que firma las cookies y la cookie de sesión, de 32 bytes como mínimo (`cookieParser(secret)`, feature `session`) |
| `app.views(carpeta)` | Carpeta de las plantillas HTML para `req.render` (`app.set('views')`, feature `templates`) |
| `app.templates(env)` | Tu propio `Environment` de minijinja: filtros, funciones, plantillas integradas (feature `templates`) |
| `app.workers(n)` | Número de hilos para `run` (por defecto: uno por CPU) |
| `app.thread_per_core(bool)` | Modo de un hilo por núcleo para `run` (por defecto `true`, solo en Linux) |
| `app.keep_alive(duración)` | Las conexiones keep-alive inactivas se cierran pasado este tiempo (por defecto 60 s); `Duration::ZERO` desactiva el keep-alive |
| `app.header_timeout(duración)` | Tiempo para enviar una cabecera de petición completa, después `408` (por defecto 30 s) |
| `app.shutdown_timeout(duración)` | Periodo de gracia de las peticiones en curso durante el apagado ordenado (por defecto 10 s) |
| `app.max_connections(n)` | Conexiones abiertas por bucle de aceptación; las demás esperan (por defecto: sin límite) |
| `app.tls(config)` | Sirve HTTPS, y HTTP/2 a los clientes que lo ofrecen (feature `tls`) |
| `app.http3(config)` | Sirve también la aplicación en HTTP/3 (QUIC, UDP), junto a HTTP/1.1 (feature `http3`) |
| `app.into_service()` | La aplicación como `Service` de tower (`vitesse::tower::AppService`, feature `tower`) |
| `app.run_unix(path)`, `app.listen_unix(path).await` | Sirve en un socket de dominio Unix en lugar de en un puerto TCP (solo Unix) |
| `app.bind_unix(path).await?` | Abre el socket Unix y devuelve un `UnixServer` (solo Unix) |

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
| `.name("user")` | Da nombre a la ruta añadida justo antes, para `req.path_for` y `req.url_for`; los nombres son globales en la aplicación, prefijos de montaje incluidos |
| `.doc(Operation::new().summary(…))` | Describe la ruta añadida justo antes en el documento OpenAPI (`vitesse::openapi`) |
| `app.group(prefijo, \|grupo\| { … })` | Un `Router` rellenado por la closure (o una `fn(&mut Router)`) y montado bajo `prefijo` |
| `app.param(nombre, mw)` | Middleware ejecutado antes del handler de cada ruta con el parámetro `nombre` (`app.param` de Express) |
| `app.bind_param(nombre, \|req, valor\| async { … })` | Vinculación de modelos: el `Ok(T)` del cargador está en `req.get::<T>()`, su `Err` es la respuesta |

| Patrón | Coincide con |
|---|---|
| `/users` | Exactamente `/users` (y `/users/`) |
| `/users/:id` | Un segmento, que se lee con `req.param("id")`; nombres con letras, cifras y `_` |
| `/flights/:from-:to` | Varios parámetros en un segmento, separados por texto: `LAX-SFO` da `from=LAX`, `to=SFO` |
| `/files/:name.:ext` | `archive.tar.gz` da `name=archive`, `ext=tar.gz`: el primer parámetro toma lo menos posible |
| `/api/v:version/status` | Texto y un parámetro en un segmento: `/api/v2/status` |
| `r"/users/:id(\d+)"` | Un parámetro restringido por una expresión regular, que tiene que coincidir con el valor entero |
| `/archive/:year?/:month?` | Parámetros opcionales, solo al final: `/archive`, `/archive/2024`, `/archive/2024/05` (`req.param("month")` es `None` cuando falta) |
| `/files/*path` | El resto de la ruta, que se lee con `req.param("path")`; también coincide con `/files` |
| `/*` | Todas las rutas; el parámetro se llama `*` |

Prioridad: estática > segmento con restricción o compuesto > parámetro > comodín, así que `r"/users/:id(\d+)"` y `/users/:name` pueden convivir. Los parámetros se decodifican (`%C3%A9` → `é`), la coincidencia distingue mayúsculas y minúsculas (salvo con `app.case_sensitive(false)`), un método equivocado responde `405` con `Allow`, y una ruta no válida o duplicada, o una expresión regular no válida, provoca un pánico al arrancar. Consulta [Enrutamiento](routing.md#parámetros-de-ruta) y [Rutas con nombre](routing.md#rutas-con-nombre-y-generación-de-url).

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
| `req.version()` | La versión de HTTP: `HTTP/1.1`, `HTTP/2.0`, `HTTP/3.0`… |
| `req.set_uri(uri)` | Reemplaza la URI (reescritura de URL en un middleware global) |
| `req.set_method(método)` | Cambia el método antes del enrutamiento (sobrescritura del método, en un middleware global) |
| `req.params_as::<T>()?` | Todos los parámetros de la ruta, deserializados en una estructura (`Path<T>` de axum); `400` si falla |
| `req.valid_json::<T>().await?`, `req.valid_form::<T>().await?`, `req.valid_query::<T>()?` | Lee y después comprueba las reglas `Validate`: `422` con los mensajes de cada campo |
| `req.route()` | El patrón de la ruta elegida, prefijo de montaje incluido (`Some("/users/:id")`); `None` si ninguna ruta coincide |
| `req.path_for("user", &[&42])?` | La ruta de una ruta con nombre, con los valores codificados: `"/users/42"`; `500` para un nombre desconocido o un valor que falta |
| `req.url_for("user", &[&42])?` | Lo mismo, con el protocolo y el host de la petición: `"https://example.com/users/42"` |
| `req.header(nombre)` | Una cabecera como `Option<&str>`, sin distinguir mayúsculas (`req.get('host')`) |
| `req.header_all(nombre)` | Todos los valores de una cabecera repetida |
| `req.headers()`, `req.headers_mut()` | Todas las cabeceras (`HeaderMap`) |
| `req.content_type()` | El `Content-Type` |
| `req.is("json")` | Comprueba el tipo del cuerpo: tipo completo, subtipo o `text/*` |
| `req.accepts(&["html", "json"])` | El mejor tipo para la cabecera `Accept` (`req.accepts()`); `None`: responde `406` |
| `req.accepts_language(&["en", "fr"])` | El mejor idioma para `Accept-Language` (`req.acceptsLanguages()`) |
| `req.hostname()` | El host solicitado, sin el puerto (`X-Forwarded-Host` detrás de un proxy de confianza) |
| `req.host()` | El host solicitado, con su puerto |
| `req.protocol()` | `"https"` o `"http"` (`X-Forwarded-Proto` detrás de un proxy de confianza) |
| `req.secure()` | `true` sobre TLS, HTTP/3, o con `X-Forwarded-Proto: https` de un proxy de confianza |
| `req.cookie(nombre)` | El valor de una cookie (`req.cookies.nombre`) |
| `req.signed_cookie(nombre)` | El valor de una cookie firmada, `Option<String>`; `None` si falta o se ha manipulado (`req.signedCookies.nombre`, feature `session`) |
| `req.sign_cookie(cookie)` | Firma una cookie antes de `.cookie(...)`; entra en pánico sin `app.cookie_secret` (feature `session`) |
| `req.session()` | La sesión del visitante, `&Session` (`req.session`); `500` sin el middleware `session` |
| `req.render(nombre, contexto)` | Renderiza una plantilla: `Result<Html<String>, Error>` (`res.render()`, feature `templates`) |
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
| `req.multipart()?` | Un cuerpo `multipart/form-data`, campo a campo (feature `multipart`); `415` si no es multipart |
| `req.body_limit()`, `req.set_body_limit(bytes)` | El límite del cuerpo de esta petición; cámbialo en un middleware |
| `req.take_body()` | El cuerpo como flujo (`Body`), sin el límite de `body_limit` |
| `req.set_body(cuerpo)` | Reemplaza el cuerpo |
| `req.state::<T>()` | El estado global (`&'static T`); entra en pánico, y por tanto da `500`, si falta |
| `req.try_state::<T>()` | Lo mismo, como `Option` |
| `req.set(valor)` | Adjunta un dato a la petición (`res.locals`); el tipo debe ser `Clone` |
| `req.get::<T>()` | Lee un dato adjuntado por un middleware |
| `req.extensions()`, `req.extensions_mut()` | Las `http::Extensions` en bruto |
| `req.ip()` | La IP del cliente; sigue a `X-Forwarded-For` detrás de un proxy de confianza (`app.trust_proxy`) |
| `req.remote_addr()` | Dirección y puerto de la propia conexión (`None` en un socket Unix) |

El cuerpo se lee bajo demanda y se guarda en caché: los métodos de lectura se pueden llamar más de una vez. Consulta [Peticiones](requests.md).

### Formularios multipart (feature `multipart`)

| Uso | Descripción |
|---|---|
| `let mut form = req.multipart()?;` | Lee el formulario a medida que llega; todo el cuerpo está limitado por `req.body_limit()` (`413`) |
| `while let Some(field) = form.next_field().await? { … }` | Los campos, en orden; lee (o descarta) uno antes de pedir el siguiente |
| `field.name()` | El nombre del campo (`<input name="avatar">`) |
| `field.file_name()` | El nombre de archivo del cliente, para un archivo: nunca lo uses tal cual como ruta |
| `field.content_type()`, `field.headers()` | El `Content-Type` del campo, todas sus cabeceras |
| `field.chunk().await?` | El siguiente trozo del contenido; `None` al final |
| `field.bytes().await?`, `field.text().await?` | Todo el contenido en memoria, como bytes o como texto UTF-8 |
| `field.save(path).await?` | Escribe el contenido en un archivo como flujo y devuelve su tamaño (`u64`); el archivo parcial se borra si la subida falla |

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
| `cookie.name()`, `cookie.value()`, `cookie.with_value(v)` | Leer una cookie; la misma cookie con otro valor |
| `Body::empty()`, `Body::from(x)` | Cuerpo vacío, o a partir de `&'static str`, `String`, `Vec<u8>`, `Bytes`… |
| `Body::from_stream(flujo)` | Cuerpo enviado como flujo (`chunked`) a partir de un `Stream` de `Result<impl Into<Bytes>, E>` |
| `Body::wrap(cuerpo)` | Envuelve cualquier `http_body::Body` (proxies) |
| `body.size()`, `body.to_bytes().await` | Tamaño si se conoce; lo lee todo en memoria |

### Server-Sent Events (`vitesse::sse`)

| Uso | Descripción |
|---|---|
| `let (tx, stream) = sse::channel(16);` | Devuelve `stream` desde el handler y envía los eventos desde una tarea lanzada con `tokio::spawn` |
| `tx.send(evento).await` | Envía un evento; `Err(Closed)` cuando el navegador se ha ido |
| `tx.try_send(evento)` | Sin esperar: `Err(TrySendError::Full(evento))` si el buffer está lleno, `Err(TrySendError::Closed(evento))` si el navegador se ha ido |
| `tx.is_closed()`, `tx.closed().await` | ¿Se ha ido el navegador? Espera hasta que se vaya; `Sender` es `Clone` |
| `Sse::new(flujo)` | Una respuesta a partir de cualquier `Stream` de `Result<Event, E>` |
| `.keep_alive(KeepAlive::new().interval(d).text("ping"))`, `.no_keep_alive()` | Comentario de keep-alive en una conexión silenciosa (por defecto: cada 15 s) |
| `Event::new().data("…")` | Un evento; los datos de varias líneas se convierten en varias líneas `data:` |
| `.json_data(&valor)?` | Datos serializados en JSON |
| `.event("tick")`, `.id("42")`, `.retry(duración)`, `.comment("…")` | Tipo de evento, id (`Last-Event-ID`), tiempo antes de reconectar, comentario |
| `event.to_string()` | El evento en el formato en que se envía |

La respuesta lleva `content-type: text/event-stream` y `cache-control: no-cache`, y termina cuando se descartan todos los `Sender`. Consulta [Respuestas](responses.md#server-sent-events).

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
| `.with_details(json!({ … }))` | Datos estructurados enviados junto al mensaje: `{"error":"…","details":{…}}`; se leen con `err.details()` |
| `err.status()`, `err.message()`, `err.source()` | Lee el error |
| `vitesse::Result<T>` | Alias de `Result<T, vitesse::Error>` |
| `?` sobre cualquier error estándar | `500 {"error":"Internal Server Error"}`; la causa solo va al log |
| Pánico en un handler o en un middleware | `500`; el servidor sigue funcionando |
| `app.problem_details()` | Todos los errores como `application/problem+json` (RFC 9457), con `instance` y `details` |
| `app.problem_details_with(\|err, problem\| problem.type_uri(…))` | Lo mismo, completado para cada error |
| `Problem::new(403).type_uri(…).title(…).detail(…).extension(k, v)` | Un documento de problema como respuesta (`vitesse::problem`) |

Consulta [Errores](errors.md).

## Validación

| Uso | Descripción |
|---|---|
| `impl Validate for T { fn validate(&self, v: &mut Validator) { … } }` | Las reglas de un tipo (`vitesse::validate`) |
| `v.field("nombre", &self.nombre).not_empty().max_length(50)` | Las reglas de un campo; se detiene en su primera regla que falla |
| `v.optional(nombre, &opt)`, `v.required(nombre, &opt)` | Una `Option`: reglas omitidas si es `None`, o «is required» |
| `.min_length(n)`, `.max_length(n)`, `.not_empty()` | Caracteres de una cadena, elementos de una colección |
| `.min(x)`, `.max(x)`, `.between(a, b)`, `.one_of(&[…])` | Límites y opciones |
| `.email()`, `.url()`, `.alphanumeric()` | Formatos de una cadena |
| `.check(\|v\| bool, "mensaje")`, `.message("…")` | Una regla tuya; reemplaza el mensaje de la regla justo anterior |
| `v.nested("direccion", &self.direccion)`, `v.each("articulos", &self.articulos)` | Valores anidados: `direccion.ciudad`, `articulos[0].nombre` |
| `v.error(nombre, mensaje)` | Un problema informado directamente (reglas sobre varios campos) |

Consulta [Validación](validation.md) y [OpenAPI](openapi.md).

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
| `middleware::body_limit(bytes)` | `express.json({ limit })` | Límite del cuerpo para una ruta (`.with()`) o un router |
| `middleware::request_id()` | `express-request-id` | Id en `X-Request-Id` (el de la petición, o un UUID nuevo), en `req.get::<RequestId>()` y en la respuesta |
| `.header("x-correlation-id")`, `.trust_incoming(false)` | | Otra cabecera; generar siempre un id nuevo |
| `middleware::basic_auth(("admin", "secret"))` | `express-basic-auth` | `401` + `WWW-Authenticate` sin credenciales válidas; también una closure `\|user, password\| -> bool` o una lista de pares |
| `.realm("Admin")` | | El realm que muestra el navegador (por defecto `Restricted`) |
| `req.get::<middleware::BasicUser>()` | | El nombre del usuario autenticado |
| `middleware::bearer_auth(token)` | | `Authorization: Bearer …`; un `&'static str`, un `String`, una lista o una closure `\|token\| -> bool` |
| `middleware::constant_time_eq(a, b)` | | Compara dos secretos en tiempo constante |
| `middleware::concurrency_limit(n)` | | Como máximo `n` peticiones en curso; las demás esperan |
| `.max_wait(duración)` | | `503` + `Retry-After` pasada esta espera (`Duration::ZERO`: de inmediato) |
| `middleware::rate_limit(max, periodo)` | `express-rate-limit` | `max` peticiones por `periodo` y por IP, después `429` + `Retry-After`; cabeceras `X-RateLimit-Limit` y `X-RateLimit-Remaining` |
| `.key(\|req: &Request\| -> Option<String>)` | `keyGenerator` | Otra clave de cliente (clave de API, id de usuario); `None`: sin límite |
| `middleware::session(almacén)` | `express-session` | Sesiones del lado del servidor, que se leen con `req.session()` (feature `session`); consulta [más abajo](#cookies-firmadas-y-sesiones-feature-session) |
| `middleware::compress()` | `compression()` | Respuestas en Brotli o gzip (feature `compress`) |
| `middleware::decompress()` | | Descomprime los cuerpos de petición `gzip`, `br` y `deflate` (feature `compress`) |
| `middleware::layer(capa_tower)` | | Cualquier `Layer` de tower / tower-http (feature `tower`) |
| `middleware::serve_static(carpeta)` | `express.static()` | Equivale a `ServeDir::new(carpeta)` |
| `middleware::etag()` | ajuste `etag` | `ETag` + `304 Not Modified` para las respuestas en memoria |
| `middleware::cache(duración)` | `apicache` | Caché de respuestas en el servidor: `.max_entries(n)`, `.key(…)`, `cache.invalidate(ruta)`; `X-Cache: HIT`/`MISS` |
| `Stack::new().with(a).with(b)` | | Varios middlewares en uno (`vitesse::middleware::Stack`) |
| `app.middleware_group("api", pila)`, `middleware::group("api")` | grupos de Laravel | Una pila con nombre, usada por su nombre |
| `app.on_request_async(\|req\| async { Ok(req) })`, `app.pre_handler_async(…)` | hooks async de Fastify | Hooks que esperan; `Err(error)` detiene la petición |

Consulta [Middlewares](middleware.md).

## Routers

| Uso | Descripción |
|---|---|
| `Router::new()` | Un router vacío (`express.Router()`) |
| `router.get(…)`, `.post(…)`, … `.route(…)`, `.ws(…)` | Los mismos métodos de enrutamiento que `App` |
| `router.mount(prefijo, otro)` | Routers anidados |
| `router.static_dir(…)`, `router.serve_dir(…)` | Archivos estáticos dentro de un router |
| `router.middleware(mw)` | Middleware del router: sus rutas y cualquier otra petición bajo su prefijo (`router.use`) |
| `router.fallback(handler)` | Responde a las rutas bajo el prefijo del router que no coinciden con ninguna ruta, en lugar del fallback de la aplicación |
| `app.mount("/api", router)` | Lo monta; el prefijo puede contener parámetros (`/users/:id/posts`) |
| `router.on_request(…)`, `router.on_response(…)`, `router.on_request_async(…)` | Hooks solo para este router (la encapsulación de Fastify) |
| `static R: TypedRoute<(u32,)> = TypedRoute::new("/users/:id")` | Una ruta tipada: `app.get(R.pattern(), …)`, `R.params(&req)?`, `R.path((42,))` |

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

## Cookies firmadas y sesiones (feature `session`)

| Uso | Descripción |
|---|---|
| `app.cookie_secret(secreto)` | El secreto (`&str`, `String` o bytes), de 32 bytes como mínimo o entra en pánico; léelo de una variable de entorno |
| `.cookie(req.sign_cookie(Cookie::new("theme", "dark")))` | Envía `theme=dark.<firma>`: legible pero no modificable, sin cifrar (`res.cookie(…, { signed: true })`) |
| `req.signed_cookie("theme")` | `Some("dark")`, o `None` si falta o se ha modificado (`req.signedCookies.theme`) |
| `app.middleware(middleware::session(MemoryStore::new()))` | Sesiones del lado del servidor (`express-session`), con el id en la cookie `vitesse.sid` |
| `.cookie_name("sid")`, `.max_age(duración)`, `.rolling(true)` | Nombre de la cookie; duración tras el último cambio (por defecto 1 día); prolongarla en cada petición |
| `.secure(bool)`, `.same_site(SameSite::Strict)`, `.domain(d)`, `.path(p)` | Atributos de la cookie; por defecto `HttpOnly`, `SameSite=Lax`, `Path=/`, y `Secure` sobre HTTPS |
| `req.session().get::<T>(clave)` | Un valor, deserializado: `Option<T>` |
| `session.insert(clave, valor)?` | Guarda cualquier valor `Serialize` |
| `session.remove(clave)`, `session.contains(clave)` | Quita un valor y lo devuelve; comprueba una clave |
| `session.clear()`, `session.is_empty()` | Quita todos los valores; comprueba si no hay ninguno |
| `session.regenerate()` | Un id nuevo, los mismos datos: llámalo al iniciar sesión |
| `session.destroy()` | Borra la sesión, en el almacén y en el navegador: cierre de sesión |
| `session.id()` | El id, `None` hasta que la sesión se guarda por primera vez |
| `vitesse::session::MemoryStore` | Sesiones en la memoria del proceso: se pierden al reiniciar, no se comparten entre instancias |
| `impl SessionStore for MyStore` | `load(id)`, `save(id, &record, ttl)` y `delete(id)`, que devuelven cada uno un `BoxFuture`; un `Record` es un `serde_json::Map` |

Una sesión se guarda, y su cookie se envía, solo cuando un handler la modifica (o en cada petición con `rolling(true)`); la cookie se firma cuando la aplicación tiene un `cookie_secret`. Consulta [Cookies firmadas y sesiones](sessions.md).

## Plantillas HTML (feature `templates`)

| Uso | Descripción |
|---|---|
| `app.views("templates")` | La carpeta de las plantillas (`app.set('views')`); cada plantilla se lee la primera vez que se usa, y después se guarda en caché |
| `req.render("users.html", json!({ … }))` | Renderiza una plantilla (`res.render()`): `Result<Html<String>, Error>`; cualquier valor `Serialize` como contexto |
| `app.templates(env)` | Tu propio `vitesse::templates::minijinja::Environment` en lugar de `app.views` |
| `env.add_filter(nombre, f)`, `env.add_function(nombre, f)`, `env.add_global(nombre, valor)` | Filtros, funciones y variables propios |
| `env.add_template(nombre, include_str!("../templates/x.html"))` | Una plantilla integrada en el binario |
| `{{ value }}`, `{{ value\|upper }}` | Un valor, escapado en las plantillas `.html`, `.htm` y `.xml`; un filtro |
| `{% for x in list %}…{% else %}…{% endfor %}`, `{% if %}…{% elif %}…{% endif %}` | Bucles y condiciones |
| `{% extends "layout.html" %}`, `{% block content %}…{% endblock %}` | Plantillas base |
| `{% include "partials/nav.html" %}`, `{% macro %}`, `{% import %}` | Fragmentos y macros |
| `{{ html\|safe }}` | HTML de confianza, sin escapar |

Una plantilla que falta o un error de renderizado dan un `500` (la causa va al log); reinicia el servidor para ver un cambio en una plantilla. Consulta [Plantillas HTML](templates.md).

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

`WebSocket` implementa `Stream` y `Sink`; los middlewares globales y de router se ejecutan sobre el handshake. Los WebSockets funcionan sobre HTTP/1.1 y, con la feature `http2`, sobre HTTP/2 (RFC 8441), pero no sobre HTTP/3. Consulta [WebSocket](websocket.md).

## Servidor

| Uso | Descripción |
|---|---|
| `3000`, `"3000"` | Escucha en `0.0.0.0:3000` |
| `"127.0.0.1:8080"`, `"[::]:3000"`, `"localhost:3000"` | Escucha en esa dirección |
| `String`, `SocketAddr`, `([127, 0, 0, 1], 8080)` | Otras formas aceptadas (`ListenAddr`) |
| `server.local_addr()` | La dirección que se usa de verdad (puerto `0`) |
| `server.http3_addr()` | La dirección UDP de HTTP/3, si está configurado |
| `server.run().await` | Sirve hasta `Ctrl+C` / `SIGTERM` y luego se detiene limpiamente |
| `server.with_graceful_shutdown(signal).await` | Sirve hasta que termina `signal`, en lugar de `Ctrl+C` / `SIGTERM`; las peticiones en curso tienen `shutdown_timeout` (10 s) para acabar |
| `unix_server.path()` | El archivo del socket (`UnixServer`), por ejemplo para cambiar sus permisos |
| `unix_server.run().await`, `unix_server.with_graceful_shutdown(signal).await` | Igual que `Server`; el archivo del socket se borra cuando se detiene |

Consulta [Configuración del servidor](server.md).

## HTTPS y HTTP/2

| Uso | Descripción |
|---|---|
| `app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?)` | HTTPS, TLS 1.2 y 1.3 (`vitesse::tls::Tls`, feature `tls`) |
| `Tls::from_pem(cadena, clave)` | Cadena de certificados y clave privada desde la memoria (PEM) |
| `Tls::from_rustls(config)` | Tu propio `rustls::ServerConfig` (TLS mutuo, varios dominios…); ALPN `h2` y `http/1.1` añadidos si no indica ninguno |
| `tls.config()` | La configuración de rustls (`&Arc<rustls::ServerConfig>`) |
| Feature `http2` | HTTP/2 sobre TLS (navegadores, ALPN), y h2c con prior knowledge en un puerto sin cifrar |
| `req.version() == Version::HTTP_2` | Una petición recibida por HTTP/2 (`vitesse::http::Version`) |

Mismas rutas, middlewares y handlers en todos los protocolos. Con la feature `ws`, los navegadores también abren WebSockets sobre HTTP/2 (RFC 8441), en la misma conexión que la página; los handlers de `app.ws` no cambian. Consulta [HTTPS y HTTP/2](https.md).

## Proxies de confianza

| Uso | Descripción |
|---|---|
| `app.trust_proxy(false)` | Nadie (por defecto): `req.ip()` es la dirección de la conexión |
| `app.trust_proxy("loopback")` | Los proxies en estas direcciones o redes: IP, CIDR (`10.0.0.0/8`), `loopback`, `linklocal`, `uniquelocal`, separados por comas |
| `app.trust_proxy(1)` | Los `n` proxies más cercanos a la aplicación, sea cual sea su dirección |
| `app.trust_proxy(true)` | Todos los proxies: el cliente es la dirección más a la izquierda de `X-Forwarded-For` |
| `vitesse::TrustProxy` | `None`, `All`, `Hops(n)`, `Networks(Vec<Network>)`; `"…".parse::<TrustProxy>()` para gestionar los errores |
| `trust.trusts(ip, hop)`, `network.contains(ip)` | Comprueba una dirección (`vitesse::Network`: `"10.0.0.0/8".parse()`) |

Efectos: `req.ip()` (`X-Forwarded-For`), `req.protocol()` y `req.secure()` (`X-Forwarded-Proto`), `req.host()` y `req.hostname()` (`X-Forwarded-Host`). Consulta [Puesta en producción](production.md#obtener-la-ip-del-cliente).

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

Mismas rutas, middlewares y handlers en los dos protocolos; los cuerpos de las peticiones llegan al handler por partes, con los mismos límites de cuerpo. Con `app.tls(...)` además, las respuestas HTTPS llevan `alt-svc` y los navegadores pasan a HTTP/3 directamente. Consulta [HTTP/3 y QUIC](http3.md).

## tower (feature `tower`)

| Uso | Descripción |
|---|---|
| `app.middleware(middleware::layer(CorsLayer::permissive()))` | Una capa (`Layer`) de tower como middleware: también con `router.middleware` y `handler.with` |
| `vitesse::tower::layer(l)` | Igual que `middleware::layer` |
| `app.into_service()` | La aplicación como `Service<http::Request<B>>` (`AppService`, `Clone` + `Copy`): hyper, `Router::nest_service` de axum, `ServiceExt::oneshot` |
| `vitesse::tower::{Layer, Service}` | Los traits de tower |
| `vitesse::tower::NextService` | El resto de la cadena de Vitesse, el servicio más interno de una capa |

La capa puede cambiar el método, la URI, las cabeceras, las extensiones y el cuerpo; sus errores se convierten en `500`. Los upgrades a WebSocket no están disponibles a través de `into_service` (`501`). Consulta [Middlewares](middleware.md#capas-de-tower).

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
| `.cookie(nombre, valor)`, `.bearer(token)`, `.query(&parámetros)` | Una cookie, `Authorization: Bearer`, una query string |
| `client.agent()` | Un cliente con tarro de cookies (sesiones, inicios de sesión): `agent.cookie(nombre)`, `agent.clear_cookies()` |
| `res.assert_status(201).assert_json_path("user.name", "Ada")` | Aserciones encadenadas: `assert_ok`, `assert_header`, `assert_see`, `assert_json`, `assert_json_includes`, `assert_validation_errors`, `assert_cookie`, `assert_redirect`… |
| `app.into_service().oneshot(request).await` | Pruebas a través de tower (feature `tower` y `ServiceExt` de tower) |

Consulta [Pruebas](testing.md).

## PubSub, cola de tareas y planificador

| Uso | Descripción |
|---|---|
| `PubSub::<String>::new()` | Temas compartidos por sus clones (`vitesse::pubsub`); ponlo en `app.state` |
| `pubsub.subscribe(tema)`, `sub.recv().await` | Se suscribe, lee el siguiente mensaje |
| `pubsub.publish(tema, msg)`, `sub.publish(msg)` | A todos los suscriptores; solo a los demás |
| `pubsub.relay(socket, tema).await` | Un WebSocket entra en una sala (feature `ws`) |
| `sub.sse(\|msg\| Event::new().data(msg))` | Una suscripción como Server-Sent Events |
| `Queue::builder().workers(4).retries(3).rate_limit(10, periodo).start()` | Una cola de tareas (`vitesse::queue`) |
| `queue.dispatch(\|\| async { … })`, `queue.dispatch_after(retraso, tarea)`, `queue.idle().await` | Añade tareas; espera a que no quede ninguna |
| `Scheduler::new().every(d, nombre, tarea).daily_at(3, 30, nombre, tarea).start()` | Tareas programadas (`vitesse::schedule`), `.stop()` en el manejador |

Consulta [PubSub y canales](pubsub.md) y [Tareas en segundo plano y programadas](background.md).

## Reexportaciones

| Elemento | Descripción |
|---|---|
| `vitesse::Bytes` | El tipo `bytes::Bytes` |
| `vitesse::http`, `HeaderMap`, `Method`, `StatusCode`, `header` | El crate `http` y sus tipos habituales |
| `vitesse::serde_json`, `json!` | El crate `serde_json` y su macro |
| `vitesse::tokio` | El crate `tokio`, con las features que usa Vitesse |
| `vitesse::http3::rustls`, `vitesse::tls::rustls` | El crate `rustls` que usan HTTP/3 y HTTPS (features `http3` y `tls`) |
| `vitesse::templates::minijinja` | El crate `minijinja` (feature `templates`) |
| `vitesse::session::{Session, Sessions, SessionStore, MemoryStore, Record}` | Los tipos de las sesiones (feature `session`) |
| `vitesse::DEFAULT_BODY_LIMIT` | `1024 * 1024` bytes |
| `vitesse::VERSION` | La versión del crate (`"0.2.0"`) |
| `Handler`, `Middleware`, `IntoResponse`, `IntoStatus`, `HandlerExt`, `ListenAddr` | Los traits públicos |
| `BoxFuture<T>`, `BoxError`, `Chained`, `Next`, `Server`, `UnixServer`, `SameSite`, `Cookie`, `Body`, `TrustProxy`, `Network` | Los demás tipos públicos |
