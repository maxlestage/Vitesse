# Plantillas HTML

Para renderizar páginas HTML en el servidor, Express tiene `res.render` con Pug, EJS o Handlebars. Vitesse tiene la feature `templates`, construida sobre [minijinja](https://docs.rs/minijinja), una implementación en Rust de Jinja2: la sintaxis de Jinja (Python), Nunjucks (JavaScript) y Twig (PHP), con `{{ … }}` para los valores y `{% … %}` para la lógica.

## Activar la feature `templates`

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["templates"] }
```

Añade `app.views(...)`, `app.templates(...)`, `req.render(...)` y el módulo `vitesse::templates`, que reexporta minijinja (`vitesse::templates::minijinja`).

## Tu primera página

Pon las plantillas en una carpeta, aquí `templates/`, junto a `Cargo.toml`:

```text
mi-app/
├── Cargo.toml
├── src/
│   └── main.rs
└── templates/
    ├── layout.html
    └── users.html
```

Indica a la aplicación dónde están con `app.views(carpeta)`, y después renderiza una con `req.render(nombre, contexto)`:

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.views("templates");

    app.get("/users", |req: Request| async move {
        let users = vec!["Ada", "Grace", "Margaret"];
        req.render("users.html", json!({ "title": "Usuarios", "users": users }))
    });

    app.run(3000)
}
```

```jinja
{# templates/users.html #}
{% extends "layout.html" %}

{% block content %}
  <h1>{{ title }}</h1>
  <ul>
    {% for user in users %}
      <li>{{ user }}</li>
    {% endfor %}
  </ul>
{% endblock %}
```

- El nombre de una plantilla es su ruta dentro de la carpeta: `"users.html"`, o `"admin/stats.html"` para `templates/admin/stats.html`. Escribe la extensión: no hay ningún `view engine` que la añada.
- La carpeta es relativa al directorio actual del servidor, como las rutas de `res::file`: ejecuta `cargo run` desde la raíz del proyecto.
- `req.render` devuelve un `Result<Html<String>, Error>`: devuélvelo tal cual, o usa `?` en un handler que devuelva `vitesse::Result<Html<String>>`. La respuesta es un `200` con `content-type: text/html; charset=utf-8`.

`render` es un método de la petición, no de un objeto de respuesta: en Vitesse, el handler [devuelve](responses.md) la página renderizada.

## Plantillas base con `extends` y `block`

Una plantilla base contiene lo que comparten todas las páginas, y deja bloques para que las páginas los rellenen:

```jinja
{# templates/layout.html #}
<!doctype html>
<html lang="es">
<head>
  <meta charset="utf-8">
  <title>{% block title %}{{ title }}{% endblock %} · Mi aplicación</title>
</head>
<body>
  <main>{% block content %}{% endblock %}</main>
</body>
</html>
```

Una página empieza con `{% extends "layout.html" %}` y después redefine los bloques que quiera; los demás conservan el contenido de la plantilla base. Las variables de la página (`title` aquí) también están disponibles en la plantilla base. Dentro de un bloque, `{{ super() }}` inserta la versión de ese bloque que tiene la plantilla base.

## Valores, bucles y condiciones

| Sintaxis | Efecto |
|---|---|
| `{{ user.name }}`, `{{ user["name"] }}` | Un valor, o un campo de un valor (escapado en las plantillas HTML) |
| `{{ name\|upper }}`, `{{ items\|length }}`, `{{ bio\|default("ninguna") }}` | Un filtro, aplicado al valor |
| `{% for item in items %}…{% else %}…{% endfor %}` | Un bucle; la parte `else` se muestra cuando la lista está vacía |
| `{{ loop.index }}`, `{{ loop.first }}`, `{{ loop.last }}` | Dentro de un bucle: la posición (desde 1), si es el primer elemento, si es el último |
| `{% if user.admin %}…{% elif user.active %}…{% else %}…{% endif %}` | Una condición |
| `{% set total = items\|length %}` | Una variable de la plantilla |
| `{# un comentario #}` | Un comentario, que no aparece en la página |
| `{%- … -%}` | Quita los espacios en blanco antes o después de la etiqueta |

```jinja
<ul>
  {% for user in users %}
    <li class="{{ "admin" if user.admin else "member" }}">{{ loop.index }}. {{ user.name|title }}</li>
  {% else %}
    <li>Todavía no hay usuarios.</li>
  {% endfor %}
</ul>
```

