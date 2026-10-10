use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vitesse::prelude::*;
use vitesse::test::TestClient;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct User {
    id: u32,
    name: String,
}

#[tokio::test]
async fn basic_routes_and_return_types() {
    let mut app = App::new();
    app.get("/", |_| async { "Hello World!" })
        .get("/string", |_| async { String::from("owned") })
        .get("/json", |_| async {
            Json(User {
                id: 1,
                name: "Ada".into(),
            })
        })
        .get("/value", |_| async { json!({ "ok": true }) })
        .get("/html", |_| async { Html("<h1>Salut</h1>") })
        .get("/empty", |_| async {})
        .get("/created", |_| async { (201, "créé") })
        .get("/status", |_| async { StatusCode::IM_A_TEAPOT })
        .get("/none", |_| async { None::<&str> })
        .get("/builder", |_| async {
            res::status(202)
                .header("x-custom", "oui")
                .json(json!([1, 2, 3]))
        });

    let c = TestClient::new(app);

    let r = c.get("/").await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.text(), "Hello World!");
    assert_eq!(r.header("content-type"), Some("text/plain; charset=utf-8"));

    assert_eq!(c.get("/string").await.text(), "owned");

    let r = c.get("/json").await;
    assert_eq!(r.header("content-type"), Some("application/json"));
    assert_eq!(
        r.json::<User>(),
        User {
            id: 1,
            name: "Ada".into()
        }
    );

    assert_eq!(c.get("/value").await.text(), r#"{"ok":true}"#);
    assert_eq!(
        c.get("/html").await.header("content-type"),
        Some("text/html; charset=utf-8")
    );

    let r = c.get("/empty").await;
    assert_eq!(r.status(), 200);
    assert!(r.bytes().is_empty());

    let r = c.get("/created").await;
    assert_eq!(r.status(), 201);
    assert_eq!(r.text(), "créé");

    let r = c.get("/status").await;
    assert_eq!(r.status(), 418);
    assert_eq!(r.text(), "I'm a teapot");

    assert_eq!(c.get("/none").await.status(), 404);

    let r = c.get("/builder").await;
    assert_eq!(r.status(), 202);
    assert_eq!(r.header("x-custom"), Some("oui"));
    assert_eq!(r.text(), "[1,2,3]");
}

#[tokio::test]
async fn params_and_query() {
    #[derive(Deserialize)]
    struct Search {
        q: String,
        page: Option<u32>,
    }

    let mut app = App::new();
    app.get("/users/:id", |req: Request| async move {
        let id: u32 = req.param_as("id")?;
        Ok::<_, Error>(format!("user {id}"))
    });
    app.get("/users/:id/posts/:post", |req: Request| async move {
        let pairs: Vec<String> = req.params().map(|(k, v)| format!("{k}={v}")).collect();
        pairs.join(",")
    });
    app.get("/files/*path", |req: Request| async move {
        req.param("path").unwrap().to_owned()
    });
    app.get("/hello/:name", |req: Request| async move {
        req.param("name").unwrap().to_owned()
    });
    app.get("/search", |req: Request| async move {
        let s: Search = req.query_as()?;
        Ok::<_, Error>(format!(
            "{}|{}|{:?}",
            s.q,
            s.page.unwrap_or(1),
            req.query("q")
        ))
    });

    let c = TestClient::new(app);
    assert_eq!(c.get("/users/42").await.text(), "user 42");
    assert_eq!(c.get("/users/42/").await.text(), "user 42");
    let r = c.get("/users/abc").await;
    assert_eq!(r.status(), 400);
    assert!(r.text().contains("invalid parameter"));
    assert_eq!(c.get("/users/1/posts/2").await.text(), "id=1,post=2");
    assert_eq!(c.get("/files/a/b/c.txt").await.text(), "a/b/c.txt");
    assert_eq!(c.get("/hello/Fran%C3%A7ois").await.text(), "François");
    assert_eq!(
        c.get("/search?q=caf%C3%A9+cr%C3%A8me&page=3").await.text(),
        r#"café crème|3|Some("café crème")"#
    );
    assert_eq!(c.get("/search").await.status(), 400);
}

