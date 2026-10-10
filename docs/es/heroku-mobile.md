# Desplegar en Heroku desde el móvil

No necesitas una computadora para poner en línea una aplicación Vitesse. Esta guía te lleva paso a paso desde el navegador de tu teléfono: primero un despliegue con un solo toque gracias al botón Deploy y luego un redespliegue automático en cada commit con GitHub Actions.

> [!NOTE]
> Todos los pasos funcionan en un navegador móvil (Safari, Chrome, Firefox…). Las interfaces de Heroku y GitHub están en inglés, así que los botones y menús se indican en inglés. Además cambian de vez en cuando: si una etiqueta no coincide exactamente con lo que ves, busca la opción más parecida. En github.com, si parece faltar un menú o una pestaña, activa el modo «Sitio de escritorio» de tu navegador (suele estar en su menú).

## Antes de empezar

Necesitas:

- **Una cuenta de Heroku**: regístrate en https://signup.heroku.com. Heroku ya no tiene plan gratuito. Las opciones más baratas son los dynos **Eco** (unos 5 $ al mes) y **Basic** (unos 7 $ al mes), pero los precios cambian: consulta https://www.heroku.com/pricing. Heroku pide un **método de pago** antes de poder crear una aplicación.
- **Una cuenta de GitHub**, solo para las secciones 2 y 3 (tu propia copia del código y los despliegues automáticos).

### Cómo funciona

El repositorio ya contiene todo lo que Heroku necesita, así que no hay que compilar nada en el teléfono:

| Archivo | Función |
|---|---|
| `app.json` | Describe la aplicación para el botón Deploy (stack «container», basado en Docker). |
| `heroku.yml` | Le indica a Heroku que construya la imagen a partir del `Dockerfile`. |
| `Dockerfile` | Compila un ejemplo (`demo` por defecto) en modo release y lo empaqueta en una imagen pequeña. |
| `examples/demo.rs` | La aplicación de demostración: una página de inicio, `/health` y una pequeña API JSON. |
| `.github/workflows/heroku.yml` | El workflow **Deploy to Heroku**, para los despliegues automáticos. |

Heroku construye la imagen Docker en sus propios servidores, la arranca y le indica a la aplicación en qué puerto escuchar.

## 1. Lo más rápido: el botón Deploy

### Desplegar la aplicación de demostración

