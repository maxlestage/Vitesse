//! Problem Details, middleware groups, caching, typed routes, async and
//! router hooks, test assertions, PubSub over SSE, OpenAPI pages and
//! typed schemas.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use vitesse::middleware::Stack;
use vitesse::openapi::OpenApi;
use vitesse::prelude::*;
use vitesse::problem::Problem;
use vitesse::test::TestClient;
use vitesse::{Middleware, StatusCode, TypedRoute};

#[tokio::test]
async fn problem_details_for_every_error() {
    let mut app = App::new();
    app.problem_details();
    app.middleware(middleware::cors());
    app.get("/accounts/:id", |_req: Request| async move {
        Err::<String, _>(
            Error::forbidden("Your balance is too low").with_details(json!({ "balance": 30 })),
        )
    });
    app.post("/custom", |_| async {
        Problem::new(409)
            .type_uri("https://example.com/probs/taken")
            .extension("suggestions", json!(["ada2"]))
    });
    app.get("/ok", |_| async { "fine" });
    let client = TestClient::new(app);

    client
        .get("/accounts/12")
        .header("origin", "https://a.example")
        .await
        .assert_status(403)
        .assert_header("content-type", "application/problem+json")
        .assert_header("access-control-allow-origin", "*")
        .assert_json(json!({
            "type": "about:blank",
            "title": "Forbidden",
            "status": 403,
            "detail": "Your balance is too low",
            "instance": "/accounts/12",
            "details": { "balance": 30 }
        }));
    client
        .delete("/ok")
        .await
        .assert_status(405)
        .assert_header("content-type", "application/problem+json")
        .assert_json_path("instance", "/ok");
    assert!(client.delete("/ok").await.header("allow").is_some());
    client
        .get("/nope")
        .await
        .assert_not_found()
        .assert_json_path("detail", "Cannot GET /nope");
    client
        .post("/custom")
        .await
        .assert_status(409)
        .assert_json_path("type", "https://example.com/probs/taken")
        .assert_json_path("suggestions.0", "ada2");
    client.get("/ok").await.assert_ok().assert_text("fine");
}

#[tokio::test]
async fn problem_details_customized_and_on_error_wins() {
    let mut app = App::new();
    app.problem_details_with(|err: &Error, problem: Problem| {
        problem.type_uri(format!(
            "https://api.example.com/problems/{}",
            err.status().as_u16()
        ))
    });
    app.get("/teapot", |_| async {
        Err::<(), _>(Error::new(418, "short and stout"))
    });
    let client = TestClient::new(app);
    client
        .get("/teapot")
        .await
        .assert_status(418)
        .assert_json_path("type", "https://api.example.com/problems/418");

    let mut app = App::new();
    app.problem_details();
    app.on_error(|err: Error| (err.status(), format!("custom: {}", err.message())));
    app.get("/x", |_| async { Err::<(), _>(Error::bad_request("bad")) });
    TestClient::new(app)
        .get("/x")
        .await
        .assert_status(400)
        .assert_text("custom: bad");
}

#[tokio::test]
async fn validation_errors_as_problems() {
    use vitesse::validate::{Validate, Validator};

    #[derive(serde::Deserialize)]
    struct Signup {
        email: String,
    }
    impl Validate for Signup {
        fn validate(&self, v: &mut Validator) {
            v.field("email", &self.email).email();
        }
    }

    let mut app = App::new();
    app.problem_details();
    app.post("/signup", |req: Request| async move {
        let signup: Signup = req.valid_json().await?;
        Ok::<_, Error>(signup.email)
    });
    TestClient::new(app)
        .post("/signup")
        .json(&json!({ "email": "nope" }))
        .await
        .assert_validation_errors(&["email"])
        .assert_header("content-type", "application/problem+json");
}

fn tag(name: &'static str) -> impl Middleware {
    move |req: Request, next: Next| async move {
        let res = next.run(req).await;
        res.append_header("x-trace", name)
    }
}