#[tokio::test]
async fn bodies() {
    #[derive(Deserialize)]
    struct Login {
        user: String,
        remember: bool,
    }

    let mut app = App::new();
    app.post("/users", |req: Request| async move {
        let mut user: User = req.json().await?;
        user.id += 1;
        Ok::<_, Error>((201, Json(user)))
    });
    app.post("/login", |req: Request| async move {
        let form: Login = req.form().await?;
        Ok::<_, Error>(format!("{}:{}", form.user, form.remember))
    });
    app.post("/echo", |req: Request| async move {
        // Le corps est mis en cache : on peut le relire.
        let a = req.text().await?;
        let b = req.bytes().await?;
        Ok::<_, Error>(format!("{a}|{}", b.len()))
    });
    app.post("/stream", |req: Request| async move { req.take_body() });
    app.body_limit(64);

    let c = TestClient::new(app);
    let r = c
        .post("/users")
        .json(&User {
            id: 1,
            name: "Ada".into(),
        })
        .await;
    assert_eq!(r.status(), 201);
    assert_eq!(
        r.json::<User>(),
        User {
            id: 2,
            name: "Ada".into()
        }
    );

    let r = c.post("/users").body("{pas du json").await;
    assert_eq!(r.status(), 400);
    assert!(
        r.json::<serde_json::Value>()["error"]
            .as_str()
            .unwrap()
            .starts_with("invalid JSON")
    );

    let r = c
        .post("/login")
        .form(&[("user", "ada"), ("remember", "true")])
        .await;
    assert_eq!(r.text(), "ada:true");

    assert_eq!(c.post("/echo").body("salut").await.text(), "salut|5");
    assert_eq!(c.post("/echo").body("x".repeat(65)).await.status(), 413);
    assert_eq!(c.post("/stream").body("flux").await.text(), "flux");
}

