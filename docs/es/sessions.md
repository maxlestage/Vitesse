# Cookies firmadas y sesiones

Una cookie guarda unos pocos datos en el navegador del visitante, que los devuelve con cada petición. Con la feature `session`, Vitesse puede hacer dos cosas más con las cookies: **firmarlas**, para que los visitantes puedan leerlas pero no modificarlas (el `cookie-parser` de Express con un secreto), y mantener **sesiones**, datos guardados en el servidor para cada visitante mientras el navegador solo guarda un id aleatorio (el `express-session` de Express).

## Activar la feature `session`

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["session"] }
```

Añade `app.cookie_secret(...)`, `req.sign_cookie(...)`, `req.signed_cookie(...)`, `middleware::session(...)`, `req.session()` y el módulo `vitesse::session`.

## El secreto

Para firmar hace falta un secreto: una cadena larga y aleatoria que solo conoce el servidor. Genera uno, una sola vez:

```sh
openssl rand -base64 48
```

Después pásaselo a la aplicación a través de una variable de entorno:

```rust
let mut app = App::new();
app.cookie_secret(std::env::var("COOKIE_SECRET").expect("COOKIE_SECRET no está definida"));
```

- El secreto tiene que medir al menos 32 bytes: uno más corto hace que `cookie_secret` entre en pánico al arrancar.
- Mantenlo fuera del código y fuera de Git. Cualquiera que lo conozca puede falsificar tus cookies.
- Todas las instancias de la aplicación necesitan el mismo secreto. Cambiarlo invalida todas las cookies firmadas con el anterior: los visitantes pierden sus cookies firmadas y sus sesiones, como si hubieran cerrado sesión.

## Cookies firmadas

`req.sign_cookie(cookie)` firma una cookie antes de que la envíes con `.cookie(...)`, y `req.signed_cookie(nombre)` la vuelve a leer y comprueba la firma:

```rust
use std::time::Duration;
use vitesse::prelude::*;

let mut app = App::new();
app.cookie_secret(std::env::var("COOKIE_SECRET").expect("COOKIE_SECRET no está definida"));

app.post("/theme/:name", |req: Request| async move {
    let theme = req.param("name").unwrap_or("light").to_owned();
    let cookie = Cookie::new("theme", theme).max_age(Duration::from_secs(365 * 24 * 3600));
    res::send_status(204).cookie(req.sign_cookie(cookie))
});

app.get("/", |req: Request| async move {
    // `None` si la cookie falta, o si el visitante la modificó.
    let theme = req.signed_cookie("theme").unwrap_or_else(|| "light".into());
    format!("tema: {theme}")
});
```

La cookie sale como `theme=dark.<firma>`: el valor, un punto y después un HMAC-SHA256 del nombre y el valor de la cookie, calculado con el secreto. `req.signed_cookie("theme")` lo vuelve a calcular: si coincide, obtienes `Some("dark")`, sin la firma; si no, obtienes `None`, como si no hubiera cookie. `req.cookie("theme")` sigue devolviendo el valor en bruto, firma incluida.

Lo que garantiza la firma:

- **El valor viene de ti.** Un visitante que cambia `dark` por `light` no tiene la firma correspondiente, y la cookie se ignora.
- **La firma cubre también el nombre**, así que el valor de una cookie no puede hacerse pasar por el de otra.

Lo que no hace:

- **Ocultar el valor.** Firmada no significa cifrada: cualquiera puede leer `dark` en las herramientas para desarrolladores del navegador. No pongas secretos ni datos personales en una cookie, firmada o no.
- **Hacerla caducar ni revocarla.** Solo el navegador aplica `Max-Age`: alguien que guardó una copia de una cookie firmada puede volver a enviarla más tarde, y sigue siendo válida mientras el secreto no cambie. Para todo lo que tenga que terminar cuando tú lo decidas, un inicio de sesión por ejemplo, usa una [sesión](#sesiones).

`sign_cookie` entra en pánico (así que la petición recibe un `500`) cuando la aplicación no tiene secreto, y `signed_cookie` devuelve `None`. El valor se escribe tal cual: limítate a letras, cifras y `-_.`. Para algo más grande o estructurado, usa una sesión.

## Sesiones

Una sesión guarda datos en el servidor para cada visitante: quién ha iniciado sesión, un carrito, un mensaje que mostrar en la página siguiente. El navegador solo guarda una cookie, `vitesse.sid`, con un id aleatorio que devuelve con cada petición. Añade el middleware `session` y después lee y escribe la sesión con `req.session()`:

```rust
use vitesse::prelude::*;
use vitesse::session::MemoryStore;

