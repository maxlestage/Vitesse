# HTML templates

To render HTML pages on the server, Express has `res.render` with Pug, EJS or Handlebars. Vitesse has the `templates` feature, built on [minijinja](https://docs.rs/minijinja), a Rust implementation of Jinja2: the syntax of Jinja (Python), Nunjucks (JavaScript) and Twig (PHP), with `{{ … }}` for values and `{% … %}` for logic.

## Enable the `templates` feature

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["templates"] }
```

It adds `app.views(...)`, `app.templates(...)`, `req.render(...)` and the `vitesse::templates` module, which re-exports minijinja (`vitesse::templates::minijinja`).

## Your first page

Put the templates in a folder, `templates/` here, next to `Cargo.toml`:

```text
my-app/
├── Cargo.toml
├── src/
│   └── main.rs
└── templates/
    ├── layout.html
    └── users.html
```

Tell the app where they are with `app.views(dir)`, then render one with `req.render(name, context)`:

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.views("templates");

    app.get("/users", |req: Request| async move {
        let users = vec!["Ada", "Grace", "Margaret"];
        req.render("users.html", json!({ "title": "Users", "users": users }))
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

- A template's name is its path in the folder: `"users.html"`, or `"admin/stats.html"` for `templates/admin/stats.html`. Write the extension: there is no `view engine` to add it.
- The folder is relative to the current directory of the server, like the paths of `res::file`: run `cargo run` from the project's root.
- `req.render` returns a `Result<Html<String>, Error>`: return it as is, or use `?` in a handler that returns `vitesse::Result<Html<String>>`. The response is a `200` with `content-type: text/html; charset=utf-8`.

`render` is a method of the request, not of a response object: in Vitesse, the handler [returns](responses.md) the rendered page.

## Layouts with `extends` and `block`

A layout holds what every page shares, and leaves blocks for the pages to fill in:

```jinja
{# templates/layout.html #}
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>{% block title %}{{ title }}{% endblock %} · My app</title>
</head>
<body>
  <main>{% block content %}{% endblock %}</main>
</body>
</html>
```

A page starts with `{% extends "layout.html" %}`, then redefines the blocks it wants; the others keep the layout's content. The page's variables (`title` here) are available in the layout too. Inside a block, `{{ super() }}` inserts the layout's version of that block.

## Values, loops and conditions

| Syntax | Effect |
|---|---|
| `{{ user.name }}`, `{{ user["name"] }}` | A value, or a field of a value (escaped in HTML templates) |
| `{{ name\|upper }}`, `{{ items\|length }}`, `{{ bio\|default("none") }}` | A filter, applied to the value |
| `{% for item in items %}…{% else %}…{% endfor %}` | A loop; the `else` part shows when the list is empty |
| `{{ loop.index }}`, `{{ loop.first }}`, `{{ loop.last }}` | Inside a loop: the position (from 1), whether it's the first item, the last one |
| `{% if user.admin %}…{% elif user.active %}…{% else %}…{% endif %}` | A condition |
| `{% set total = items\|length %}` | A variable of the template |
| `{# a comment #}` | A comment, left out of the page |
| `{%- … -%}` | Removes the whitespace before or after the tag |

```jinja
<ul>
  {% for user in users %}
    <li class="{{ "admin" if user.admin else "member" }}">{{ loop.index }}. {{ user.name|title }}</li>
  {% else %}
    <li>No users yet.</li>
  {% endfor %}
</ul>
```

The built-in filters include `upper`, `lower`, `title`, `capitalize`, `trim`, `length`, `default`, `join`, `first`, `last`, `sort`, `reverse`, `replace`, `round` and `safe`. The [minijinja documentation](https://docs.rs/minijinja/latest/minijinja/syntax/index.html) lists the whole syntax.

## The context: `json!` or a struct

The context is anything that implements `Serialize`. `json!` is handy for a few values; a struct with `#[derive(Serialize)]` documents what the template expects, and the compiler checks it on the Rust side:

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
    ]; // in a real app: from your database
    req.render("users.html", UsersPage { title: "Users", users })
}
```

Fields become variables (`{{ title }}`, `{% for user in users %}`), `None` becomes `none` (false in an `{% if %}`), and a page without variables takes `()` as its context.

## Escaping and `|safe`

In templates whose name ends with `.html`, `.htm` or `.xml`, every value is HTML-escaped: `<`, `>`, `&`, `"`, `'` and `/` become entities. A comment that contains `<script>` is shown as text, and doesn't run:

```jinja
<p>{{ comment }}</p>
{# comment = "<script>alert(1)</script>" gives: #}
<p>&lt;script&gt;alert(1)&lt;&#x2f;script&gt;</p>
```

Templates with another extension (`.txt` for the text of an email, for instance) aren't escaped.

To insert HTML as is, mark it with the `safe` filter: `{{ article_html|safe }}`. Only do it for HTML you produced yourself, Markdown rendered by your server for instance, and cleaned up if it comes from users (with a crate such as ammonia).

> [!WARNING]
> Escaping protects text and quoted attributes (`title="{{ name }}"`). It doesn't make a URL safe: `<a href="{{ website }}">` with a website set to `javascript:…` by a user still runs a script on click, so check that such URLs start with `https://`. Avoid putting values inside `<script>` blocks, too: put them in a `data-` attribute and read it from JavaScript.

## Partials with `include`

`{% include %}` inserts another template, which sees the same variables. It's the way to share a navigation bar or a footer between layouts:

```jinja
{# templates/layout.html #}
<body>
  {% include "partials/nav.html" %}
  <main>{% block content %}{% endblock %}</main>
</body>
```

For a piece that takes parameters, a form field for instance, write a macro and import it:

```jinja
{# templates/forms.html #}
{% macro field(name, label, type="text") %}
  <label>{{ label }} <input name="{{ name }}" type="{{ type }}"></label>
{% endmacro %}

{# templates/login.html #}
{% import "forms.html" as forms %}
<form method="post">
  {{ forms.field("email", "Email", type="email") }}
  {{ forms.field("password", "Password", type="password") }}
</form>
```

## Filters, functions and embedded templates: `app.templates`

`app.views(dir)` covers the common case. For filters, functions or global variables of your own, configure a minijinja `Environment` and give it to the app with `app.templates(env)`:

```rust
use vitesse::prelude::*;
use vitesse::templates::minijinja::{Environment, UndefinedBehavior, path_loader};

let mut env = Environment::new();
env.set_loader(path_loader("templates")); // what app.views("templates") does
env.set_undefined_behavior(UndefinedBehavior::SemiStrict); // a misspelled variable is an error
env.add_filter("price", |cents: u64| format!("{}.{:02} €", cents / 100, cents % 100));
env.add_function("asset", |path: String| {
    format!("/static/{path}?v={}", env!("CARGO_PKG_VERSION"))
});
env.add_global("site_name", "My shop");

let mut app = App::new();
app.templates(env);
```

```jinja
<link rel="stylesheet" href="{{ asset("app.css") }}">
<h1>{{ site_name }}</h1>
<p>{{ product.name }}: {{ product.cents|price }}</p>
```

- A filter or a function is a closure: its arguments are converted from the template's values, and what it returns goes back into the page.
- A global is available in every template, like Express's `app.locals` in views.
- By default, a missing variable prints as an empty string. With `UndefinedBehavior::SemiStrict`, printing it or looping over it is an error, which catches typos, while `{% if user %}` still works when `user` is missing.

### Templates embedded in the binary

Instead of a loader, add the templates to the environment with `include_str!`: they are compiled into the program, and the server no longer needs the `templates/` folder at runtime.

```rust
use vitesse::templates::minijinja::Environment;

let mut env = Environment::new();
env.add_template("layout.html", include_str!("../templates/layout.html")).unwrap();
env.add_template("users.html", include_str!("../templates/users.html")).unwrap();
app.templates(env);
```

`include_str!` takes a path relative to the source file (`src/main.rs` here). `add_template` checks the syntax right away, so a mistake in a template stops the program at startup instead of failing on the first request. The price: a template change needs a new build.

## Errors

A template that doesn't exist, a syntax error, an unknown filter or a field read on a missing value: `req.render` returns an error, and the visitor gets a `500 {"error":"Internal Server Error"}`. The cause, with the template's name and line, is only written to the logs:

```text
[vitesse] error 500: unknown filter: filter prices is unknown (in users.html:5)
[vitesse] error 500: template not found: template "user.html" does not exist
```

Calling `req.render` when the app has neither `app.views` nor `app.templates` is a `500` too.

With `app.views`, a template is compiled the first time it's used, so a syntax error only shows up when someone opens that page. A test that renders each page catches it before a deployment: see [Testing templates](testing.md#testing-templates).

## Reloading templates

Templates are read from the disk the first time each one is used, then kept in memory: a change only shows after a restart. During development, `cargo watch -x run` ([cargo-watch](https://crates.io/crates/cargo-watch)) watches the whole project and restarts the server when you save a template too. In production, the cache is what you want: a page is compiled once, and rendered many times.

## Deploying

The `templates/` folder must exist in the directory the server runs from. In a Docker image, copy it into the runtime stage, as for [static files](docker.md#static-files-and-other-assets), or [embed](#templates-embedded-in-the-binary) the templates in the binary.

## From Express

| Express | Vitesse |
|---|---|
| `app.set('views', './views')` | `app.views("views")` |
| `app.set('view engine', 'pug')` | Nothing to set: the engine is minijinja, and template names include their extension |
| `res.render('users', { users })` | `req.render("users.html", json!({ "users": users }))` |
| `app.locals.siteName` in every view | `env.add_global("site_name", …)` with `app.templates(env)` |
| `res.locals.user` | Put it in the context |
| Pug's `extends` and `block`, EJS's `include` | `{% extends %}`, `{% block %}`, `{% include %}` |

## Other template engines

The `templates` feature is a convenience, not an obligation. Any Rust template engine works: [askama](https://crates.io/crates/askama) (templates checked at compile time), [tera](https://crates.io/crates/tera), [maud](https://crates.io/crates/maud) (HTML written as Rust macros), [handlebars](https://crates.io/crates/handlebars)… Render the page into a `String` with the crate's API, then return `Html(page)`, which sends it with `content-type: text/html; charset=utf-8` (see [Sending responses](responses.md#what-a-handler-can-return)).