#[tokio::test]
async fn not_found_method_not_allowed_options_head() {
    let mut app = App::new();
    app.get("/items", |_| async { "liste" });
    app.post("/items", |_| async { "créé" });

    let c = TestClient::new(app);

    let r = c.get("/nope").await;
    assert_eq!(r.status(), 404);
    assert_eq!(r.text(), r#"{"error":"Cannot GET /nope"}"#);

    let r = c.delete("/items").await;
    assert_eq!(r.status(), 405);
    assert_eq!(r.header("allow"), Some("GET, HEAD, POST, OPTIONS"));

    let r = c.request(Method::OPTIONS, "/items").await;
    assert_eq!(r.status(), 204);
    assert_eq!(r.header("allow"), Some("GET, HEAD, POST, OPTIONS"));

    let r = c.request(Method::HEAD, "/items").await;
    assert_eq!(r.status(), 200);
}

#[tokio::test]
async fn custom_fallback() {
    let mut app = App::new();
    app.fallback(|req: Request| async move { (404, format!("rien à {}", req.path())) });
    let c = TestClient::new(app);
    let r = c.get("/x").await;
    assert_eq!(r.status(), 404);
    assert_eq!(r.text(), "rien à /x");
}

#[tokio::test]
async fn middleware_order_and_short_circuit() {
    let log = Arc::new(Mutex::new(Vec::<String>::new()));

    let mut app = App::new();
    let l = log.clone();
    app.middleware(move |req: Request, next: Next| {
        let l = l.clone();
        async move {
            l.lock().unwrap().push("a:avant".into());
            let res = next.run(req).await;
            l.lock().unwrap().push("a:après".into());
            res.header("x-a", "1")
        }
    });
    let l = log.clone();
    app.middleware(move |mut req: Request, next: Next| {
        let l = l.clone();
        async move {
            l.lock().unwrap().push("b:avant".into());
            if req.header("x-block").is_some() {
                return Error::forbidden("bloqué").into_response();
            }
            req.set(String::from("donnée du middleware"));
            next.run(req).await
        }
    });
    let l = log.clone();
    app.get("/", move |req: Request| {
        let l = l.clone();
        async move {
            l.lock().unwrap().push("handler".into());
            req.get::<String>().cloned().unwrap_or_default()
        }
    });

    let c = TestClient::new(app);
    let r = c.get("/").await;
    assert_eq!(r.text(), "donnée du middleware");
    assert_eq!(r.header("x-a"), Some("1"));
    assert_eq!(
        *log.lock().unwrap(),
        ["a:avant", "b:avant", "handler", "a:après"]
    );

    log.lock().unwrap().clear();
    let r = c.get("/").header("x-block", "1").await;
    assert_eq!(r.status(), 403);
    assert_eq!(r.header("x-a"), Some("1"));
    assert_eq!(*log.lock().unwrap(), ["a:avant", "b:avant", "a:après"]);

    // Les middlewares globaux s'appliquent aussi aux 404.
    assert_eq!(c.get("/nope").await.header("x-a"), Some("1"));
}

async fn auth(req: Request, next: Next) -> Response {
    match req.header("authorization") {
        Some("Bearer ok") => next.run(req).await,
        _ => Error::unauthorized("connectez-vous").into_response(),
    }
}

async fn tag(req: Request, next: Next) -> Response {
    next.run(req).await.header("x-tag", "1")
}

#[tokio::test]
async fn routers_and_route_middleware() {
    let mut users = Router::new();
    users.get("/", |_| async { "tous" });
    users.get("/:id", |req: Request| async move {
        format!("user {}", req.param("id").unwrap())
    });

    let mut api = Router::new();
    api.middleware(auth);
    api.mount("/users", users);
    api.get("/me", |_| async { "moi" });

    let mut app = App::new();
    app.mount("/api", api);
    app.get("/public", |_| async { "public" });
    app.get("/tagged", (|_| async { "ok" }).with(auth).with(tag));

    let c = TestClient::new(app);
    assert_eq!(c.get("/public").await.text(), "public");
    assert_eq!(c.get("/api/me").await.status(), 401);
    let ok = |path: &str| c.get(path).header("authorization", "Bearer ok");
    assert_eq!(ok("/api/me").await.text(), "moi");
    assert_eq!(ok("/api/users").await.text(), "tous");
    assert_eq!(ok("/api/users/7").await.text(), "user 7");

    assert_eq!(c.get("/tagged").await.status(), 401);
    let r = ok("/tagged").await;
    assert_eq!(r.text(), "ok");
    assert_eq!(r.header("x-tag"), Some("1"));
}

#[tokio::test]
async fn state_cookies_redirects() {
    let mut app = App::new();
    app.state(AtomicUsize::new(0));
    app.get("/count", |req: Request| async move {
        let n = req.state::<AtomicUsize>().fetch_add(1, Ordering::Relaxed) + 1;
        n.to_string()
    });
    app.get("/whoami", |req: Request| async move {
        req.cookie("session").unwrap_or("anonyme").to_owned()
    });
    app.get("/login", |_| async {
        Response::new()
            .cookie(Cookie::new("session", "abc").http_only(true))
            .text("ok")
    });
    app.get("/old", |_| async { Redirect::permanent("/new") });
    app.get("/go", |_| async { res::redirect("/ailleurs") });
    app.get("/missing-state", |req: Request| async move {
        req.state::<String>().clone()
    });

    let c = TestClient::new(app);
    assert_eq!(c.get("/count").await.text(), "1");
    assert_eq!(c.get("/count").await.text(), "2");
    assert_eq!(c.get("/whoami").await.text(), "anonyme");
    assert_eq!(
        c.get("/whoami")
            .header("cookie", "a=1; session=xyz")
            .await
            .text(),
        "xyz"
    );
    assert_eq!(
        c.get("/login").await.header("set-cookie"),
        Some("session=abc; Path=/; HttpOnly")
    );

    let r = c.get("/old").await;
    assert_eq!(r.status(), 301);
    assert_eq!(r.header("location"), Some("/new"));
    assert_eq!(c.get("/go").await.status(), 302);

    // Un état manquant panique : converti en 500.
    assert_eq!(c.get("/missing-state").await.status(), 500);
}

#[tokio::test]
async fn errors_and_panics() {
    let mut app = App::new();
    app.middleware(|req: Request, next: Next| async move {
        next.run(req).await.header("x-cors", "garde-moi")
    });
    app.on_error(|err: Error| res::status(err.status()).html(format!("<p>{}</p>", err.message())));
    app.get("/io", |_| async {
        let n: u32 = "pas un nombre".parse()?;
        Ok::<_, Error>(n.to_string())
    });
    app.get("/panic", |_| async {
        if true {
            panic!("boum");
        }
        "jamais"
    });
    app.get("/teapot", |_| async {
        Error::new(418, "je suis une théière")
    });

    let c = TestClient::new(app);

    let r = c.get("/io").await;
    assert_eq!(r.status(), 500);
    // Le détail de l'erreur interne n'est pas exposé.
    assert_eq!(r.text(), "<p>Internal Server Error</p>");

    let r = c.get("/panic").await;
    assert_eq!(r.status(), 500);
    assert_eq!(r.header("x-cors"), Some("garde-moi"));

    let r = c.get("/teapot").await;
    assert_eq!(r.status(), 418);
    assert_eq!(r.text(), "<p>je suis une théière</p>");
    assert_eq!(r.header("content-type"), Some("text/html; charset=utf-8"));

    assert_eq!(c.get("/nope").await.text(), "<p>Cannot GET /nope</p>");
}

#[tokio::test]
async fn builtin_middlewares() {
    let mut app = App::new();
    app.middleware(middleware::helmet());
    app.middleware(
        middleware::cors()
            .allow_origin("https://ok.fr")
            .max_age(Duration::from_secs(60)),
    );
    app.middleware(middleware::timeout(Duration::from_millis(50)));
    app.get("/", |_| async { "ok" });
    app.get("/slow", |_| async {
        tokio::time::sleep(Duration::from_secs(5)).await;
        "trop tard"
    });

    let c = TestClient::new(app);

    let r = c.get("/").header("origin", "https://ok.fr").await;
    assert_eq!(
        r.header("access-control-allow-origin"),
        Some("https://ok.fr")
    );
    assert_eq!(r.header("x-content-type-options"), Some("nosniff"));

    let r = c.get("/").header("origin", "https://pirate.fr").await;
    assert_eq!(r.header("access-control-allow-origin"), None);

    let r = c
        .request(Method::OPTIONS, "/")
        .header("origin", "https://ok.fr")
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "content-type")
        .await;
    assert_eq!(r.status(), 204);
    assert_eq!(
        r.header("access-control-allow-headers"),
        Some("content-type")
    );
    assert_eq!(r.header("access-control-max-age"), Some("60"));

    assert_eq!(c.get("/slow").await.status(), 503);
}

