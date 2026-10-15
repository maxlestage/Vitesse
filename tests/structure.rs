//! Validation, OpenAPI, lifecycle hooks, `param` callbacks and route model
//! binding, route groups and plugins.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};
use vitesse::openapi::{OpenApi, Operation};
use vitesse::prelude::*;
use vitesse::test::TestClient;
use vitesse::validate::{Validate, Validator};

#[derive(Debug, Serialize, Deserialize)]
struct SignUp {
    email: String,
    name: String,
    age: u8,
    website: Option<String>,
}

impl Validate for SignUp {
    fn validate(&self, v: &mut Validator) {
        v.field("email", &self.email).email();
        v.field("name", &self.name).not_empty().max_length(10);
        v.field("age", &self.age).min(13).message("trop jeune");
        v.optional("website", &self.website).url();
    }
}

#[derive(Debug, Deserialize)]
struct Page {
    page: u32,
}

impl Validate for Page {
    fn validate(&self, v: &mut Validator) {
        v.field("page", &self.page).between(1, 100);
    }
}

#[tokio::test]
async fn validation_errors_list_every_field() {
    let mut app = App::new();
    app.post("/users", |req: Request| async move {
        let user: SignUp = req.valid_json().await?;
        Ok::<_, Error>((201, Json(json!({ "name": user.name }))))
    });
    app.post("/form", |req: Request| async move {
        let user: SignUp = req.valid_form().await?;
        Ok::<_, Error>(user.name)
    });
    app.get("/items", |req: Request| async move {
        let page: Page = req.valid_query()?;
        Ok::<_, Error>(format!("page {}", page.page))
    });
    let c = TestClient::new(app);

    let ok = json!({ "email": "ada@example.com", "name": "Ada", "age": 36 });
    let r = c.post("/users").json(&ok).await;
    assert_eq!(r.status(), 201);
    assert_eq!(r.json::<serde_json::Value>()["name"], "Ada");

    let bad = json!({ "email": "nope", "name": "", "age": 9, "website": "ftp://x" });
    let r = c.post("/users").json(&bad).await;
    assert_eq!(r.status(), 422);
    assert_eq!(
        r.json::<serde_json::Value>(),
        json!({
            "error": "Validation failed",
            "details": {
                "age": ["trop jeune"],
                "email": ["must be a valid email address"],
                "name": ["must not be empty"],
                "website": ["must be a valid URL"],
            }
        })
    );

    // Unreadable JSON stays a 400.
    let r = c.post("/users").body("{").await;
    assert_eq!(r.status(), 400);

    let r = c
        .post("/form")
        .form(&[
            ("email", "a@b.co"),
            ("name", "far too long a name"),
            ("age", "20"),
        ])
        .await;
    assert_eq!(r.status(), 422);
    assert_eq!(
        r.json::<serde_json::Value>()["details"]["name"][0],
        "must be at most 10 characters long"
    );

    assert_eq!(c.get("/items?page=3").await.text(), "page 3");
    let r = c.get("/items?page=0").await;
    assert_eq!(r.status(), 422);
    assert_eq!(
        r.json::<serde_json::Value>()["details"]["page"][0],
        "must be between 1 and 100"
    );
    assert_eq!(c.get("/items?page=x").await.status(), 400);
}