[![Deploy to Heroku](https://www.herokucdn.com/deploy/button.svg)](https://www.heroku.com/deploy?template=https://github.com/maxlestage/Vitesse)

1. Toca el botón de arriba (también está en el README del proyecto). Inicia sesión en Heroku si te lo pide.
2. Heroku muestra un formulario para crear una aplicación nueva:
   - **App name**: por ejemplo `mi-demo-vitesse`. Los nombres son únicos en todo Heroku y solo admiten minúsculas, números y guiones. También puedes dejarlo vacío y Heroku elegirá uno.
   - **Choose a region**: Estados Unidos o Europa.
3. Toca **Deploy app**. Heroku descarga el código y construye la imagen Docker; puedes seguir el registro de construcción en la página. Calcula unos minutos: el código Rust se compila con todas las optimizaciones activadas.
4. Cuando termine la construcción, toca **View** para abrir tu aplicación. Deberías ver «⚡ Vitesse is running.». La dirección exacta (`https://….herokuapp.com`) aparece en la pestaña **Settings** de la aplicación en Heroku.

Prueba luego estas rutas, añadiéndolas después de la dirección de tu aplicación en la barra del navegador:

| Ruta | Respuesta |
|---|---|
| `/health` | `ok` |
| `/api/hello/Ada` | `{"message":"Hello, Ada!"}` |
| `/api/todos` | `[]` (la lista de tareas, vacía por ahora) |

> [!NOTE]
> La demo guarda su lista de tareas en memoria: se borra cada vez que la aplicación se reinicia (nuevo despliegue, cambio de configuración y el reinicio automático que Heroku hace aproximadamente una vez al día). Para datos que deban conservarse, usa una base de datos.

### Desplegar tu propia copia (fork)

Para modificar el código necesitas tu propia copia del repositorio en GitHub:

1. Abre https://github.com/maxlestage/Vitesse, toca **Fork** y luego **Create fork**.
2. Abre el enlace de despliegue con tu nombre de usuario de GitHub en lugar de `maxlestage`, escribiéndolo en la barra de direcciones:

   ```text
   https://www.heroku.com/deploy?template=https://github.com/<tu-usuario>/Vitesse
   ```

   También puedes editar el botón en el `README.md` de tu fork para que apunte a él (la sección 3 explica cómo editar un archivo desde el teléfono).
3. Sigue los mismos pasos que antes.

> [!IMPORTANT]
> El botón despliega **una sola vez**. Cuando más adelante modifiques tu fork, la aplicación de Heroku no se actualizará sola. Para redesplegar en cada commit, configura el workflow de la siguiente sección y reutiliza el mismo nombre de aplicación: el workflow puede actualizar la aplicación que acabas de crear.

## 2. Despliegue continuo con GitHub Actions

Objetivo: cada commit en la rama `master` de tu fork reconstruye y redespliega la aplicación sin que tengas que hacer nada más. El repositorio incluye un workflow llamado **Deploy to Heroku** ([`.github/workflows/heroku.yml`](https://github.com/maxlestage/Vitesse/blob/master/.github/workflows/heroku.yml)) que habla directamente con la Platform API de Heroku: sin la CLI de Heroku y sin computadora.

En cada ejecución:

1. comprueba que está configurado (si no, se detiene sin fallar cuando se trata de un push);
2. crea la aplicación de Heroku si todavía no existe (con el stack Docker «container»);
3. envía tu código a Heroku, que construye la imagen a partir de `heroku.yml` y la publica; el registro de construcción se muestra en directo en GitHub;
4. escribe la dirección de la aplicación en el resumen de la ejecución.

Necesitas tu propio fork (consulta «Desplegar tu propia copia» más arriba).

### Paso 1: obtener un token de la API de Heroku

1. En el navegador del teléfono, ve a https://dashboard.heroku.com e inicia sesión.
2. Abre **Account settings** (desde el menú o tu avatar, arriba a la derecha).
3. Elige una de estas dos opciones:
   - **Rápida**: en la sección **API Key**, toca **Reveal** y copia la clave.
   - **Recomendada**: abre la pestaña **Applications** y, en **Authorizations**, toca **Create authorization**. Ponle una descripción como «GitHub Actions» (si dejas vacía la expiración, normalmente obtienes un token de larga duración) y copia el token. Un token dedicado se puede revocar por separado, sin afectar a nada más.

> [!WARNING]
> Este token actúa en tu nombre sobre toda tu cuenta de Heroku. Nunca lo pegues en un archivo, un commit, un issue o un mensaje: solo en los secretos de GitHub, como se explica en el siguiente paso.

### Paso 2: añadir el secreto y la variable en GitHub

1. En github.com, abre **tu fork** y toca **Settings** (en la barra de pestañas del repositorio; desplázala hacia un lado o activa el «Sitio de escritorio» si no la ves).
2. En el menú, abre **Secrets and variables** y luego **Actions**.
3. En la pestaña **Secrets**, toca **New repository secret**:
   - **Name**: `HEROKU_API_KEY`
   - **Secret**: pega el token
   - toca **Add secret**.
4. Abre la pestaña **Variables** y toca **New repository variable**:
   - **Name**: `HEROKU_APP_NAME`
   - **Value**: el nombre de tu aplicación (la que creaste con el botón, o un nombre nuevo: el workflow crea la aplicación si hace falta)
   - toca **Add variable**.
5. Opcional: añade una variable `HEROKU_REGION` con el valor `eu` para crear la aplicación en Europa. Solo se usa cuando el workflow crea la aplicación.

| Nombre | Tipo | Obligatorio | Valor |
|---|---|---|---|
| `HEROKU_API_KEY` | Secreto | Sí | Tu token de Heroku |
| `HEROKU_APP_NAME` | Variable | Sí | Por ejemplo `mi-demo-vitesse` |
| `HEROKU_REGION` | Variable | No | `us` (por defecto) o `eu` |

### Paso 3: activar Actions en tu fork

GitHub desactiva los workflows de un fork recién creado. Abre la pestaña **Actions** de tu fork: si un mensaje indica que los workflows no se ejecutan en este repositorio bifurcado, toca el botón que los activa.

### Paso 4: lanzar el primer despliegue

1. En la pestaña **Actions**, elige **Deploy to Heroku** en la lista de workflows.
2. Toca **Run workflow**, deja la rama `master` (el campo «Heroku app name» es opcional: sustituye a `HEROKU_APP_NAME` solo para esa ejecución) y confirma con el botón verde **Run workflow**.
3. Toca la ejecución que aparece para seguirla. La primera construcción tarda unos minutos.
4. Cuando se ponga en verde, la dirección de la aplicación aparecerá en el resumen de la ejecución.

A partir de ahora, **cada commit en `master` que toque el código** redespliega la aplicación: `src/`, `examples/`, `Cargo.toml`, `Cargo.lock`, `Dockerfile`, `heroku.yml` o el propio workflow. Cambiar el README o la documentación no lanza ningún despliegue.

> [!NOTE]
> Mientras falte el secreto o la variable, los pushes simplemente se saltan el despliegue (la ejecución muestra un aviso, no un fallo). En cambio, una ejecución manual falla con un mensaje que indica lo que falta.

## 3. Personaliza tu aplicación

### Editar el código desde el teléfono

1. En github.com, dentro de tu fork, abre `examples/demo.rs`.
2. Toca el icono del **lápiz** (✏️, «Edit this file»).
3. Cambia algo, por ejemplo el saludo de la ruta `/api/hello/:name`:

   ```rust
   app.get("/api/hello/:name", |req: Request| async move {
       let name = req.param("name").unwrap_or("world");
       Json(json!({ "message": format!("¡Hola, {name}!") }))
   });
   ```

4. Toca **Commit changes…**, escribe un mensaje corto, deja marcada la opción «Commit directly to the `master` branch» y confirma con **Commit changes**.
5. El workflow arranca solo (puedes seguirlo en la pestaña **Actions**). Unos minutos después, la nueva versión estará en línea.

Añadir una ruta es igual de fácil. Inserta esta línea junto a las demás rutas:

```rust
app.get("/api/ping", |_| async { json!({ "pong": true }) });
```

Consulta [Enrutamiento](routing.md) y [Responder](responses.md) para ver todo lo que puede hacer una ruta.

> [!TIP]
> Un error de tecleo en el código Rust hace que falle la construcción, y la versión anterior sigue en línea sin más: Heroku solo publica las construcciones que terminan bien. Abre la ejecución fallida en la pestaña **Actions** para leer el mensaje del compilador (indica el archivo, la línea y la columna), corrige el código y vuelve a hacer commit.

### Usar tu propio ejemplo

Puedes dejar `demo.rs` como está y escribir tu aplicación en un archivo nuevo:

1. En tu fork, abre la carpeta `examples`, toca **Add file** → **Create new file** y ponle un nombre, por ejemplo `mi_app.rs`.
2. Pega una aplicación mínima:

   ```rust
   use vitesse::prelude::*;

   fn main() -> std::io::Result<()> {
       let mut app = App::new();
       app.middleware(middleware::logger());

       app.get("/", |_| async { "¡Hola desde mi teléfono!" });
       app.get("/health", |_| async { "ok" });

       // Heroku elige el puerto: escucha siempre en $PORT.
       let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
       app.run(port)
   }
   ```

3. Haz commit del archivo.
4. Abre el `Dockerfile`, toca el lápiz y sustituye la línea `ARG EXAMPLE=demo` por `ARG EXAMPLE=mi_app` (el nombre del archivo sin `.rs`). Haz commit: el workflow construye y despliega tu ejemplo.

El argumento de construcción `EXAMPLE` elige qué archivo de `examples/` se compila; al cambiar su valor por defecto en el `Dockerfile`, Heroku pasa a usar tu ejemplo. Tu código puede usar los crates ya declarados en `Cargo.toml`: `vitesse`, `serde` (con `derive`) y `tokio`.

> [!IMPORTANT]
> Tu aplicación debe escuchar en **`0.0.0.0:$PORT`**. Heroku elige el puerto al arrancar y lo pasa en la variable de entorno `PORT`. `app.run(port)` con un simple número de puerto (o una cadena que solo contenga dígitos) ya escucha en todas las interfaces. Nunca escribas `3000` a fuego y nunca escuches solo en `127.0.0.1`: Heroku no podría llegar a tu aplicación.

> [!TIP]
> ¿Necesitas otro crate? Abre `Cargo.toml` con el icono del lápiz, añade una línea bajo `[dev-dependencies]` (la sección que pueden usar los ejemplos sin cambiar las dependencias de la propia biblioteca), por ejemplo `chrono = "0.4"`, y haz commit. La construcción resuelve el nuevo crate por sí sola, mientras que los demás conservan las versiones fijadas en `Cargo.lock`. Más adelante, hacer commit de un `Cargo.lock` actualizado desde una computadora (cualquier `cargo build` lo refresca) sigue siendo una buena práctica para tener construcciones reproducibles.

### Configuración: variables de entorno

En Heroku, abre tu aplicación y ve a **Settings** → **Reveal Config Vars**. Cada «config var» se convierte en una variable de entorno, que tu código lee con `std::env::var("NOMBRE")`. Guardar un cambio reinicia la aplicación. No definas `PORT` tú mismo: Heroku se encarga.

## 4. El día a día

### Registros

Panel de Heroku → tu aplicación → **More** (arriba a la derecha) → **View logs**. Verás los mensajes del propio Heroku (arranque, parada, caídas, una línea por petición de su router) y todo lo que imprime tu aplicación. Con `middleware::logger()`, la aplicación escribe una línea por petición, como `GET /api/todos 200 0.084 ms`.

La pestaña **Activity** muestra las construcciones y publicaciones, con un enlace al registro de cada construcción.

### Reiniciar, escalar, detener

- **Reiniciar**: **More** → **Restart all dynos**.
- **Tipo de dyno**: en la pestaña **Resources**, edita la línea `web` (icono del lápiz) para cambiar el tipo (Eco, Basic, Standard…); el precio cambia en consecuencia. Un dyno Eco se duerme tras unos 30 minutos sin tráfico y la siguiente visita lo despierta en unos segundos; un dyno Basic nunca duerme.
- **Más dynos**: ejecutar varios dynos del mismo proceso requiere dynos Standard o superiores.
- **Detener**: en el mismo modo de edición, apaga el dyno `web`, o elimina la aplicación (**Settings** → **Delete app**).

### Dominio propio

En **Settings** → **Domains**, toca **Add domain** e introduce, por ejemplo, `www.ejemplo.com`. Heroku te da un **destino DNS** («DNS target»): crea en tu registrador de dominios un registro `CNAME` que apunte a él. Un dominio raíz (`ejemplo.com`) requiere un registrador que admita registros `ALIAS`/`ANAME` o el aplanamiento de CNAME.

Los certificados HTTPS de los dominios propios los gestiona el Automated Certificate Management (ACM) de Heroku, que no está disponible en todos los tipos de dyno: compruébalo en https://devcenter.heroku.com/articles/automated-certificate-management. Más detalles: https://devcenter.heroku.com/articles/custom-domains.

## 5. Solución de problemas

### La construcción falla

Abre el registro de construcción: en GitHub (pestaña **Actions**, ejecución fallida) o en Heroku (pestaña **Activity** → registro de construcción). Busca la primera línea que empiece por `error`:

| Mensaje | Causa | Solución |
|---|---|---|
| `error[E…]` con un archivo y una línea | Error de compilación de Rust | Corrige el código en esa línea y vuelve a hacer commit. |
| `no example target named …` | `ARG EXAMPLE` no coincide con ningún archivo de `examples/` | Usa el nombre del archivo sin `.rs`. |
| `no matching package named …` o `failed to select a version …` | El nombre o la versión de un crate en `Cargo.toml` es incorrecto | Comprueba el nombre exacto y una versión existente en https://crates.io y vuelve a hacer commit. |

### El workflow falla antes de construir

| Mensaje | Solución |
|---|---|
| `Heroku deployment is not configured` | Añade el secreto `HEROKU_API_KEY` y la variable `HEROKU_APP_NAME` (paso 2). |
| `Heroku rejected the API key` | El token es incorrecto, caducó o se revocó: crea uno nuevo y actualiza el secreto (**Settings** → **Secrets and variables** → **Actions** → `HEROKU_API_KEY`). |
| `This Heroku account cannot access the app` | Otra cuenta de Heroku ya usa ese nombre: elige otro en `HEROKU_APP_NAME`. |

Si falla la creación de la aplicación, comprueba que tu cuenta de Heroku tiene un método de pago, o crea primero la aplicación con el botón Deploy (o desde el panel de Heroku) y pon su nombre en `HEROKU_APP_NAME`.

### «Application error» en el navegador

La construcción salió bien, pero la aplicación no responde. Abre los registros y busca un código de error:

| Código | Significado | Qué hacer |
|---|---|---|
| `R10` Boot timeout | La aplicación no escuchaba en `$PORT` dentro de los 60 segundos siguientes al arranque. | Escucha en `$PORT` (ver más arriba), nunca en un puerto fijo ni en `127.0.0.1`. |
| `H10` App crashed | El proceso se detuvo. | Lee las líneas justo encima en los registros: un pánico en `main`, una ruta no válida o duplicada (Vitesse entra en pánico al arrancar e indica la ruta culpable), un puerto fijo… |
| `H14` No web dynos running | No hay ningún dyno en marcha para el proceso `web`. | En **Resources**, comprueba que el dyno `web` está encendido. |
| `H12` Request timeout | Una petición tardó más de 30 segundos. | Heroku corta las peticiones a los 30 segundos: haz el handler más rápido o usa `middleware::timeout(...)` para responder antes con un `503` limpio. |

Un pánico dentro de un handler no tumba la aplicación: Vitesse lo convierte en una respuesta `500`. Todos los códigos de error de Heroku están en https://devcenter.heroku.com/articles/error-codes.

### Otras sorpresas

- **La primera petición es lenta**: un dyno Eco estaba dormido; se despierta con la primera visita.
- **Mis datos desaparecieron**: los datos en memoria se pierden en cada reinicio. Usa una base de datos.
- **`PORT=8080` en el `Dockerfile`**: es solo el valor por defecto para otras plataformas. En Heroku manda la variable `PORT` definida al arrancar.
- **WebSocket funciona**: el router de Heroku admite conexiones [WebSocket](websocket.md) (usa `wss://` con la dirección HTTPS de tu aplicación). Cierra una conexión que pasa unos 55 segundos en silencio: haz que el servidor envíe con regularidad un mensaje o un `Message::Ping` (cada 30 segundos, por ejemplo).
- **HTTP/3, no**: el router de Heroku no reenvía UDP a los dynos, así que [HTTP/3](http3.md) no puede llegar a tu aplicación en Heroku. Deja allí desactivada la feature `http3`.

## Para saber más

- [Docker](docker.md): construir y ejecutar la misma imagen en tu computadora o en otras plataformas.
- [Puesta en producción](production.md): límites, registros, apagado ordenado y seguridad.
- [Configuración del servidor](server.md): `app.run`, direcciones de escucha, workers.