#[tokio::test]
async fn cors_credentials_never_echo_unknown_origins() {
    // Sans origine déclarée, les cookies ne doivent pas ouvrir la porte à
    // n'importe quel site : la réponse dit `*`, que les navigateurs refusent
    // avec des identifiants.
    let mut open = App::new();
    open.middleware(middleware::cors().allow_credentials(true));
    open.get("/", |_| async { "ok" });
    let r = TestClient::new(open)
        .get("/")
        .header("origin", "https://pirate.fr")
        .await;
    assert_eq!(r.header("access-control-allow-origin"), Some("*"));

    let mut strict = App::new();
    strict.middleware(
        middleware::cors()
            .allow_origin("https://ok.fr")
            .allow_credentials(true),
    );
    strict.get("/", |_| async { "ok" });
    let c = TestClient::new(strict);
    let r = c.get("/").header("origin", "https://ok.fr").await;
    assert_eq!(
        r.header("access-control-allow-origin"),
        Some("https://ok.fr")
    );
    assert_eq!(r.header("access-control-allow-credentials"), Some("true"));
    assert_eq!(r.header("vary"), Some("Origin"));
    let r = c.get("/").header("origin", "https://pirate.fr").await;
    assert_eq!(r.header("access-control-allow-origin"), None);
}