#[tokio::test]
async fn error_details_reach_on_error() {
    let mut app = App::new();
    app.get("/", |_| async {
        Error::conflict("taken").with_details(json!({ "suggestion": "ada2" }))
    });
    app.get("/plain", |_| async { Error::conflict("taken") });
    let plain = TestClient::new(app);
    assert_eq!(plain.get("/plain").await.text(), r#"{"error":"taken"}"#);
    let r = plain.get("/").await;
    assert_eq!(r.status(), 409);
    assert_eq!(
        r.text(),
        r#"{"error":"taken","details":{"suggestion":"ada2"}}"#
    );

    let mut app = App::new();
    app.get("/", |_| async {
        Error::conflict("taken").with_details(json!({ "suggestion": "ada2" }))
    });
    app.on_error(|err: Error| {
        let hint = err.details().map(|d| d["suggestion"].clone());
        (
            err.status(),
            Json(json!({ "message": err.message(), "hint": hint })),
        )
    });
    let r = TestClient::new(app).get("/").await;
    assert_eq!(r.json::<serde_json::Value>()["hint"], "ada2");
}

#[derive(Deserialize)]
struct PostPath {
    user: String,
    post: u64,
    draft: Option<bool>,
}

#[tokio::test]
async fn typed_route_parameters() {
    let mut app = App::new();
    app.get(
        "/users/:user/posts/:post/:draft?",
        |req: Request| async move {
            let path: PostPath = req.params_as()?;
            Ok::<_, Error>(format!("{} {} {:?}", path.user, path.post, path.draft))
        },
    );
    let c = TestClient::new(app);
    assert_eq!(
        c.get("/users/ada%20l/posts/42").await.text(),
        "ada l 42 None"
    );
    assert_eq!(
        c.get("/users/ada/posts/7/true").await.text(),
        "ada 7 Some(true)"
    );
    let r = c.get("/users/ada/posts/x").await;
    assert_eq!(r.status(), 400);
    assert!(r.text().contains("invalid route parameters"));
}

#[tokio::test]
async fn openapi_document_and_swagger_ui() {
    let mut app = App::new();
    app.get("/users/:id(\\d+)", |_| async { "user" })
        .name("user")
        .doc(
            Operation::new()
                .summary("Get a user")
                .tag("users")
                .response_json(
                    200,
                    "The user",
                    json!({ "$ref": "#/components/schemas/User" }),
                ),
        );
    let mut api = Router::new();
    api.post("/pets", |_| async { StatusCode::CREATED }).doc(
        Operation::new()
            .operation_id("createPet")
            .body(json!({ "type": "object" })),
    );
    api.get("/pets/secret", |_| async { "" })
        .doc(Operation::new().hidden());
    app.mount("/api", api);
    app.group("/v2", |g| {
        g.get("/ping", |_| async { "pong" });
    });
    app.static_dir("/assets", ".");

    let spec = OpenApi::new("Test API", "1.2.3")
        .schema("User", json!({ "type": "object" }))
        .swagger_ui("/docs");
    let doc = app.openapi_document(&spec);
    assert_eq!(
        doc["paths"]["/api/pets"]["post"]["operationId"],
        "createPet"
    );
    app.openapi("/openapi.json", spec);
    let c = TestClient::new(app);

    let r = c.get("/openapi.json").await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.header("content-type"), Some("application/json"));
    let doc: serde_json::Value = r.json();
    assert_eq!(doc["openapi"], "3.1.0");
    assert_eq!(doc["info"]["title"], "Test API");
    assert_eq!(doc["info"]["version"], "1.2.3");
    let user = &doc["paths"]["/users/{id}"]["get"];
    assert_eq!(user["summary"], "Get a user");
    assert_eq!(user["tags"][0], "users");
    assert_eq!(user["parameters"][0]["name"], "id");
    assert_eq!(user["parameters"][0]["in"], "path");
    assert_eq!(
        doc["paths"]["/api/pets"]["post"]["requestBody"]["required"],
        true
    );
    assert!(doc["paths"]["/v2/ping"]["get"].is_object());
    assert!(doc["paths"]["/api/pets/secret"].is_null());
    // Static files, the document and the Swagger UI page are not listed.
    let paths = doc["paths"].as_object().unwrap();
    assert!(paths.keys().all(|p| !p.starts_with("/assets")));
    assert!(paths.get("/openapi.json").is_none());
    assert!(paths.get("/docs").is_none());

    let r = c.get("/docs").await;
    assert_eq!(r.status(), 200);
    assert!(r.header("content-type").unwrap().starts_with("text/html"));
    assert!(r.text().contains("SwaggerUIBundle"));
    assert!(r.text().contains("\"/openapi.json\""));
}

#[test]
#[should_panic(expected = "must follow the route")]
fn doc_needs_a_route() {
    App::new().doc(Operation::new());
}

#[tokio::test]
async fn lifecycle_hooks() {
    let seen = Arc::new(AtomicUsize::new(0));
    let mut app = App::new();
    app.on_request(|req: &mut Request| {
        if req.header("x-block").is_some() {
            return Err(Error::forbidden("blocked"));
        }
        req.set(String::from("from on_request"));
        Ok(())
    });
    app.pre_handler(|req: &mut Request| {
        // After routing: the route and its parameters are known.
        if req.param("id") == Some("0") {
            return Err(Error::not_found("no user 0"));
        }
        let route = req.route().unwrap_or("?").to_owned();
        req.set(route);
        Ok(())
    });
    let counter = seen.clone();
    app.on_response(move |res: &mut Response| {
        counter.fetch_add(1, Ordering::Relaxed);
        res.set_header("x-hooked", "1");
    });
    app.on_error(|err: Error| (err.status(), format!("custom: {}", err.message())));
    app.get("/users/:id", |req: Request| async move {
        req.get::<String>().cloned().unwrap_or_default()
    });
    let mut admin = Router::new();
    admin.middleware(|req: Request, next: Next| async move {
        next.run(req).await.header("x-router", "1")
    });
    admin.get("/:page", |req: Request| async move {
        req.get::<String>().cloned().unwrap_or_default()
    });
    app.mount("/admin", admin);
    let c = TestClient::new(app);

    let r = c.get("/users/1").await;
    assert_eq!(r.text(), "/users/:id");
    assert_eq!(r.header("x-hooked"), Some("1"));
    let r = c.get("/admin/stats").await;
    assert_eq!(r.text(), "/admin/:page");
    assert_eq!(r.header("x-router"), Some("1"));

    let r = c.get("/users/0").await;
    assert_eq!(r.status(), 404);
    assert_eq!(r.text(), "custom: no user 0");
    assert_eq!(r.header("x-hooked"), Some("1"));

    let r = c.get("/users/1").header("x-block", "1").await;
    assert_eq!(r.status(), 403);
    assert_eq!(r.text(), "custom: blocked");

    // 404s go through on_request and on_response, not pre_handler.
    let r = c.get("/nope").await;
    assert_eq!(r.status(), 404);
    assert_eq!(r.header("x-hooked"), Some("1"));
    assert_eq!(seen.load(Ordering::Relaxed), 5);
}