#[tokio::test]
async fn middleware_stacks_and_named_groups() {
    let mut app = App::new();
    app.middleware_group("api", Stack::new().with(tag("a")).with(tag("b")));
    app.middleware_group("empty", Stack::new());
    let inner = Stack::new().with(tag("inner"));
    app.group("/api", |api| {
        api.middleware(middleware::group("api"));
        api.get("/users", |_| async { "users" });
    });
    app.get(
        "/nested",
        (|_| async { "nested" })
            .with(Stack::new().with(tag("outer")).with(inner))
            .with(middleware::group("empty")),
    );
    app.get(
        "/short",
        (|_| async { "never" }).with(
            Stack::new().with(|_req: Request, _next: Next| async { Error::unauthorized("stop") }),
        ),
    );
    app.get(
        "/unknown",
        (|_| async { "never" }).with(middleware::group("nope")),
    );
    let client = TestClient::new(app);

    let res = client.get("/api/users").await;
    res.assert_text("users");
    let traces: Vec<&str> = res
        .headers()
        .get_all("x-trace")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    // The inner middleware adds its header first, on the way back.
    assert_eq!(traces, ["b", "a"]);
    // The 404 under the group's prefix goes through the group too.
    assert!(client.get("/api/nope").await.header("x-trace").is_some());

    let res = client.get("/nested").await;
    res.assert_text("nested");
    let traces: Vec<&str> = res
        .headers()
        .get_all("x-trace")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert_eq!(traces, ["inner", "outer"]);

    client.get("/short").await.assert_status(401);
    client.get("/unknown").await.assert_status(500);
}

#[tokio::test]
async fn etag_and_not_modified() {
    let mut app = App::new();
    app.middleware(middleware::etag());
    app.get("/data", |_| async { Json(json!({ "items": [1, 2, 3] })) });
    app.post("/data", |_| async { "posted" });
    let client = TestClient::new(app);

    let first = client.get("/data").await;
    let etag = first.header("etag").unwrap().to_owned();
    assert!(etag.starts_with("W/\""));
    client
        .get("/data")
        .header("if-none-match", &etag)
        .await
        .assert_status(304)
        .assert_header("etag", &etag)
        .assert_text("");
    client
        .get("/data")
        .header("if-none-match", "\"other\"")
        .await
        .assert_status(200);
    client.post("/data").await.assert_header_missing("etag");
}

#[tokio::test]
async fn response_cache() {
    let calls = Arc::new(AtomicUsize::new(0));
    let cache = middleware::cache(Duration::from_secs(60)).max_entries(2);
    let mut app = App::new();
    let counter = calls.clone();
    app.get(
        "/posts",
        (move |req: Request| {
            let counter = counter.clone();
            async move {
                let n = counter.fetch_add(1, Ordering::SeqCst);
                format!("posts {n} {}", req.query("page").unwrap_or_default())
            }
        })
        .with(cache.clone()),
    );
    app.get(
        "/private",
        (|_| async { res::send("secret").header("cache-control", "private") }).with(cache.clone()),
    );
    app.get(
        "/cookie",
        (|_| async { res::send("hi").cookie(Cookie::new("a", "b")) }).with(cache.clone()),
    );
    let client = TestClient::new(app);

    let miss = client.get("/posts").await;
    miss.assert_text("posts 0 ")
        .assert_header("x-cache", "MISS")
        .assert_header("cache-control", "public, max-age=60");
    let hit = client.get("/posts").await;
    hit.assert_text("posts 0 ").assert_header("x-cache", "HIT");
    assert!(hit.header("age").is_some());
    let etag = hit.header("etag").unwrap().to_owned();
    client
        .get("/posts")
        .header("if-none-match", &etag)
        .await
        .assert_status(304);
    // Another query string is another entry.
    client.get("/posts?page=2").await.assert_text("posts 1 2");
    // Authorization bypasses the cache.
    client
        .get("/posts")
        .bearer("t")
        .await
        .assert_text("posts 2 ")
        .assert_header_missing("x-cache");
    assert_eq!(cache.len(), 2);
    cache.invalidate("/posts");
    client.get("/posts").await.assert_text("posts 3 ");
    // The oldest entry made room.
    assert_eq!(cache.len(), 2);
    cache.invalidate_prefix("/posts");
    assert!(cache.is_empty());

    client
        .get("/private")
        .await
        .assert_header_missing("x-cache");
    client.get("/cookie").await.assert_header_missing("x-cache");
    assert!(cache.is_empty());
}

