//! HTML templates (`res.render` in Express), with
//! [minijinja](https://crates.io/crates/minijinja), a Rust implementation of
//! Jinja2: `{{ variable }}`, `{% for %}`, `{% if %}`, template inheritance
//! with `{% extends %}` and `{% block %}`, filters… Needs the `templates`
//! feature.
//!
//! ```no_run
//! use vitesse::prelude::*;
//!
//! let mut app = App::new();
//! app.views("templates"); // templates/users.html, templates/layout.html…
//! app.get("/users", |req: Request| async move {
//!     let users = vec!["Ada", "Grace"];
//!     req.render("users.html", json!({ "title": "Users", "users": users }))
//! });
//! ```
//!
//! with `templates/users.html`:
//!
//! ```jinja
//! {% extends "layout.html" %}
//! {% block content %}
//!   <ul>{% for user in users %}<li>{{ user }}</li>{% endfor %}</ul>
//! {% endblock %}
//! ```
//!
//! Values are HTML-escaped in `.html`, `.htm` and `.xml` templates. Templates
//! are read from the disk the first time they are used, then kept in memory:
//! restart the server to see a change. For filters or functions of your own,
//! configure a [`minijinja::Environment`] and give it to
//! [`App::templates`](crate::App::templates).

pub use minijinja;

use crate::error::Error;
use crate::response::Html;

/// Renders a template of an environment, for [`Request::render`](crate::Request::render).
pub(crate) fn render(
    env: Option<&minijinja::Environment<'static>>,
    name: &str,
    context: impl serde::Serialize,
) -> Result<Html<String>, Error> {
    let env = env.ok_or_else(|| {
        Error::internal("no templates: call `app.views(\"templates\")` at startup")
    })?;
    let template = env
        .get_template(name)
        .map_err(|e| Error::internal("Internal Server Error").with_source(e))?;
    template
        .render(context)
        .map(Html)
        .map_err(|e| Error::internal("Internal Server Error").with_source(e))
}

/// An environment that loads the templates of a directory.
pub(crate) fn from_dir(dir: std::path::PathBuf) -> minijinja::Environment<'static> {
    let mut env = minijinja::Environment::new();
    env.set_loader(minijinja::path_loader(dir));
    env
}