#[tokio::test]
async fn hooks_turn_panics_into_500() {
    let mut app = App::new();
    app.on_request(|req: &mut Request| {
        if req.path() == "/boom" {
            panic!("boom");
        }
        Ok(())
    });
    app.get("/boom", |_| async { "unreachable" });
    app.get("/", |_| async { "ok" });
    let c = TestClient::new(app);
    assert_eq!(c.get("/boom").await.status(), 500);
    assert_eq!(c.get("/").await.text(), "ok");
}

#[derive(Clone, Debug)]
struct User {
    id: u32,
    name: &'static str,
}

#[tokio::test]
async fn param_callbacks_and_model_binding() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut app = App::new();
    app.get("/users/:user", |req: Request| async move {
        let user = req.get::<User>().expect("bound");
        format!("#{} {}", user.id, user.name)
    });
    // Declared after the route: applies anyway.
    app.bind_param("user", |_req: &Request, id: &str| {
        let id = id.parse::<u32>();
        async move {
            match id {
                Ok(1) => Ok(User { id: 1, name: "Ada" }),
                Ok(_) => Err(Error::not_found("no such user")),
                Err(_) => Err(Error::bad_request("invalid user id")),
            }
        }
    });
    let counter = calls.clone();
    app.param("post", move |req: Request, next: Next| {
        counter.fetch_add(1, Ordering::Relaxed);
        async move { next.run(req).await.header("x-post", "checked") }
    });
    app.get("/users/:user/posts/:post", |req: Request| async move {
        format!(
            "{} / {}",
            req.get::<User>().map_or("?", |u| u.name),
            req.param("post").unwrap_or_default()
        )
    });
    app.get("/plain/:id", |_| async { "untouched" });

    // A router's routes have the router's own callbacks, not the app's.
    let mut api = Router::new();
    api.param("user", |req: Request, next: Next| async move {
        next.run(req).await.header("x-router-param", "1")
    });
    api.get("/users/:user", |req: Request| async move {
        format!("api {}", req.get::<User>().is_some())
    });
    app.group("/api", |g| {
        g.get("/posts/:post", |_| async { "api post" });
    });
    app.mount("/api", api);
    let c = TestClient::new(app);

    assert_eq!(c.get("/users/1").await.text(), "#1 Ada");
    let r = c.get("/users/2").await;
    assert_eq!(r.status(), 404);
    assert_eq!(r.json::<serde_json::Value>()["error"], "no such user");
    assert_eq!(c.get("/users/x").await.status(), 400);

    let r = c.get("/users/1/posts/9").await;
    assert_eq!(r.text(), "Ada / 9");
    assert_eq!(r.header("x-post"), Some("checked"));
    assert_eq!(c.get("/users/2/posts/9").await.status(), 404);
    assert_eq!(calls.load(Ordering::Relaxed), 1);

    assert_eq!(c.get("/plain/1").await.text(), "untouched");

    let r = c.get("/api/users/2").await;
    assert_eq!(r.text(), "api false");
    assert_eq!(r.header("x-router-param"), Some("1"));
    let r = c.get("/api/posts/1").await;
    assert_eq!(r.text(), "api post");
    assert_eq!(r.header("x-post"), None);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn groups_and_plugins() {
    struct Health {
        path: &'static str,
    }
    impl vitesse::Plugin for Health {
        fn register(self, app: &mut App) {
            app.get(self.path, |_| async { "ok" });
        }
    }
    fn users(r: &mut Router) {
        r.get("/", |_| async { "all users" });
        r.get("/:id", |req: Request| async move {
            format!("user {}", req.param("id").unwrap_or_default())
        })
        .name("user");
    }

    let mut app = App::new();
    app.register(Health { path: "/healthz" })
        .register(|app: &mut App| {
            app.state(7u32);
        });
    app.group("/admin", |admin| {
        admin.middleware(|req: Request, next: Next| async move {
            if req.header("x-admin").is_none() {
                return Error::unauthorized("admins only").into_response();
            }
            next.run(req).await
        });
        admin.get("/stats", |req: Request| async move {
            format!("stats {}", req.state::<u32>())
        });
        admin.group("/users", users);
    });
    app.group("/users", |r| {
        r.get(
            "/me",
            |req: Request| async move { req.path_for("user", &[&5]) },
        );
    });
    let c = TestClient::new(app);

    assert_eq!(c.get("/healthz").await.text(), "ok");
    assert_eq!(c.get("/admin/stats").await.status(), 401);
    assert_eq!(c.get("/admin/nope").await.status(), 401);
    assert_eq!(
        c.get("/admin/stats").header("x-admin", "1").await.text(),
        "stats 7"
    );
    assert_eq!(
        c.get("/admin/users/3").header("x-admin", "1").await.text(),
        "user 3"
    );
    assert_eq!(c.get("/users/me").await.text(), "/admin/users/5");
}