#[tokio::test]
async fn cache_entries_expire() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let mut app = App::new();
    app.middleware(middleware::cache(Duration::from_millis(50)));
    app.get("/now", move |_| {
        let counter = counter.clone();
        async move { counter.fetch_add(1, Ordering::SeqCst).to_string() }
    });
    let client = TestClient::new(app);
    client.get("/now").await.assert_text("0");
    client.get("/now").await.assert_text("0");
    tokio::time::sleep(Duration::from_millis(80)).await;
    client.get("/now").await.assert_text("1");
}

static COMMENT: TypedRoute<(String, u32)> = TypedRoute::new("/blog/:slug/comments/:id(\\d+)");

#[tokio::test]
async fn typed_routes() {
    let mut app = App::new();
    app.get(COMMENT.pattern(), |req: Request| async move {
        let (slug, id) = COMMENT.params(&req)?;
        Ok::<_, Error>(format!(
            "{slug} #{id} {}",
            COMMENT.url(&req, (slug.clone(), id + 1))?
        ))
    });
    let client = TestClient::new(app);
    client
        .get("/blog/hello%20you/comments/7")
        .header("host", "example.com")
        .await
        .assert_text("hello you #7 http://example.com/blog/hello%20you/comments/8");
    assert_eq!(COMMENT.path(("a/b".into(), 1)), "/blog/a%2Fb/comments/1");
    assert!(std::panic::catch_unwind(|| COMMENT.path(("x".into(), 1))).is_ok());
}

#[tokio::test]
async fn async_and_router_hooks() {
    #[derive(Clone)]
    struct Tenant(String);

    let mut app = App::new();
    app.on_request(|req: &mut Request| {
        req.headers_mut().insert("x-order", "sync".parse().unwrap());
        Ok(())
    });
    app.on_request_async(|mut req: Request| async move {
        tokio::task::yield_now().await;
        if req.header("x-banned").is_some() {
            return Err(Error::forbidden("banned"));
        }
        let order = format!("{},async", req.header("x-order").unwrap_or(""));
        req.headers_mut().insert("x-order", order.parse().unwrap());
        Ok(req)
    });
    app.pre_handler_async(|mut req: Request| async move {
        if let Some(slug) = req.param("tenant").map(str::to_owned) {
            if slug == "unknown" {
                return Err(Error::not_found("no such tenant"));
            }
            req.set(Tenant(slug));
        }
        Ok(req)
    });
    app.get("/t/:tenant", |req: Request| async move {
        format!(
            "{} {}",
            req.get::<Tenant>().map(|t| t.0.clone()).unwrap_or_default(),
            req.header("x-order").unwrap_or("")
        )
    });
    app.group("/admin", |admin| {
        admin.on_request(|req: &mut Request| match req.header("x-admin") {
            Some(_) => Ok(()),
            None => Err(Error::forbidden("admins only")),
        });
        admin.on_response(|res: &mut Response| {
            res.set_header("cache-control", "no-store");
        });
        admin.on_request_async(|req: Request| async move { Ok(req) });
        admin.get("/stats", |_| async { "stats" });
    });
    app.get("/panic", |_| async { "never" });
    app.pre_handler_async(|req: Request| async move {
        if req.path() == "/panic" {
            panic!("hook panic");
        }
        Ok(req)
    });
    let client = TestClient::new(app);

    client.get("/t/acme").await.assert_text("acme sync,async");
    client.get("/t/unknown").await.assert_not_found();
    client
        .get("/t/acme")
        .header("x-banned", "1")
        .await
        .assert_status(403);
    client
        .get("/admin/stats")
        .await
        .assert_status(403)
        .assert_header("cache-control", "no-store");
    client
        .get("/admin/stats")
        .header("x-admin", "1")
        .await
        .assert_text("stats");
    client.get("/panic").await.assert_status(500);
}

