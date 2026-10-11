//! Gabarits HTML avec minijinja : héritage, échappement, erreurs.
#![cfg(feature = "templates")]

use vitesse::prelude::*;
use vitesse::templates::minijinja::Environment;
use vitesse::test::TestClient;

#[tokio::test]
async fn views_from_a_directory() {
    let dir = std::env::temp_dir().join(format!("vitesse-views-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("layout.html"),
        "<title>{{ title }}</title><main>{% block content %}{% endblock %}</main>",
    )
    .unwrap();
    std::fs::write(
        dir.join("users.html"),
        "{% extends \"layout.html\" %}{% block content %}{% for u in users %}<li>{{ u }}</li>{% endfor %}{% endblock %}",
    )
    .unwrap();

    let mut app = App::new();
    app.views(&dir);
    app.get("/users", |req: Request| async move {
        req.render(
            "users.html",
            json!({ "title": "Users", "users": ["Ada", "<script>"] }),
        )
    });
    app.get("/missing", |req: Request| async move {
        req.render("nope.html", json!({}))
    });
    let c = TestClient::new(app);

    let r = c.get("/users").await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.header("content-type"), Some("text/html; charset=utf-8"));
    assert_eq!(
        r.text(),
        "<title>Users</title><main><li>Ada</li><li>&lt;script&gt;</li></main>"
    );
    assert_eq!(c.get("/missing").await.status(), 500);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn an_environment_of_your_own() {
    let mut env = Environment::new();
    env.add_filter("shout", |s: String| format!("{}!", s.to_uppercase()));
    env.add_template("hello.html", "<h1>{{ name|shout }}</h1>")
        .unwrap();
    let mut app = App::new();
    app.templates(env);
    app.get("/hello/:name", |req: Request| async move {
        req.render("hello.html", json!({ "name": req.param("name") }))
    });
    let r = TestClient::new(app).get("/hello/ada").await;
    assert_eq!(r.text(), "<h1>ADA!</h1>");
}

#[tokio::test]
async fn render_without_views_is_a_500() {
    let mut app = App::new();
    app.get("/", |req: Request| async move { req.render("x.html", ()) });
    assert_eq!(TestClient::new(app).get("/").await.status(), 500);
}