let mut app = App::new();
app.middleware(middleware::session(MemoryStore::new()));

app.get("/visits", |req: Request| async move {
    let session = req.session();
    let visits = session.get::<u32>("visits").unwrap_or(0) + 1;
    session.insert("visits", visits)?;
    Ok::<_, Error>(format!("visita n.º {visits}"))
});
```

Cada navegador cuenta sus propias visitas. Una sesión contiene valores con nombre, guardados en JSON: cualquier tipo que implemente `Serialize` puede entrar, y salir como cualquier tipo que implemente `Deserialize`.

| Método | `express-session` | Efecto |
|---|---|---|
| `session.get::<T>(clave)` | `req.session.clave` | El valor como `Option<T>`; `None` si falta o es de otro tipo |
| `session.insert(clave, valor)?` | `req.session.clave = valor` | Guarda un valor; un `Result`, un `Err` (`500`) si no se puede serializar |
| `session.remove(clave)` | `delete req.session.clave` | Quita un valor y lo devuelve (`Option<serde_json::Value>`) |
| `session.contains(clave)` | `'clave' in req.session` | Si la sesión contiene esa clave |
| `session.clear()` | | Quita todos los valores y conserva la sesión |
| `session.is_empty()` | | Si la sesión no contiene ningún valor |
| `session.regenerate()` | `req.session.regenerate()` | Un id nuevo, los mismos datos: llámalo al iniciar sesión |
| `session.destroy()` | `req.session.destroy()` | Borra la sesión, en el almacén y en el navegador: el cierre de sesión |
| `session.id()` | `req.sessionID` | El id, un `Option<String>`: `None` hasta que la sesión se guarda por primera vez |

- Añade el middleware con `app.middleware(...)`, para que todas las rutas tengan una sesión. `req.session()` entra en pánico (y la petición recibe un `500`) en una ruta que el middleware no cubre.
- `get` devuelve un valor propio: pide un `String`, no un `&str`.
- `Session` es `Clone`, y un clon da acceso a la misma sesión.
- En un handler [WebSocket](websocket.md#parámetros-cabeceras-y-estado) (`app.ws`), `req.session()` contiene la sesión tal como estaba cuando se aceptó la conexión. Los cambios que hagas ahí no se guardan, porque el middleware ya ha enviado su respuesta (el `101`): modifica la sesión en una ruta normal.

### Iniciar y cerrar sesión

```rust
use serde::Deserialize;
use vitesse::prelude::*;
use vitesse::session::MemoryStore;

#[derive(Deserialize)]
struct Login {
    email: String,
    password: String,
}

async fn login(req: Request) -> vitesse::Result<Redirect> {
    let form: Login = req.form().await?;
    let Some(user_id) = check_password(&form.email, &form.password).await else {
        return Err(Error::unauthorized("correo o contraseña incorrectos"));
    };
    let session = req.session();
    session.regenerate(); // un id nuevo: el que tenía el visitante hasta ahora ya no sirve
    session.insert("user_id", user_id)?;
    Ok(Redirect::see_other("/account"))
}

async fn account(req: Request) -> vitesse::Result<String> {
    let user_id: u64 = req
        .session()
        .get("user_id")
        .ok_or_else(|| Error::unauthorized("inicia sesión"))?;
    Ok(format!("Hola, usuario n.º {user_id}"))
}

async fn logout(req: Request) -> Redirect {
    req.session().destroy();
    Redirect::see_other("/")
}

let mut app = App::new();
app.middleware(middleware::session(MemoryStore::new()));
app.post("/login", login);
app.get("/account", account);
app.post("/logout", logout);
```

`check_password` es tu propia función: busca al usuario y verifica el hash de la contraseña (con argon2 o bcrypt, por ejemplo), y devuelve un `Option<u64>`.

- **`regenerate()` al iniciar sesión** frustra la fijación de sesión: un atacante que consiguió que la víctima usara un id de sesión que él conoce (en un ordenador compartido, a través de un subdominio) no puede usarlo una vez que la víctima ha iniciado sesión. Los datos se conservan, un carrito por ejemplo.
- **`destroy()` al cerrar sesión** borra la sesión del almacén, así que una copia de la cookie ya no sirve, y le dice al navegador que borre la cookie.
- El cierre de sesión es un `POST`: un `GET` lo puede provocar cualquier enlace o imagen de otro sitio.
- Después de `destroy()`, la sesión sigue destruida hasta el final de la petición: un valor insertado después no se guarda. Para mostrar un mensaje tras cerrar sesión, ponlo en la URL de la redirección (`/?bye`) o en una cookie normal.

Para proteger un grupo de rutas, comprueba la sesión en un [middleware](middleware.md) y pásales el usuario a los handlers:

```rust
#[derive(Clone)]
struct UserId(u64);