#[tokio::test]
async fn agent_keeps_cookies() {
    let mut app = App::new();
    app.post("/login", |_| async {
        res::send("welcome").cookie(Cookie::new("user", "ada"))
    });
    app.post("/logout", |_| async {
        res::send("bye").clear_cookie("user")
    });
    app.get("/me", |req: Request| async move {
        req.cookie("user").unwrap_or("nobody").to_owned()
    });
    app.get("/search", |req: Request| async move {
        format!(
            "{}:{}",
            req.query("q").unwrap_or_default(),
            req.header("authorization").unwrap_or("")
        )
    });
    let client = TestClient::new(app);
    let agent = client.agent();

    agent.get("/me").await.assert_text("nobody");
    agent
        .post("/login")
        .await
        .assert_ok()
        .assert_cookie("user", "ada");
    agent.get("/me").await.assert_text("ada");
    // The plain client has no jar.
    client.get("/me").await.assert_text("nobody");
    client
        .get("/me")
        .cookie("user", "bob")
        .await
        .assert_text("bob");
    agent.post("/logout").await;
    assert_eq!(agent.cookie("user"), None);
    agent.set_cookie("user", "eve");
    agent.get("/me").await.assert_text("eve");
    agent.clear_cookies();
    assert!(agent.cookies().is_empty());

    client
        .get("/search")
        .query(&[("q", "rust web")])
        .bearer("tok")
        .await
        .assert_text("rust web:Bearer tok")
        .assert_see("rust")
        .assert_dont_see("python");
}

#[tokio::test]
async fn json_assertions() {
    let mut app = App::new();
    app.get("/users", |_| async {
        Json(
            json!({ "data": [{ "name": "Ada", "roles": ["admin"] }, { "name": "Bob" }], "a/b": 1 }),
        )
    });
    app.get("/go", |_| async { Redirect::to("/users") });
    let client = TestClient::new(app);
    let res = client.get("/users").await;
    res.assert_json_count("data", 2)
        .assert_json_path("data.0.roles.0", "admin")
        .assert_json_path("/a~1b", 1)
        .assert_json_missing("data.1.roles")
        .assert_json_includes(json!({ "data": [{ "name": "Ada" }, {}] }));
    assert_eq!(res.json_path("data.1.name"), Some(json!("Bob")));
    client.get("/go").await.assert_redirect("/users");

    let failed = std::panic::catch_unwind(|| {
        res.assert_json_path("data.0.name", "Bob");
    });
    let message = *failed.unwrap_err().downcast::<String>().unwrap();
    assert!(
        message.contains("expected \"Bob\" at \"data.0.name\", got \"Ada\""),
        "{message}"
    );
}

#[tokio::test]
async fn pubsub_as_server_sent_events() {
    use http_body_util::BodyExt;
    use vitesse::pubsub::PubSub;

    let pubsub = PubSub::<String>::new();
    let res = pubsub
        .subscribe("news")
        .sse(|title| sse::Event::new().event("news").data(title))
        .no_keep_alive()
        .into_response();
    assert_eq!(res.get_header("content-type"), Some("text/event-stream"));
    let mut body = res.into_body();
    assert_eq!(pubsub.publish("news", "first"), 1);
    let frame = body.frame().await.unwrap().unwrap().into_data().unwrap();
    assert_eq!(&frame[..], b"event: news\ndata: first\n\n");
    drop(body);
    assert_eq!(
        pubsub.subscribers("news"),
        0,
        "the subscription left with the client"
    );
}

