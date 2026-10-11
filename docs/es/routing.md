# Enrutamiento

El enrutamiento elige qué handler responde a una petición según su método y su ruta. Vitesse usa el vocabulario de Express (`app.get`, `:id`, `app.all`), pero su enrutador es un árbol: el orden en que declaras las rutas no importa, y un método incorrecto recibe un `405` como corresponde.

## Declarar rutas

Cada método HTTP tiene su propia función. Existen en `App` y en [`Router`](routers.md):

| Vitesse | Express | Métodos aceptados |
|---|---|---|
| `app.get(ruta, handler)` | `app.get()` | `GET`, y también `HEAD` (ver más abajo) |
| `app.post(ruta, handler)` | `app.post()` | `POST` |
| `app.put(ruta, handler)` | `app.put()` | `PUT` |
| `app.patch(ruta, handler)` | `app.patch()` | `PATCH` |
| `app.delete(ruta, handler)` | `app.delete()` | `DELETE` |
| `app.head(ruta, handler)` | `app.head()` | `HEAD` |
| `app.options(ruta, handler)` | `app.options()` | `OPTIONS` |
| `app.all(ruta, handler)` | `app.all()` | todos los métodos |
| `app.route(método, ruta, handler)` | `app[método]()` | el método que pases, incluso uno no estándar |

```rust
app.get("/articles", |_| async { "lista de artículos" });
app.post("/articles", |_| async { (201, "artículo creado") });
app.put("/articles/:id", |_| async { "artículo reemplazado" });
app.patch("/articles/:id", |_| async { "artículo modificado" });
app.delete("/articles/:id", |_| async { StatusCode::NO_CONTENT });
app.all("/ping", |_| async { "pong" });

// Un método no estándar
let purge = Method::from_bytes(b"PURGE").unwrap();
app.route(purge, "/cache", |_| async { "caché vaciada" });
```