async fn require_login(mut req: Request, next: Next) -> Response {
    match req.session().get::<u64>("user_id") {
        Some(id) => {
            req.set(UserId(id));
            next.run(req).await
        }
        None => Redirect::to("/login").into_response(),
    }
}

let mut account = Router::new();
account.middleware(require_login);
account.get("/", |req: Request| async move {
    let user = req.get::<UserId>().unwrap();
    format!("Hola, usuario n.º {}", user.0)
});
app.mount("/account", account);
```

Los middlewares globales se ejecutan antes que los de los routers, así que `require_login` encuentra la sesión que abrió `app.middleware(middleware::session(...))`.

### Opciones

`middleware::session(almacén)` devuelve un builder:

```rust
use std::time::Duration;
use vitesse::SameSite;

app.middleware(
    middleware::session(MemoryStore::new())
        .cookie_name("sid")
        .max_age(Duration::from_secs(30 * 24 * 3600)) // 30 días
        .rolling(true)
        .same_site(SameSite::Strict),
);
```

| Opción | Por defecto | Efecto |
|---|---|---|
| `.cookie_name(nombre)` | `"vitesse.sid"` | El nombre de la cookie (`name` en express-session) |
| `.max_age(duración)` | 1 día | Cuánto vive una sesión después de su último cambio, en el almacén y en el navegador (`cookie.maxAge`) |
| `.rolling(true)` | `false` | Prolonga la sesión en cada petición, para que termine tras `max_age` de inactividad; cuesta una escritura en el almacén por petición (`rolling`) |
| `.secure(bool)` | `Secure` cuando la petición llegó por HTTPS | Fuerza el atributo `Secure`, activado o desactivado (`cookie.secure`) |
| `.same_site(SameSite::Strict)` | `SameSite::Lax` | El atributo `SameSite` (`cookie.sameSite`) |
| `.domain("example.com")` | ninguno: solo este host | El atributo `Domain`, para compartir la sesión con los subdominios (`cookie.domain`) |
| `.path("/app")` | `/` | El atributo `Path` (`cookie.path`) |

### Cuándo se envía la cookie

Una sesión se guarda, y su cookie se envía, solo cuando un handler la modifica: `insert`, `remove` de un valor que existía, `clear` de una sesión que no estaba vacía, `regenerate`. Con `rolling(true)`, una sesión existente también se guarda en cada petición.

- Un visitante que solo lee páginas no recibe ninguna cookie y no ocupa sitio en el almacén, como con `saveUninitialized: false` en express-session.
- Leer la sesión no vuelve a enviar la cookie, ni prolonga la sesión (salvo con `rolling(true)`).
- La sesión se guarda cuando la respuesta sale del middleware, una vez que el handler (y los middlewares que van después de él) han terminado. Un cambio hecho más tarde, en una tarea lanzada con `tokio::spawn`, no se guarda.
- Un id que el almacén no conoce (caducado, borrado, inventado por el cliente) abre una sesión vacía, que recibe un id nuevo si se escribe algo en ella: Vitesse nunca adopta un id elegido por el cliente.
- Dos peticiones simultáneas del mismo visitante trabajan cada una con su propia copia de la sesión, y gana la última en terminar, como con express-session. Mantén las sesiones pequeñas, y guarda los datos que cambian a menudo en tu base de datos.

### Atributos de la cookie

La cookie de sesión es:

- **`HttpOnly`**: el JavaScript de la página no puede leerla, así que una vulnerabilidad XSS no puede robar la sesión.
- **`SameSite=Lax`** por defecto: el navegador la omite en las peticiones que provocan otros sitios, salvo en los enlaces normales (consulta [CSRF](#csrf)).
- **`Secure` cuando la petición llegó por HTTPS**: el navegador solo la devuelve entonces por HTTPS. Detrás de un proxy inverso que termina el TLS, la aplicación recibe HTTP sin cifrar: declara el proxy con [`app.trust_proxy`](requests.md#detrás-de-un-proxy-trust_proxy), para que cuente su `X-Forwarded-Proto: https`, o fuerza el atributo con `.secure(true)`.
- **Firmada** cuando la aplicación tiene un [`cookie_secret`](#el-secreto). El id ya son 128 bits aleatorios del sistema operativo, imposibles de adivinar; la firma permite a Vitesse rechazar un id falsificado sin consultar el almacén.

> [!IMPORTANT]
> Sobre HTTP sin cifrar, cualquiera en la misma red (una wifi pública) puede leer la cookie de sesión y usarla. En producción, sirve la aplicación por HTTPS: consulta [HTTPS y HTTP/2](https.md) y [Puesta en producción](production.md).

## Almacenes

### `MemoryStore`

`MemoryStore::new()` guarda las sesiones en la memoria del proceso. No necesita nada, lo que lo hace ideal para el desarrollo, las pruebas y las aplicaciones pequeñas, pero:

- las sesiones se pierden cuando el servidor se reinicia: cada despliegue cierra la sesión de todo el mundo;
- no se comparten entre instancias: detrás de un balanceador de carga, un visitante que ha iniciado sesión en un servidor no la tiene iniciada en los demás;
- las sesiones caducadas se eliminan de vez en cuando, a medida que llegan otras nuevas.

### Tu propio almacén

Para Redis, PostgreSQL o cualquier otra base de datos, implementa los tres métodos del trait `SessionStore`. Los datos de una sesión son un `Record`, un `serde_json::Map`, fácil de guardar como texto JSON. Esta es la forma de un almacén SQL, con un `HashMap` en lugar de la tabla para que el ejemplo funcione tal cual; los comentarios dan las consultas correspondientes:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use vitesse::session::{Record, SessionStore};
use vitesse::tokio::sync::RwLock;
use vitesse::{BoxError, BoxFuture, serde_json};

/// Hace las veces de una tabla `sessions (id TEXT PRIMARY KEY, data TEXT, expires_at TIMESTAMP)`.
/// Un almacén real guarda aquí un pool de conexiones (`sqlx::PgPool`, `deadpool_redis::Pool`…).
#[derive(Clone, Default)]
struct SqlStore {
    table: Arc<RwLock<HashMap<String, (String, SystemTime)>>>,
}

impl SessionStore for SqlStore {
    fn load(&self, id: &str) -> BoxFuture<Result<Option<Record>, BoxError>> {
        let (table, id) = (self.table.clone(), id.to_owned());
        Box::pin(async move {
            // SELECT data FROM sessions WHERE id = $1 AND expires_at > now()
            let table = table.read().await;
            match table.get(&id) {
                Some((data, expires_at)) if *expires_at > SystemTime::now() => {
                    Ok(Some(serde_json::from_str(data)?))
                }
                _ => Ok(None),
            }
        })
    }

    fn save(&self, id: &str, record: &Record, ttl: Duration) -> BoxFuture<Result<(), BoxError>> {
        let (table, id) = (self.table.clone(), id.to_owned());
        let data = serde_json::to_string(record);
        Box::pin(async move {
            // INSERT INTO sessions (id, data, expires_at) VALUES ($1, $2, now() + $3)
            // ON CONFLICT (id) DO UPDATE SET data = $2, expires_at = now() + $3
            table.write().await.insert(id, (data?, SystemTime::now() + ttl));
            Ok(())
        })
    }

    fn delete(&self, id: &str) -> BoxFuture<Result<(), BoxError>> {
        let (table, id) = (self.table.clone(), id.to_owned());
        Box::pin(async move {
            // DELETE FROM sessions WHERE id = $1
            table.write().await.remove(&id);
            Ok(())
        })
    }
}

let mut app = App::new();
app.middleware(middleware::session(SqlStore::default()));
```