#[test]
fn openapi_pages() {
    let mut app = App::new();
    app.get("/pets", |_| async { "[]" });
    app.openapi(
        "/openapi.json",
        OpenApi::new("Pets", "1.0.0")
            .swagger_ui("/docs")
            .swagger_ui_assets("/vendor/swagger/")
            .scalar("/reference")
            .redoc("/redoc"),
    );
    let client = TestClient::new(app);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            client
                .get("/docs")
                .await
                .assert_ok()
                .assert_see("/vendor/swagger/swagger-ui-bundle.js")
                .assert_dont_see("cdn.jsdelivr.net");
            client
                .get("/reference")
                .await
                .assert_see("@scalar/api-reference")
                .assert_see("data-url=\"/openapi.json\"");
            client
                .get("/redoc")
                .await
                .assert_see("<redoc spec-url=\"/openapi.json\">");
            client
                .get("/openapi.json")
                .await
                .assert_json_path("paths./pets.get.responses.200.description", "OK");
        });
    let _ = StatusCode::OK;
}

#[cfg(feature = "schemars")]
#[test]
fn openapi_schemas_from_types() {
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use vitesse::openapi::Operation;
    use vitesse::validate::{Validate, Validator};

    #[allow(dead_code)]
    #[derive(Deserialize, JsonSchema)]
    struct Address {
        city: String,
    }

    /// A new pet.
    #[allow(dead_code)]
    #[derive(Deserialize, JsonSchema)]
    struct NewPet {
        /// The pet's name.
        name: String,
        age: Option<u8>,
        address: Address,
    }

    impl Validate for NewPet {
        fn validate(&self, v: &mut Validator) {
            v.field("name", &self.name).not_empty();
        }
    }

    #[derive(Serialize, JsonSchema)]
    struct Pet {
        id: u64,
        name: String,
    }

    #[allow(dead_code)]
    #[derive(Deserialize, JsonSchema)]
    struct Search {
        /// Words to look for.
        q: String,
        #[serde(default)]
        size: u32,
    }

    let mut app = App::new();
    app.post("/pets", |_| async { StatusCode::CREATED }).doc(
        Operation::new()
            .validated_body::<NewPet>()
            .response_type::<Pet>(201, "The new pet")
            .response_type::<Vec<Pet>>(200, "Pets"),
    );
    app.get("/pets", |_| async { "[]" })
        .doc(Operation::new().query_type::<Search>());
    let doc = app.openapi_document(
        &OpenApi::new("Pets", "1")
            .schema_type::<Pet>()
            .schema("Pet", json!({ "type": "object", "description": "mine" })),
    );
    let post = &doc["paths"]["/pets"]["post"];
    assert_eq!(
        post["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/NewPet"
    );
    assert_eq!(
        post["responses"]["201"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/Pet"
    );
    assert_eq!(
        post["responses"]["200"]["content"]["application/json"]["schema"]["type"],
        "array"
    );
    assert!(post["responses"]["422"]["content"].is_object());
    let schemas = &doc["components"]["schemas"];
    assert_eq!(schemas["NewPet"]["required"], json!(["name", "address"]));
    assert_eq!(
        schemas["NewPet"]["properties"]["name"]["description"],
        "The pet's name."
    );
    assert_eq!(
        schemas["NewPet"]["properties"]["address"]["$ref"],
        "#/components/schemas/Address"
    );
    assert_eq!(schemas["Address"]["properties"]["city"]["type"], "string");
    // The spec's own schema wins.
    assert_eq!(schemas["Pet"]["description"], "mine");
    let params = &doc["paths"]["/pets"]["get"]["parameters"];
    assert_eq!(params[0]["name"], "q");
    assert_eq!(params[0]["required"], true);
    assert_eq!(params[0]["description"], "Words to look for.");
    assert_eq!(params[1]["name"], "size");
    assert_eq!(params[1]["required"], false);
}