> [!WARNING]
> `app.route` no es el `app.route(ruta)` de Express. En Vitesse registra una sola ruta para el `Method` que le pasas. Para poner varios métodos en la misma ruta, encadena las funciones (consulta [Encadenar llamadas](#encadenar-llamadas)).

> [!TIP]
> Los formularios HTML solo pueden enviar `GET` y `POST`. Para llegar a una ruta `PUT` o `DELETE` desde un formulario, cambia el método en un middleware global con `req.set_method(...)`, como `method-override` en Express: consulta [Sobrescribir el método](requests.md#sobrescribir-el-método).

### Handlers

Un handler es una función o closure `async` que recibe una `Request` y devuelve cualquier cosa que implemente `IntoResponse`: texto, `Json(...)`, una tupla `(estado, cuerpo)`, un `Result`, etc. (consulta [Enviar la respuesta](responses.md)).

```rust
// Una closure que ignora la petición
app.get("/", |_| async { "Inicio" });

// Una closure que la usa: indica su tipo y agrega `async move`
app.get("/hello/:name", |req: Request| async move {
    format!("¡Hola, {}!", req.param("name").unwrap_or("desconocido"))
});

// Una función con nombre
async fn list_users(_req: Request) -> Json<Vec<&'static str>> {
    Json(vec!["ada", "grace"])
}
app.get("/users", list_users);
```

### Encadenar llamadas

Cada función de enrutamiento devuelve `&mut Self`, así que puedes encadenar llamadas. Se parece un poco a `app.route('/users').get(...).post(...)` en Express:

```rust
app.get("/users", list_users)
    .post("/users", create_user)
    .get("/users/:id", show_user)
    .delete("/users/:id", delete_user);
```

Encadena sobre tu variable, después de `let mut app = App::new();`. Si encadenas directamente sobre `App::new()`, te quedarías con una referencia a un valor temporal.

## Parámetros de ruta

Un segmento que empieza por `:` captura esa parte de la ruta. `req.param(nombre)` lo lee y devuelve un `Option<&str>`:

```rust
app.get("/users/:id/posts/:post", |req: Request| async move {
    let user = req.param("id").unwrap();
    let post = req.param("post").unwrap();
    format!("publicación {post} del usuario {user}")
});
```

`GET /users/42/posts/7` responde `publicación 7 del usuario 42`. El `unwrap()` no puede fallar aquí, porque la ruta solo coincide cuando los dos parámetros están presentes.

- `req.params()` recorre todos los pares `(nombre, valor)`, en orden.
- Los valores vienen decodificados: `/users/Fran%C3%A7ois` da `François`. Un `+` sigue siendo un `+`, porque solo significa espacio en la query string. `req.path()` sigue devolviendo la ruta sin decodificar.
- Un parámetro nunca coincide con un valor vacío ni con nada que contenga una `/`.
- Los nombres solo pueden tener letras, dígitos y `_`.

La sintaxis es la de Express (versión 4), así que funcionan algunas formas más: varios parámetros en un segmento, restricciones y parámetros opcionales.

### Varios parámetros en un segmento

Un segmento puede mezclar texto y parámetros, siempre que algo de texto los separe:

```rust
app.get("/flights/:from-:to", |req: Request| async move {
    let from = req.param("from").unwrap();
    let to = req.param("to").unwrap();
    format!("de {from} a {to}")
});
app.get("/files/:name.:ext", show_file);
app.get("/api/v:version/status", api_status);
app.get("/img/:id.png", show_png);
```

| Petición | Ruta | Parámetros |
|---|---|---|
| `/flights/LAX-SFO` | `/flights/:from-:to` | `from = "LAX"`, `to = "SFO"` |
| `/files/report.pdf` | `/files/:name.:ext` | `name = "report"`, `ext = "pdf"` |
| `/files/archive.tar.gz` | `/files/:name.:ext` | `name = "archive"`, `ext = "tar.gz"` |
| `/api/v2/status` | `/api/v:version/status` | `version = "2"` |
| `/img/42.png` | `/img/:id.png` | `id = "42"` |
| `/files/README` o `/api/2/status` | ninguna: `404` | |

- Cada parámetro toma al menos un carácter, y todos salvo el último toman lo menos posible, como en Express: por eso `archive.tar.gz` da `archive` y `tar.gz`.
- Dos parámetros tienen que estar separados por texto: `/:from:to` se rechaza al arrancar, ya que nada indica dónde termina `from`.

### Restricciones con expresiones regulares

Una expresión regular entre paréntesis, justo después del nombre, restringe los valores que acepta un parámetro. Una petición cuyo valor no coincide simplemente no coincide con la ruta: otra ruta puede atenderla, o recibe un `404`.

```rust
app.get(r"/users/:id(\d+)", |req: Request| async move {
    let id: u64 = req.param_as("id")?;
    Ok::<_, Error>(format!("usuario n.º {id}"))
});
app.get("/docs/:lang(en|fr|es)/:page", show_doc);
app.get(r"/posts/:year(\d{4})-:slug", show_post);
```

| Petición | Respuesta |
|---|---|
| `/users/42` | `usuario n.º 42` |
| `/users/ada` | `404` (u otra ruta, consulta [más abajo](#gana-la-ruta-más-específica)) |
| `/docs/fr/routing` | `show_doc`, con `lang = "fr"` |
| `/docs/de/routing` | `404` |
| `/posts/2024-hello-world` | `show_post`, con `year = "2024"` y `slug = "hello-world"` |

- En Rust, escribe la ruta como una cadena en bruto, `r"/users/:id(\d+)"`, para que las barras invertidas se queden tal cual. En una cadena normal, duplícalas: `"/users/:id(\\d+)"`.
- La expresión tiene que coincidir con el valor entero: `:lang(en|fr)` acepta `fr`, no `fra`. No hacen falta `^` ni `$`.
- No puede contener una `/`, ya que un parámetro nunca la contiene.
- Se compara con el valor tal como aparece en la URL, antes de decodificarlo: para ella, `%20` son tres caracteres.
- Vitesse usa [regex-lite](https://crates.io/crates/regex-lite): la sintaxis habitual (clases, `\d`, `\w`, `|`, repeticiones, `(?i)` para ignorar mayúsculas y minúsculas) funciona, pero `\d` y `\w` solo cubren ASCII, y las clases Unicode como `\p{L}` no están disponibles. Las expresiones se compilan una sola vez, al arrancar, y solo se ejecutan para los parámetros que tienen una restricción.
- Una expresión no válida detiene el programa al arrancar: `invalid route: invalid regular expression…`.

Una restricción elige la ruta, y `req.param_as` sigue convirtiendo el valor: aquí, un número demasiado grande para caber en un `u64` sigue recibiendo un `400`.

> [!NOTE]
> Express 5 eliminó las expresiones regulares de las rutas, y actix-web las escribe `{id:\d+}`. Vitesse conserva la sintaxis de Express 4, `:id(\d+)`.

### Parámetros opcionales

Un `?` después de un parámetro lo hace opcional. Así, una sola ruta responde a varias rutas distintas:

```rust
app.get("/archive/:year?/:month?", |req: Request| async move {
    match (req.param("year"), req.param("month")) {
        (Some(year), Some(month)) => format!("archivo de {month}/{year}"),
        (Some(year), None) => format!("archivo de {year}"),
        _ => "todo el archivo".to_owned(),
    }
});
```

| Petición | `req.param("year")` | `req.param("month")` |
|---|---|---|
| `/archive` | `None` | `None` |
| `/archive/2024` | `Some("2024")` | `None` |
| `/archive/2024/05` | `Some("2024")` | `Some("05")` |

- Solo los últimos segmentos de una ruta pueden ser opcionales (`/:lang?/about` se rechaza), y solo un parámetro que ocupa un segmento entero (`/files/:name.:ext?` también se rechaza).
- Una restricción también puede ser opcional: `r"/users/:id(\d+)?"` coincide con `/users` y `/users/7`, no con `/users/ada`.
- La ruta cuenta como una ruta por cada forma posible: `/archive/:year?` junto con otra ruta `/archive` para el mismo método es un [duplicado](#rutas-no-válidas-o-duplicadas).

## Parámetros tipados

`req.param_as::<T>(nombre)` convierte un parámetro en cualquier tipo que implemente `FromStr`: enteros, `bool`, `f64`, `IpAddr`, tus propios tipos, etc. Si el parámetro falta o no se puede convertir, devuelve un error `400 Bad Request`, así que basta con un `?`:

```rust
async fn show_user(req: Request) -> vitesse::Result<String> {
    let id: u64 = req.param_as("id")?;
    Ok(format!("usuario n.º {id}"))
}

app.get("/users/:id", show_user);
```

```text
GET /users/42   → 200 usuario n.º 42
GET /users/abc  → 400 {"error":"invalid parameter 'id': 'abc'"}
GET /users/-1   → 400 (un u64 no puede ser negativo)
```

Para responder `404` en lugar de `400` a `/users/abc`, o para dejar que otra ruta la atienda, añade una [restricción](#restricciones-con-expresiones-regulares) a la ruta: `r"/users/:id(\d+)"`.

Para tus propios tipos, implementa `FromStr`:

```rust
use std::str::FromStr;

enum Format {
    Json,
    Csv,
}

impl FromStr for Format {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "json" => Ok(Format::Json),
            "csv" => Ok(Format::Csv),
            _ => Err(()),
        }
    }
}

app.get("/export/:format", |req: Request| async move {
    let message = match req.param_as::<Format>("format")? {
        Format::Json => "exportando en JSON",
        Format::Csv => "exportando en CSV",
    };
    Ok::<_, Error>(message)
});
```

`GET /export/csv` responde `exportando en CSV`, y `GET /export/xml` recibe un `400`.

## Comodines

Un segmento `*nombre` captura el resto de la ruta, barras incluidas. Express 5 usa la misma sintaxis:

```rust
app.get("/files/*path", |req: Request| async move {
    format!("pediste {}", req.param("path").unwrap())
});
```

| Petición | `req.param("path")` |
|---|---|
| `/files/informe.pdf` | `"informe.pdf"` |
| `/files/2024/t1/informe.pdf` | `"2024/t1/informe.pdf"` |
| `/files` o `/files/` | `""` (vacío) |

- Un comodín tiene que ser el último segmento. `/files/*path/edit` se rechaza al arrancar.
- También funciona un comodín anónimo `*`. Su valor se guarda con el nombre `"*"`, así que lo lees con `req.param("*")`.
- `/*` por sí solo atrapa todas las rutas que ninguna ruta más específica captura.

## Cómo se eligen las rutas

### Gana la ruta más específica

Cuando varias rutas podrían coincidir, Vitesse las compara segmento a segmento, del tipo de segmento más específico al menos específico:

1. un segmento estático (`/users/new`);
2. un segmento con una restricción o con texto alrededor de sus parámetros (`/users/:id(\d+)`, `/img/:id.png`);
3. un parámetro simple (`/users/:name`);
4. un comodín (`/users/*rest`).

El orden en que declaras las rutas no importa, mientras que en Express gana la primera ruta registrada. Así, una ruta con una restricción y un parámetro simple pueden compartir la misma posición:

```rust
app.get("/users/new", |_| async { "formulario de alta" });
app.get(r"/users/:id(\d+)", |_| async { "un usuario, por id" });
app.get("/users/:name", |_| async { "un usuario, por nombre" });
app.get("/users/*rest", |_| async { "todo lo demás bajo /users" });
```

| Petición | Ruta usada |
|---|---|
| `/users/new` | `/users/new` (el estático le gana a todo) |
| `/users/42` | `/users/:id(\d+)` (la restricción coincide) |
| `/users/ada` | `/users/:name` (la restricción no coincide) |
| `/users/42/settings` | `/users/*rest` |
| `/users` | `/users/*rest` (comodín vacío) |

Si una rama no lleva a ninguna parte, el enrutador retrocede y prueba la siguiente opción. Con `/a/:x/c` y `/a/b/d`, una petición a `/a/b/c` coincide con `/a/:x/c` con `x = "b"`.

El orden de declaración solo importa entre dos segmentos del segundo tipo en la misma posición, por ejemplo `/:id(\d+)` y `/:slug([a-z-]+)`: se prueban en el orden en que los declaraste.

### Barra final, mayúsculas y query string

- La barra final se ignora: `/users/` coincide con `/users`. Express hace lo mismo por defecto.
- Las rutas **distinguen mayúsculas de minúsculas** por defecto: `/Users` no coincide con `/users`. Express no las distingue por defecto; para hacer lo mismo, llama a `app.case_sensitive(false)`.
- La query string no influye: `/search?q=rust` coincide con `/search`.

```rust
let mut app = App::new();
app.case_sensitive(false);
app.get("/users/:id", show_user); // también responde a /Users/Ada y /USERS/Ada
```

Con `app.case_sensitive(false)`, los segmentos estáticos y el texto alrededor de los parámetros (`.png` en `/img/:id.png`) coinciden sin importar las mayúsculas, también en los prefijos de montaje. Los parámetros conservan las mayúsculas y minúsculas de la petición (`req.param("id")` es `"Ada"`), y las restricciones no se tocan: añade `(?i)` a la expresión si también debe ignorar las mayúsculas. Solo afecta a las letras ASCII.

### Rutas no válidas o duplicadas

Vitesse detecta los errores de declaración al arrancar la aplicación, no cuando llega una petición. `app.get(...)` entra en pánico con un mensaje claro si:

- la ruta no empieza por `/`;
- tiene un segmento vacío (`/a//b`);
- un nombre de parámetro no es válido, o dos parámetros van seguidos sin texto entre ellos (`/:a:b`);
- una restricción no es una expresión regular válida, o su paréntesis no está cerrado;
- un parámetro opcional no está al final de la ruta, o no ocupa un segmento entero;
- un comodín no es el último segmento;
- el mismo método se registra dos veces en la misma ruta.

Dos patrones que solo se diferencian en el nombre de sus parámetros son la misma ruta, así que `/users/:id` y `/users/:name` chocan:

```text
thread 'main' panicked at src/main.rs:12:9:
duplicate route: GET /users/:name is already defined
```

`/users/:id(\d+)` y `/users/:name` no chocan, ya que solo una de las dos tiene una restricción.

## La ruta elegida: `req.route()`

`req.route()` devuelve el patrón de la ruta elegida, prefijo de montaje incluido, como `req.route.path` en Express (o `MatchedPath` en axum):

```rust
app.get("/users/:id", |req: Request| async move {
    format!("ruta: {}", req.route().unwrap_or("?")) // "ruta: /users/:id"
});
```

| Petición | `req.path()` | `req.route()` |
|---|---|---|
| `GET /users/42` | `/users/42` | `Some("/users/:id")` |
| `GET /api/v1/items/a/b`, router montado en `/api/:version` con una ruta `/items/*path` | `/api/v1/items/a/b` | `Some("/api/:version/items/*path")` |
| `GET /archive/2024`, ruta `/archive/:year?/:month?` | `/archive/2024` | `Some("/archive/:year?/:month?")` |
| `DELETE /users/42`, sin ruta `DELETE` (`405`) | `/users/42` | `None` |
| `GET /nope` (`404`) | `/nope` | `None` |

El patrón es el que declaraste, restricciones y `?` incluidos: `r"/users/:id(\d+)"` da `/users/:id(\d+)`.

Es lo que quieres en los registros y las métricas: `/users/:id` ocupa una sola línea en un panel, donde la ruta en bruto ocuparía una por usuario. Los middlewares globales se ejecutan antes del enrutamiento, así que ahí `req.route()` busca la ruta en el momento en que lo llamas. Llámalo antes de `next.run(req)`, que se queda con la petición; el resultado es un `&'static str`, así que puedes conservarlo después:

```rust
use std::time::Instant;

app.middleware(|req: Request, next: Next| async move {
    let route = req.route().unwrap_or("sin ruta");
    let start = Instant::now();
    let res = next.run(req).await;
    println!("{route} {} {:?}", res.status_code(), start.elapsed());
    res
});
```

En los middlewares de router, los de ruta y los handlers, la ruta ya se conoce y `req.route()` no cuesta nada.

## Rutas con nombre y generación de URL

Escribir `/users/42` a mano en cada enlace y cada redirección funciona hasta que la ruta cambia. Mejor dale un nombre a la ruta, con `.name(...)` justo después de ella, y deja que Vitesse construya sus rutas, como `url_for` en actix-web:

```rust
async fn show_user(req: Request) -> String {
    format!("usuario {}", req.param("id").unwrap_or("?"))
}

async fn create_user(req: Request) -> vitesse::Result<Redirect> {
    let id = 42; // el id del nuevo usuario
    let link = req.path_for("user", &[&id])?; // "/users/42"
    Ok(Redirect::see_other(link))
}

app.get("/users/:id", show_user).name("user");
app.post("/users", create_user);
```

- `req.path_for(nombre, &[valores])` devuelve la ruta que corresponde a ese nombre, con sus parámetros rellenados **en orden**. Los valores pueden ser cualquier cosa que implemente `Display`: `&[&42]`, `&[&"LAX", &"SFO"]`, `&[]` para una ruta sin parámetros.
- `req.url_for(nombre, &[valores])` devuelve la URL absoluta, `https://example.com/users/42`: la misma ruta, precedida de `req.protocol()` y `req.host()`. Detrás de un proxy, decláralo con [`app.trust_proxy`](production.md#obtener-la-ip-del-cliente) para que reflejen lo que ve el cliente.

| Ruta | Llamada | Resultado |
|---|---|---|
| `/users/:id` | `path_for("user", &[&42])` | `/users/42` |
| `/users/:id` | `path_for("user", &[&"ada lovelace"])` | `/users/ada%20lovelace` |
| `/flights/:from-:to` | `path_for("flight", &[&"LAX", &"SFO"])` | `/flights/LAX-SFO` |
| `/files/*path` | `path_for("file", &[&"docs/report 2024.pdf"])` | `/files/docs/report%202024.pdf` |
| `/archive/:year?/:month?` | `path_for("archive", &[&2024])` | `/archive/2024` |
| `/users/:id` | `url_for("user", &[&42])` | `https://example.com/users/42` |

- Los valores se codifican, así que un espacio se convierte en `%20` y una `/` en `%2F`. Solo un comodín conserva sus `/`, ya que representa varios segmentos.
- Los parámetros opcionales del final se pueden omitir, y un comodín también: `path_for("archive", &[])` da `/archive`.
- Las restricciones se comprueban: `path_for` rechaza `"ada"` para `r"/users/:id(\d+)"`.
- La ruta se construye en el momento de la llamada, así que puede ir a cualquier parte: una cabecera `Location`, un enlace en `Html(...)`, un campo JSON.

Los nombres se comprueban en dos momentos:

- **Al arrancar**, `name` entra en pánico si no hay una ruta justo antes (un `mount`, por ejemplo), o si el nombre ya está ocupado: los nombres son únicos en toda la aplicación.
- **Al llamarlos**, `path_for` y `url_for` devuelven un error `500` si el nombre es desconocido, si falta o sobra un valor, si un valor no cumple su restricción o (solo `url_for`) si la petición no tiene host. El cliente solo ve `{"error":"Internal Server Error"}`; la causa (`cannot build the URL of 'user': missing a value for ':id'`) se escribe en la salida de error, como los demás errores del servidor.

Las rutas declaradas en un [router](routers.md#rutas-con-nombre-en-los-routers) también pueden tener nombre. El nombre pertenece a toda la aplicación, y la ruta incluye el prefijo de montaje: sus parámetros van primero en la lista de valores.

## HEAD, OPTIONS y 405

Vitesse se encarga del comportamiento HTTP estándar:

- **HEAD**: una petición `HEAD` usa la ruta `GET` y recibe las mismas cabeceras sin cuerpo, salvo que registres `app.head(...)`.
- **405 Method Not Allowed**: cuando la ruta existe pero no para ese método, la respuesta es un `405` con una cabecera `Allow` que enumera los métodos disponibles. Express responde `404` en ese caso.
- **OPTIONS**: sin una ruta `app.options(...)` explícita, una petición `OPTIONS` recibe `204 No Content` con la misma cabecera `Allow`.

```rust
app.get("/items", list_items);
app.post("/items", create_item);
```

```http
DELETE /items HTTP/1.1

HTTP/1.1 405 Method Not Allowed
content-type: application/json
allow: GET, HEAD, POST, OPTIONS

{"error":"Method Not Allowed"}
```

En esa misma ruta, `OPTIONS /items` recibe un `204 No Content` con `allow: GET, HEAD, POST, OPTIONS`.

`app.all(...)` acepta todos los métodos en su ruta, así que nunca produce un `405`. Si la misma ruta tiene además una ruta para un método concreto, esa gana sobre `all`.

> [!TIP]
> Los navegadores envían sus peticiones CORS previas (preflight) como `OPTIONS`. Para responderlas, usa `middleware::cors()`, que las atiende antes del enrutamiento. Consulta [Middlewares](middleware.md).

## Fallback: personalizar el 404

Cuando ninguna ruta coincide, Vitesse responde `404` con `{"error":"Cannot GET /nope"}`, el mismo mensaje que Express. `app.fallback` reemplaza ese handler. Cumple el papel del último `app.use((req, res) => ...)` de una aplicación Express:

```rust
app.fallback(|req: Request| async move {
    (404, format!("Aquí no hay nada: {}", req.path()))
});
```

Un uso habitual es una aplicación de una sola página (SPA): enviar `index.html` para cualquier ruta desconocida y dejar que el enrutador del front-end se encargue.

```rust
app.fallback(|_| async { res::file("dist/index.html").await });
```

- El fallback se ejecuta después de los middlewares globales, como cualquier ruta.
- No se llama para un `405`, porque la ruta sí existe, solo que no para ese método.
- Un [router](routers.md#el-fallback-de-un-router) puede tener su propio fallback para las rutas bajo su prefijo, con `router.fallback(handler)`: `404` en JSON bajo `/api`, por ejemplo, y una página HTML en el resto.
- Para cambiar de una vez el formato de *todas* las respuestas de error (404, 405, 400, 500, etc.), usa `app.on_error`. Consulta [Gestión de errores](errors.md).

## Organizar las rutas

Cuando la aplicación crece, agrupa las rutas relacionadas en un `Router` y móntalo bajo un prefijo, como harías con `express.Router()`:

```rust
let mut api = Router::new();
api.get("/users", list_users).post("/users", create_user);

app.mount("/api", api); // GET /api/users, POST /api/users
```

Los enrutadores pueden tener sus propios middlewares y anidarse: consulta [Enrutadores](routers.md). Para un middleware en una sola ruta (`handler.with(auth)`), consulta [Middlewares](middleware.md).
