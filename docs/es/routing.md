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
- Un parámetro ocupa un segmento entero. Nunca coincide con un segmento vacío ni con nada que contenga una `/`.
- Los nombres solo pueden tener letras, dígitos y `_`. No se admiten segmentos parciales como `/vuelos/:desde-:hasta` ni expresiones regulares. Captura el segmento entero y divídelo tú.

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

Cuando varias rutas podrían coincidir, Vitesse las compara segmento a segmento: **estático > parámetro > comodín**. El orden de declaración no importa. En Express, gana la primera ruta registrada.

```rust
app.get("/users/new", |_| async { "formulario de alta" });
app.get("/users/:id", |_| async { "un usuario" });
app.get("/users/*rest", |_| async { "todo lo demás bajo /users" });
```

| Petición | Ruta usada |
|---|---|
| `/users/new` | `/users/new` (el estático le gana al parámetro) |
| `/users/42` | `/users/:id` |
| `/users/42/settings` | `/users/*rest` |
| `/users` | `/users/*rest` (comodín vacío) |

Si una rama no lleva a ninguna parte, el enrutador retrocede y prueba la siguiente opción. Con `/a/:x/c` y `/a/b/d`, una petición a `/a/b/c` coincide con `/a/:x/c` con `x = "b"`.

### Barra final, mayúsculas y query string

- La barra final se ignora: `/users/` coincide con `/users`. Express hace lo mismo por defecto.
- Las rutas **distinguen mayúsculas de minúsculas**: `/Users` no coincide con `/users`. Express no las distingue por defecto.
- La query string no influye: `/search?q=rust` coincide con `/search`.

### Rutas no válidas o duplicadas

Vitesse detecta los errores de declaración al arrancar la aplicación, no cuando llega una petición. `app.get(...)` entra en pánico con un mensaje claro si:

- la ruta no empieza por `/`;
- tiene un segmento vacío (`/a//b`);
- un nombre de parámetro no es válido;
- un comodín no es el último segmento;
- el mismo método se registra dos veces en la misma ruta.

Dos patrones que solo se diferencian en el nombre de sus parámetros son la misma ruta, así que `/users/:id` y `/users/:name` chocan:

```text
thread 'main' panicked at src/main.rs:12:9:
duplicate route: GET /users/:name is already defined
```

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
- Para cambiar de una vez el formato de *todas* las respuestas de error (404, 405, 400, 500, etc.), usa `app.on_error`. Consulta [Gestión de errores](errors.md).

## Organizar las rutas

Cuando la aplicación crece, agrupa las rutas relacionadas en un `Router` y móntalo bajo un prefijo, como harías con `express.Router()`:

```rust
let mut api = Router::new();
api.get("/users", list_users).post("/users", create_user);

app.mount("/api", api); // GET /api/users, POST /api/users
```

Los enrutadores pueden tener sus propios middlewares y anidarse: consulta [Enrutadores](routers.md). Para un middleware en una sola ruta (`handler.with(auth)`), consulta [Middlewares](middleware.md).