- Cada método devuelve un `BoxFuture` que debe poseer sus datos (`'static`): clona el id y el pool, y serializa el registro, **antes** del bloque `async`.
- `load` se ejecuta en cada petición que lleva una cookie de sesión, `save` cuando la sesión ha cambiado, y `delete` con `destroy()` y `regenerate()`.
- `?` convierte cualquier error en un `BoxError`. Un error de `load` o de `save` da a la petición un `500` (la causa va al log); un error de `delete` solo se registra en el log.
- Con Redis, `SET id data EX ttl`, `GET id` y `DEL id` bastan, y Redis olvida por sí solo las sesiones caducadas. En una tabla SQL, borra de vez en cuando las filas caducadas, en una tarea lanzada junto con la aplicación.

### tower-sessions

Si ya usas [tower-sessions](https://crates.io/crates/tower-sessions) y sus almacenes, también funciona, a través de la feature `tower`: consulta [Sesiones con tower-sessions](from-axum-actix.md#sesiones-con-tower-sessions). Su API es asíncrona (`session.get(...).await?`), e ignora `app.cookie_secret`.

## CSRF

Los navegadores adjuntan las cookies a las peticiones automáticamente, incluidas las que provoca otro sitio: un formulario de otro sitio que envía un `POST` a tu ruta `/transfer` llevaría la sesión de tu visitante. Es la falsificación de peticiones entre sitios (CSRF).

`SameSite=Lax`, el valor por defecto de la cookie de sesión, bloquea la mayor parte: los navegadores no envían la cookie con un `POST`, `PUT`, `PATCH` o `DELETE` que viene de otro sitio, ni con sus llamadas `fetch` ni con sus imágenes, solo con las navegaciones normales (enlaces), que son peticiones `GET`. Lo que te toca a ti es asegurarte de que un `GET` nunca cambia nada.

Añade un token CSRF cuando eso no baste: cuando pases a `SameSite::None` (la aplicación se ejecuta en un iframe de otro sitio, o un proveedor de pagos le envía formularios de vuelta), cuando tus subdominios no se fían todos unos de otros (son el «mismo sitio»), o cuando importan los navegadores antiguos. Guarda un token aleatorio en la sesión, ponlo en un campo oculto de cada formulario y compara los dos en cada `POST`:

```rust
use serde::Deserialize;
use vitesse::prelude::*;

/// El token CSRF de esta sesión, creado la primera vez que se usa.
fn csrf_token(req: &Request) -> vitesse::Result<String> {
    let session = req.session();
    if let Some(token) = session.get::<String>("csrf") {
        return Ok(token);
    }
    let token = format!("{:032x}", rand::random::<u128>()); // cargo add rand
    session.insert("csrf", &token)?;
    Ok(token)
}

app.get("/transfer", |req: Request| async move {
    let token = csrf_token(&req)?;
    Ok::<_, Error>(Html(format!(
        r#"<form method="post"><input type="hidden" name="csrf" value="{token}">…</form>"#
    )))
});

#[derive(Deserialize)]
struct Transfer {
    csrf: String,
    amount: u64,
}

app.post("/transfer", |req: Request| async move {
    let form: Transfer = req.form().await?;
    let expected = req.session().get::<String>("csrf").unwrap_or_default();
    if expected.is_empty() || !middleware::constant_time_eq(form.csrf.as_bytes(), expected.as_bytes()) {
        return Err(Error::forbidden("token CSRF no válido"));
    }
    Ok(format!("{} transferidos", form.amount))
});
```

Con [plantillas](templates.md), pasa el token en el contexto: `req.render("transfer.html", json!({ "csrf": csrf_token(&req)? }))`.

## Pruebas

`TestClient` no guarda las cookies entre peticiones: lee la cabecera `Set-Cookie` de una respuesta y devuelve su parte `nombre=valor` en una cabecera `cookie`, como haría un navegador:

```rust
use vitesse::test::{TestClient, TestResponse};

/// La parte `nombre=valor` de la cabecera `Set-Cookie` de la respuesta.
fn cookie_of(res: &TestResponse) -> String {
    let set_cookie = res.header("set-cookie").expect("no hay cabecera Set-Cookie");
    set_cookie.split(';').next().unwrap().to_owned()
}

#[tokio::test]
async fn cuenta_las_visitas_por_visitante() {
    let client = TestClient::new(my_api::app());

    let first = client.get("/visits").await;
    assert_eq!(first.text(), "visita n.º 1");
    let cookie = cookie_of(&first); // "vitesse.sid=…"

    let second = client.get("/visits").header("cookie", &cookie).await;
    assert_eq!(second.text(), "visita n.º 2");

    // Otro visitante, sin la cookie, empieza de cero.
    assert_eq!(client.get("/visits").await.text(), "visita n.º 1");
}
```

Lo mismo vale para las cookies firmadas, y cambiar el valor es una buena forma de comprobar que se rechaza una cookie falsificada: `cookie.replacen("dark", "light", 1)`. Más en [Pruebas](testing.md#probar-las-sesiones).