Entre los filtros incluidos están `upper`, `lower`, `title`, `capitalize`, `trim`, `length`, `default`, `join`, `first`, `last`, `sort`, `reverse`, `replace`, `round` y `safe`. La [documentación de minijinja](https://docs.rs/minijinja/latest/minijinja/syntax/index.html) recoge toda la sintaxis.

## El contexto: `json!` o una estructura

El contexto es cualquier cosa que implemente `Serialize`. `json!` es práctico para unos pocos valores; una estructura con `#[derive(Serialize)]` documenta lo que espera la plantilla, y el compilador lo comprueba del lado de Rust:

```rust
use serde::Serialize;
use vitesse::prelude::*;

#[derive(Serialize)]
struct User {
    name: String,
    admin: bool,
}

#[derive(Serialize)]
struct UsersPage {
    title: &'static str,
    users: Vec<User>,
}

async fn users(req: Request) -> vitesse::Result<Html<String>> {
    let users = vec![
        User { name: "ada lovelace".into(), admin: true },
        User { name: "grace hopper".into(), admin: false },
    ]; // en una aplicación real: desde tu base de datos
    req.render("users.html", UsersPage { title: "Usuarios", users })
}
```

Los campos se convierten en variables (`{{ title }}`, `{% for user in users %}`), `None` se convierte en `none` (falso en un `{% if %}`), y una página sin variables recibe `()` como contexto.

## Escapado y `|safe`

En las plantillas cuyo nombre termina en `.html`, `.htm` o `.xml`, todos los valores se escapan para HTML: `<`, `>`, `&`, `"`, `'` y `/` se convierten en entidades. Un comentario que contiene `<script>` se muestra como texto, y no se ejecuta:

```jinja
<p>{{ comment }}</p>
{# comment = "<script>alert(1)</script>" da: #}
<p>&lt;script&gt;alert(1)&lt;&#x2f;script&gt;</p>
```

Las plantillas con otra extensión (`.txt` para el texto de un correo, por ejemplo) no se escapan.

Para insertar HTML tal cual, márcalo con el filtro `safe`: `{{ article_html|safe }}`. Hazlo solo con HTML que hayas producido tú, Markdown renderizado por tu servidor por ejemplo, y limpiado si viene de los usuarios (con un crate como ammonia).

> [!WARNING]
> El escapado protege el texto y los atributos entre comillas (`title="{{ name }}"`). No vuelve segura una URL: `<a href="{{ website }}">`, con un sitio web que un usuario ha puesto a `javascript:…`, sigue ejecutando un script al hacer clic, así que comprueba que esas URL empiezan por `https://`. Evita también poner valores dentro de bloques `<script>`: ponlos en un atributo `data-` y léelos desde JavaScript.

## Fragmentos con `include`

`{% include %}` inserta otra plantilla, que ve las mismas variables. Es la forma de compartir una barra de navegación o un pie de página entre plantillas base:

```jinja
{# templates/layout.html #}
<body>
  {% include "partials/nav.html" %}
  <main>{% block content %}{% endblock %}</main>
</body>
```

Para un fragmento que recibe parámetros, un campo de formulario por ejemplo, escribe una macro e impórtala:

```jinja
{# templates/forms.html #}
{% macro field(name, label, type="text") %}
  <label>{{ label }} <input name="{{ name }}" type="{{ type }}"></label>
{% endmacro %}

{# templates/login.html #}
{% import "forms.html" as forms %}
<form method="post">
  {{ forms.field("email", "Correo", type="email") }}
  {{ forms.field("password", "Contraseña", type="password") }}
</form>
```

## Filtros, funciones y plantillas integradas: `app.templates`

`app.views(carpeta)` cubre el caso habitual. Para tus propios filtros, funciones o variables globales, configura un `Environment` de minijinja y dáselo a la aplicación con `app.templates(env)`:

```rust
use vitesse::prelude::*;
use vitesse::templates::minijinja::{Environment, UndefinedBehavior, path_loader};

let mut env = Environment::new();
env.set_loader(path_loader("templates")); // lo que hace app.views("templates")
env.set_undefined_behavior(UndefinedBehavior::SemiStrict); // una variable mal escrita es un error
env.add_filter("price", |cents: u64| format!("{}.{:02} €", cents / 100, cents % 100));
env.add_function("asset", |path: String| {
    format!("/static/{path}?v={}", env!("CARGO_PKG_VERSION"))
});
env.add_global("site_name", "Mi tienda");

let mut app = App::new();
app.templates(env);
```

```jinja
<link rel="stylesheet" href="{{ asset("app.css") }}">
<h1>{{ site_name }}</h1>
<p>{{ product.name }}: {{ product.cents|price }}</p>
```

- Un filtro o una función es una closure: sus argumentos se convierten a partir de los valores de la plantilla, y lo que devuelve vuelve a la página.
- Una variable global está disponible en todas las plantillas, como `app.locals` en las vistas de Express.
- Por defecto, una variable que falta se muestra como una cadena vacía. Con `UndefinedBehavior::SemiStrict`, mostrarla o recorrerla es un error, lo que detecta las erratas, mientras que `{% if user %}` sigue funcionando cuando falta `user`.

### Plantillas integradas en el binario

En lugar de un loader, añade las plantillas al entorno con `include_str!`: se compilan dentro del programa, y el servidor ya no necesita la carpeta `templates/` en ejecución.

```rust
use vitesse::templates::minijinja::Environment;

let mut env = Environment::new();
env.add_template("layout.html", include_str!("../templates/layout.html")).unwrap();
env.add_template("users.html", include_str!("../templates/users.html")).unwrap();
app.templates(env);
```

`include_str!` recibe una ruta relativa al archivo fuente (`src/main.rs` aquí). `add_template` comprueba la sintaxis enseguida, así que un error en una plantilla detiene el programa al arrancar en lugar de fallar en la primera petición. El precio: un cambio en una plantilla necesita una nueva compilación.

## Errores

Una plantilla que no existe, un error de sintaxis, un filtro desconocido o un campo leído en un valor que falta: `req.render` devuelve un error, y el visitante recibe un `500 {"error":"Internal Server Error"}`. La causa, con el nombre y la línea de la plantilla, solo se escribe en los logs:

```text
[vitesse] error 500: unknown filter: filter prices is unknown (in users.html:5)
[vitesse] error 500: template not found: template "user.html" does not exist
```

Llamar a `req.render` cuando la aplicación no tiene ni `app.views` ni `app.templates` también da un `500`.

Con `app.views`, una plantilla se compila la primera vez que se usa, así que un error de sintaxis solo aparece cuando alguien abre esa página. Una prueba que renderiza cada página lo detecta antes de un despliegue: consulta [Probar las plantillas](testing.md#probar-las-plantillas).

## Recargar las plantillas

Las plantillas se leen del disco la primera vez que se usa cada una, y después se guardan en memoria: un cambio solo se ve tras un reinicio. Durante el desarrollo, `cargo watch -x run` ([cargo-watch](https://crates.io/crates/cargo-watch)) vigila todo el proyecto y reinicia el servidor también cuando guardas una plantilla. En producción, la caché es lo que quieres: una página se compila una vez y se renderiza muchas.

## Desplegar

La carpeta `templates/` tiene que existir en el directorio desde el que se ejecuta el servidor. En una imagen Docker, cópiala en la etapa de ejecución, como los [archivos estáticos](docker.md#archivos-estáticos-y-otros-recursos), o [integra](#plantillas-integradas-en-el-binario) las plantillas en el binario.

## Viniendo de Express

| Express | Vitesse |
|---|---|
| `app.set('views', './views')` | `app.views("views")` |
| `app.set('view engine', 'pug')` | Nada que configurar: el motor es minijinja, y los nombres de las plantillas incluyen su extensión |
| `res.render('users', { users })` | `req.render("users.html", json!({ "users": users }))` |
| `app.locals.siteName` en todas las vistas | `env.add_global("site_name", …)` con `app.templates(env)` |
| `res.locals.user` | Ponlo en el contexto |
| `extends` y `block` de Pug, `include` de EJS | `{% extends %}`, `{% block %}`, `{% include %}` |

## Otros motores de plantillas

La feature `templates` es una comodidad, no una obligación. Cualquier motor de plantillas de Rust funciona: [askama](https://crates.io/crates/askama) (plantillas comprobadas al compilar), [tera](https://crates.io/crates/tera), [maud](https://crates.io/crates/maud) (HTML escrito como macros de Rust), [handlebars](https://crates.io/crates/handlebars)… Renderiza la página en un `String` con la API del crate y después devuelve `Html(page)`, que la envía con `content-type: text/html; charset=utf-8` (consulta [Responder](responses.md#qué-puede-devolver-un-handler)).