#[tokio::test]
async fn static_files() {
    let dir = std::env::temp_dir().join(format!("vitesse-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("hello.txt"), "bonjour le monde").unwrap();
    std::fs::write(dir.join("style.css"), "body{}").unwrap();
    std::fs::write(dir.join("sub/index.html"), "<p>index</p>").unwrap();
    std::fs::write(dir.join(".env"), "SECRET=1").unwrap();
    let big: Vec<u8> = (0..600_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(dir.join("big.bin"), &big).unwrap();

    let mut app = App::new();
    app.static_dir("/static", &dir);
    app.middleware(ServeDir::new(&dir));
    app.get("/api", |_| async { "api" });
    app.get("/download", {
        let dir = dir.clone();
        move |_| {
            let path = dir.join("hello.txt");
            async move { res::download(path, "salut.txt").await }
        }
    });

    let c = TestClient::new(app);

    let r = c.get("/static/hello.txt").await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.text(), "bonjour le monde");
    assert_eq!(r.header("content-type"), Some("text/plain; charset=utf-8"));
    let etag = r.header("etag").unwrap().to_owned();

    let r = c
        .get("/static/hello.txt")
        .header("if-none-match", &etag)
        .await;
    assert_eq!(r.status(), 304);

    let r = c.get("/static/hello.txt").header("range", "bytes=8-").await;
    assert_eq!(r.status(), 206);
    assert_eq!(r.text(), "le monde");
    assert_eq!(r.header("content-range"), Some("bytes 8-15/16"));
    assert_eq!(
        c.get("/static/hello.txt")
            .header("range", "bytes=99-")
            .await
            .status(),
        416
    );

    // Gros fichier envoyé en flux, et plage au milieu.
    let r = c.get("/static/big.bin").await;
    assert_eq!(r.bytes().as_ref(), &big[..]);
    let r = c
        .get("/static/big.bin")
        .header("range", "bytes=300000-300009")
        .await;
    assert_eq!(r.bytes().as_ref(), &big[300_000..300_010]);

    assert_eq!(c.get("/static/sub/").await.text(), "<p>index</p>");
    let r = c.get("/static/sub").await;
    assert_eq!(r.status(), 301);
    assert_eq!(r.header("location"), Some("/static/sub/"));

    // Sécurité : pas de sortie du dossier, pas de fichiers cachés.
    assert_eq!(c.get("/static/../Cargo.toml").await.status(), 404);
    assert_eq!(
        c.get("/static/%2e%2e/%2e%2e/etc/passwd").await.status(),
        404
    );
    assert_eq!(c.get("/static/.env").await.status(), 404);
    assert_eq!(c.get("/static/nope.txt").await.status(), 404);

    // En middleware : sert si le fichier existe, sinon continue vers les routes.
    assert_eq!(c.get("/style.css").await.text(), "body{}");
    assert_eq!(c.get("/api").await.text(), "api");
    assert_eq!(c.get("/.env").await.status(), 404);

    let r = c.get("/download").await;
    assert_eq!(
        r.header("content-disposition"),
        Some("attachment; filename=\"salut.txt\"")
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
#[should_panic(expected = "duplicate route")]
fn duplicate_route_panics() {
    let mut app = App::new();
    app.get("/a", |_| async { "1" });
    app.get("/a", |_| async { "2" });
}

#[test]
#[should_panic(expected = "invalid route")]
fn invalid_route_panics() {
    let mut app = App::new();
    app.get("pas-de-slash", |_| async { "1" });
}

#[tokio::test]
async fn utf8_header_values_are_read_the_same_way_everywhere() {
    // Que la table des en-têtes soit construite ou non, une valeur UTF-8
    // valide est lue de la même façon.
    let mut app = App::new();
    app.get("/lazy", |req: Request| async move {
        req.header("x-name").unwrap_or("none").to_owned()
    });
    app.get("/map", |req: Request| async move {
        let _ = req.headers().len();
        req.header("x-name").unwrap_or("none").to_owned()
    });
    let server = app.bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());
    for path in ["/lazy", "/map"] {
        let res = raw(
            addr,
            &format!("GET {path} HTTP/1.1\r\nhost: x\r\nx-name: José\r\nconnection: close\r\n\r\n"),
        )
        .await;
        assert!(res.ends_with("José"), "{path}: {res}");
    }
}

#[tokio::test]
async fn panics_in_middleware_go_through_on_error_and_outer_middleware() {
    let mut app = App::new();
    app.middleware(
        |req: Request, next: Next| async move { next.run(req).await.header("x-outer", "1") },
    );
    app.middleware(|req: Request, next: Next| async move {
        if req.path() == "/boom" {
            panic!("boum dans un middleware");
        }
        next.run(req).await
    });
    app.on_error(|err: Error| res::status(err.status()).text(format!("custom: {}", err.message())));
    app.get("/boom", |_| async { "jamais" });
    app.get("/ok", |_| async { "ok" });

    let c = TestClient::new(app);
    let r = c.get("/boom").await;
    assert_eq!(r.status(), 500);
    assert_eq!(r.header("x-outer"), Some("1"));
    assert_eq!(r.text(), "custom: Internal Server Error");
    assert_eq!(c.get("/ok").await.text(), "ok");
}

#[tokio::test]
async fn router_middleware_covers_its_whole_prefix() {
    let mut admin = Router::new();
    admin.middleware(|req: Request, next: Next| async move {
        next.run(req).await.header("x-admin", "1")
    });
    admin.get("/stats", |_| async { "stats" });

    let mut api = Router::new();
    api.middleware(
        |req: Request, next: Next| async move { next.run(req).await.header("x-api", "1") },
    );
    api.middleware(middleware::cors());
    api.get("/users", |_| async { "users" });
    api.mount("/admin", admin);

    let mut app = App::new();
    app.mount("/api", api);
    app.get("/other", |_| async { "other" });
    let c = TestClient::new(app);

    // Une route du routeur : comme avant.
    let r = c.get("/api/users").await;
    assert_eq!((r.status().as_u16(), r.header("x-api")), (200, Some("1")));
    // Chemin inconnu sous le préfixe : le middleware passe, puis le 404.
    let r = c.get("/api/nope").await;
    assert_eq!(r.status(), 404);
    assert_eq!(r.header("x-api"), Some("1"));
    assert_eq!(r.header("x-admin"), None);
    assert_eq!(c.get("/api").await.header("x-api"), Some("1"));
    // Mauvaise méthode : 405 avec `Allow`, après le middleware.
    let r = c.post("/api/users").await;
    assert_eq!(r.status(), 405);
    assert_eq!(r.header("x-api"), Some("1"));
    assert_eq!(r.header("allow"), Some("GET, HEAD, OPTIONS"));
    // Pré-vol CORS sur une route du routeur : le middleware CORS répond.
    let r = c
        .request(Method::OPTIONS, "/api/users")
        .header("origin", "https://app.example")
        .header("access-control-request-method", "POST")
        .await;
    assert_eq!(r.status(), 204);
    assert_eq!(r.header("access-control-allow-origin"), Some("*"));
    // Routeurs imbriqués : les deux chaînes, l'extérieure d'abord.
    let r = c.get("/api/admin/nope").await;
    assert_eq!(r.status(), 404);
    assert_eq!(
        (r.header("x-api"), r.header("x-admin")),
        (Some("1"), Some("1"))
    );
    let r = c.get("/api/admin/stats").await;
    assert_eq!(r.text(), "stats");
    assert_eq!(
        (r.header("x-api"), r.header("x-admin")),
        (Some("1"), Some("1"))
    );
    // Hors du préfixe : pas de middleware du routeur.
    assert_eq!(c.get("/apix").await.header("x-api"), None);
    assert_eq!(c.get("/other").await.header("x-api"), None);
}

#[tokio::test]
async fn send_file_answers_304_and_206_like_express() {
    let dir = std::env::temp_dir().join(format!("vitesse-sendfile-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("digits.txt");
    std::fs::write(&file, "0123456789").unwrap();

    let mut app = App::new();
    let path = file.clone();
    app.get("/f", move |_| {
        let path = path.clone();
        async move { res::file(path).await }
    });
    let path = file.clone();
    app.get("/d", move |_| {
        let path = path.clone();
        async move { res::download(path, "chiffres.txt").await }
    });
    let c = TestClient::new(app);

    let r = c.get("/f").await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.text(), "0123456789");
    let etag = r.header("etag").unwrap().to_owned();
    let last_modified = r.header("last-modified").unwrap().to_owned();

    let r = c.get("/f").header("if-none-match", &etag).await;
    assert_eq!(r.status(), 304);
    assert!(r.bytes().is_empty());
    let r = c
        .get("/f")
        .header("if-modified-since", &last_modified)
        .await;
    assert_eq!(r.status(), 304);

    let r = c.get("/f").header("range", "bytes=2-5").await;
    assert_eq!(r.status(), 206);
    assert_eq!(r.header("content-range"), Some("bytes 2-5/10"));
    assert_eq!(r.text(), "2345");
    assert_eq!(c.get("/f").header("range", "bytes=50-").await.status(), 416);

    // `If-Range` : la plage n'est servie que si le fichier n'a pas changé.
    let r = c
        .get("/f")
        .header("range", "bytes=0-1")
        .header("if-range", "Wed, 21 Oct 2015 07:28:00 GMT")
        .await;
    assert_eq!(
        (r.status().as_u16(), r.text().as_str()),
        (200, "0123456789")
    );
    let r = c
        .get("/f")
        .header("range", "bytes=0-1")
        .header("if-range", &last_modified)
        .await;
    assert_eq!((r.status().as_u16(), r.text().as_str()), (206, "01"));

    let r = c.get("/d").header("range", "bytes=8-").await;
    assert_eq!(r.status(), 206);
    assert_eq!(r.text(), "89");
    assert_eq!(
        r.header("content-disposition"),
        Some("attachment; filename=\"chiffres.txt\"")
    );

    // Par le vrai moteur HTTP : les en-têtes sont repérés à l'analyse.
    let mut app = App::new();
    let path = file.clone();
    app.get("/f", move |_| {
        let path = path.clone();
        async move { res::file(path).await }
    });
    let server = app.bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());
    let res = raw(
        addr,
        "GET /f HTTP/1.1\r\nhost: x\r\nrange: bytes=7-\r\nconnection: close\r\n\r\n",
    )
    .await;
    assert!(res.starts_with("HTTP/1.1 206"), "{res}");
    assert!(res.ends_with("789"), "{res}");
    let res = raw(
        addr,
        &format!(
            "GET /f HTTP/1.1\r\nhost: x\r\nif-none-match: {etag}\r\nconnection: close\r\n\r\n"
        ),
    )
    .await;
    assert!(res.starts_with("HTTP/1.1 304"), "{res}");

    std::fs::remove_dir_all(&dir).ok();
}

/// Envoie des requêtes HTTP brutes sur une connexion TCP.
async fn raw(addr: std::net::SocketAddr, request: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    stream.read_to_end(&mut out).await.unwrap();
    String::from_utf8_lossy(&out).into_owned()
}

#[tokio::test]
async fn real_server_over_tcp() {
    let mut app = App::new();
    app.get("/", |_| async { "Hello World!" });
    app.get("/ip", |req: Request| async move {
        req.ip().unwrap().to_string()
    });
    app.post("/echo", |req: Request| async move { req.text().await });
    let dir = std::env::temp_dir().join(format!("vitesse-tcp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.txt"), "0123456789").unwrap();
    let big = vec![b'x'; 1_000_000];
    std::fs::write(dir.join("big.bin"), &big).unwrap();
    app.static_dir("/s", &dir);

    let server = app.bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(server.with_graceful_shutdown(async {
        stop_rx.await.ok();
    }));

    let res = raw(
        addr,
        "GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(res.starts_with("HTTP/1.1 200 OK\r\n"), "{res}");
    assert!(res.contains("content-length: 12\r\n"), "{res}");
    assert!(res.ends_with("\r\n\r\nHello World!"), "{res}");

    // HEAD : mêmes en-têtes, pas de corps.
    let res = raw(
        addr,
        "HEAD / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(res.contains("content-length: 12\r\n"), "{res}");
    assert!(res.ends_with("\r\n\r\n"), "{res}");

    // Pipelining : trois requêtes d'un coup sur la même connexion.
    let res = raw(
        addr,
        "GET / HTTP/1.1\r\nHost: x\r\n\r\n\
         GET /ip HTTP/1.1\r\nHost: x\r\n\r\n\
         POST /echo HTTP/1.1\r\nHost: x\r\nContent-Length: 5\r\nConnection: close\r\n\r\nsalut",
    )
    .await;
    assert_eq!(res.matches("HTTP/1.1 200 OK").count(), 3, "{res}");
    assert!(res.contains("127.0.0.1"), "{res}");
    assert!(res.ends_with("salut"), "{res}");

    // Corps envoyé en chunked.
    let res = raw(
        addr,
        "POST /echo HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n\
         3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n",
    )
    .await;
    assert!(res.ends_with("abcde"), "{res}");

    // HEAD sur un fichier statique : la taille sans le contenu.
    let res = raw(
        addr,
        "HEAD /s/a.txt HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(res.contains("content-length: 10\r\n"), "{res}");
    assert!(res.ends_with("\r\n\r\n"), "{res}");

    // Gros fichier envoyé en flux, avec sa taille exacte.
    let res = raw(
        addr,
        "GET /s/big.bin HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(
        res.contains("content-length: 1000000\r\n"),
        "{}",
        &res[..300]
    );
    assert_eq!(res.split("\r\n\r\n").nth(1).unwrap().len(), 1_000_000);
    std::fs::remove_dir_all(&dir).ok();

    stop_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

/// Un flux de trois morceaux, pour tester les réponses `chunked`.
struct Chunks(u8);

impl futures_core::Stream for Chunks {
    type Item = Result<String, std::io::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.0 += 1;
        std::task::Poll::Ready((self.0 <= 3).then(|| Ok(format!("morceau{}", self.0))))
    }
}

/// Envoie `request`, puis lit la réponse jusqu'à la fermeture.
async fn raw_bytes(addr: std::net::SocketAddr, request: &[u8]) -> String {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(request).await.unwrap();
    let mut out = Vec::new();
    stream.read_to_end(&mut out).await.unwrap();
    String::from_utf8_lossy(&out).into_owned()
}

#[tokio::test]
async fn engine_edge_cases() {
    let mut app = App::new();
    app.post("/len", |req: Request| async move {
        let body = req.bytes().await?;
        Ok::<_, Error>(body.len().to_string())
    });
    app.post("/ignore", |_| async { "ignoré" });
    app.get("/slow", |_| async {
        tokio::time::sleep(Duration::from_millis(30)).await;
        "lent"
    });
    app.get("/fast", |_| async { "rapide" });
    app.get("/stream", |_| async { Body::from_stream(Chunks(0)) });
    app.get("/json", |_| async { json!({ "ok": true }) });
    app.get("/empty", |_| async { StatusCode::NO_CONTENT });
    app.body_limit(10 * 1024 * 1024);

    let server = app.bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());

    // Gros corps (> 64 Kio) : transmis au handler en flux.
    let big = "x".repeat(300_000);
    let req = format!(
        "POST /len HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{big}",
        big.len()
    );
    let res = raw_bytes(addr, req.as_bytes()).await;
    assert!(
        res.ends_with("\r\n\r\n300000"),
        "{}",
        &res[..200.min(res.len())]
    );

    // Gros corps en chunked.
    let mut req = String::from(
        "POST /len HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
    );
    for _ in 0..10 {
        req.push_str(&format!("{:x}\r\n{}\r\n", 20_000, "y".repeat(20_000)));
    }
    req.push_str("0\r\n\r\n");
    let res = raw_bytes(addr, req.as_bytes()).await;
    assert!(res.ends_with("\r\n\r\n200000"), "{res}");

    // Un handler qui ignore un gros corps : on répond puis on ferme.
    let req = format!(
        "POST /ignore HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{big}",
        big.len()
    );
    let res = raw_bytes(addr, req.as_bytes()).await;
    assert!(res.starts_with("HTTP/1.1 200 OK\r\n"), "{res}");
    assert!(res.contains("connection: close\r\n"), "{res}");

    // Expect: 100-continue.
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let body = "z".repeat(100_000);
    let head = format!(
        "POST /len HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await.unwrap();
    let mut buf = [0u8; 25];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"HTTP/1.1 100 Continue\r\n\r\n");
    stream.write_all(body.as_bytes()).await.unwrap();
    let mut out = String::new();
    stream.read_to_string(&mut out).await.unwrap();
    assert!(out.ends_with("100000"), "{out}");

    // HTTP/1.0 : fermeture par défaut, keep-alive sur demande.
    let res = raw_bytes(addr, b"GET /fast HTTP/1.0\r\n\r\n").await;
    assert!(
        res.contains("connection: close\r\n") && res.ends_with("rapide"),
        "{res}"
    );
    let res = raw_bytes(
        addr,
        b"GET /fast HTTP/1.0\r\nConnection: keep-alive\r\n\r\nGET /fast HTTP/1.0\r\n\r\n",
    )
    .await;
    assert!(res.contains("connection: keep-alive\r\n"), "{res}");
    assert_eq!(res.matches("rapide").count(), 2, "{res}");

    // Requêtes invalides.
    let res = raw_bytes(addr, b"BLA BLA\r\n\r\n").await;
    assert!(res.starts_with("HTTP/1.1 400 Bad Request\r\n"), "{res}");
    let huge = format!("GET / HTTP/1.1\r\nX-Big: {}\r\n\r\n", "a".repeat(70_000));
    let res = raw_bytes(addr, huge.as_bytes()).await;
    assert!(
        res.starts_with("HTTP/1.1 431 "),
        "{}",
        &res[..60.min(res.len())]
    );
    let res = raw_bytes(
        addr,
        b"POST /len HTTP/1.1\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\n",
    )
    .await;
    assert!(res.starts_with("HTTP/1.1 400 "), "{res}");

    // Pipelining avec un handler lent : l'ordre des réponses est respecté.
    let res = raw_bytes(
        addr,
        b"GET /slow HTTP/1.1\r\nHost: x\r\n\r\nGET /fast HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await;
    let (slow, fast) = (res.find("lent").unwrap(), res.find("rapide").unwrap());
    assert!(slow < fast, "{res}");

    // Réponse en flux (taille inconnue) : chunked.
    let res = raw_bytes(addr, b"GET /stream HTTP/1.1\r\nConnection: close\r\n\r\n").await;
    assert!(res.contains("transfer-encoding: chunked\r\n"), "{res}");
    assert!(
        res.ends_with("\r\n\r\n8\r\nmorceau1\r\n8\r\nmorceau2\r\n8\r\nmorceau3\r\n0\r\n\r\n"),
        "{res}"
    );

    // HEAD sur une route JSON, et 204 sans corps ni longueur.
    let res = raw_bytes(addr, b"HEAD /json HTTP/1.1\r\nConnection: close\r\n\r\n").await;
    assert!(
        res.contains("content-length: 11\r\n") && res.ends_with("\r\n\r\n"),
        "{res}"
    );
    let res = raw_bytes(addr, b"GET /empty HTTP/1.1\r\nConnection: close\r\n\r\n").await;
    assert!(
        res.starts_with("HTTP/1.1 204 ") && !res.contains("content-length"),
        "{res}"
    );
}

#[tokio::test]
async fn graceful_shutdown_finishes_requests() {
    let mut app = App::new();
    app.get("/slow", |_| async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        "fini"
    });
    let server = app.bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(server.with_graceful_shutdown(async {
        stop_rx.await.ok();
    }));

    // Une connexion inactive, qui ne doit pas bloquer l'arrêt.
    let _idle = tokio::net::TcpStream::connect(addr).await.unwrap();
    let slow = tokio::spawn(raw(addr, "GET /slow HTTP/1.1\r\nHost: x\r\n\r\n"));
    tokio::time::sleep(Duration::from_millis(50)).await;
    stop_tx.send(()).unwrap();

    let res = slow.await.unwrap();
    assert!(res.starts_with("HTTP/1.1 200 OK\r\n"), "{res}");
    assert!(
        res.contains("connection: close\r\n") && res.ends_with("fini"),
        "{res}"
    );
    tokio::time::timeout(Duration::from_secs(3), handle)
        .await
        .expect("le serveur doit s'arrêter")
        .unwrap()
        .unwrap();
    assert!(tokio::net::TcpStream::connect(addr).await.is_err());
}
