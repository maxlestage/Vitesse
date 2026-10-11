//! Interopérabilité tower : de vraies couches tower-http et tower comme
//! middlewares Vitesse, et une application Vitesse comme `Service`.
#![cfg(feature = "tower")]

use std::time::{Duration, Instant};

use http::{HeaderName, HeaderValue};
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::validate_request::ValidateRequestHeaderLayer;
use tower05::ServiceExt;
use vitesse::prelude::*;
use vitesse::test::TestClient;

#[tokio::test]
// Le type d'erreur est imposé par `ValidateRequestHeaderLayer::custom`.
#[allow(clippy::result_large_err)]
async fn tower_http_layers_as_middlewares() {
    let mut app = App::new();
    app.middleware(middleware::layer(SetResponseHeaderLayer::overriding(
        HeaderName::from_static("x-powered-by"),
        HeaderValue::from_static("Vitesse + tower"),
    )));
    app.on_error(|err: Error| (err.status(), format!("custom: {}", err.message())));
    app.get("/", |_| async { "home" });
    app.get("/missing", |_| async { Error::not_found("nothing here") });
    let mut api = Router::new();
    api.middleware(middleware::layer(ValidateRequestHeaderLayer::custom(
        |req: &mut http::Request<Body>| {
            if req.headers().get("x-token").is_some_and(|t| t == "secret") {
                Ok(())
            } else {
                Err(res::status(401).text("no token").into_http())
            }
        },
    )));
    api.middleware(middleware::layer(CorsLayer::permissive()));
    api.post("/echo", |req: Request| async move {
        let body = req.bytes().await?;
        Ok::<_, Error>(body)
    });
    app.mount("/api", api);
    app.post(
        "/small",
        (|req: Request| async move {
            let body = req.bytes().await?;
            Ok::<_, Error>(format!("{} bytes", body.len()))
        })
        .with(middleware::layer(RequestBodyLimitLayer::new(8))),
    );
    let c = TestClient::new(app);

    let r = c.get("/").await;
    assert_eq!(r.text(), "home");
    assert_eq!(r.header("x-powered-by"), Some("Vitesse + tower"));
    // L'erreur traverse la couche tower jusqu'à `on_error`.
    let r = c.get("/missing").await;
    assert_eq!(r.status(), 404);
    assert_eq!(r.text(), "custom: nothing here");
    assert_eq!(r.header("x-powered-by"), Some("Vitesse + tower"));

    let r = c.post("/api/echo").body("hello").await;
    assert_eq!(r.status(), 401);
    assert_eq!(r.text(), "no token");
    let r = c
        .post("/api/echo")
        .header("x-token", "secret")
        .header("origin", "https://example.com")
        .body("hello")
        .await;
    assert_eq!(r.text(), "hello");
    assert_eq!(r.header("access-control-allow-origin"), Some("*"));

    assert_eq!(c.post("/small").body("1234").await.text(), "4 bytes");
    assert_eq!(c.post("/small").body("123456789").await.status(), 413);
}

#[tokio::test]
async fn layers_can_change_the_request() {
    let mut app = App::new();
    app.middleware(middleware::layer(tower05::util::MapRequestLayer::new(
        |mut req: http::Request<Body>| {
            req.headers_mut()
                .insert("x-added", HeaderValue::from_static("by tower"));
            if req.uri().path() == "/old" {
                *req.uri_mut() = "/new?from=old".parse().unwrap();
                *req.method_mut() = Method::POST;
            }
            req
        },
    )));
    app.post("/new", |req: Request| async move {
        format!(
            "{} {} {:?} {}",
            req.method(),
            req.path(),
            req.query("from"),
            req.header("x-added").unwrap_or("-")
        )
    });
    let c = TestClient::new(app);
    assert_eq!(
        c.get("/old").await.text(),
        r#"POST /new Some("old") by tower"#
    );
}

#[tokio::test]
async fn the_service_is_shared_between_requests() {
    let mut app = App::new();
    app.get(
        "/slow",
        (|_| async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            "ok"
        })
        .with(middleware::layer(
            tower05::limit::ConcurrencyLimitLayer::new(1),
        )),
    );
    let c = TestClient::new(app);
    let start = Instant::now();
    let requests: Vec<_> = (0..3)
        .map(|_| tokio::spawn(c.get("/slow").send()))
        .collect();
    for r in requests {
        assert_eq!(r.await.unwrap().text(), "ok");
    }
    // Une seule à la fois : les clones du service partagent la limite.
    assert!(start.elapsed() >= Duration::from_millis(300));
}

#[tokio::test]
async fn an_app_is_a_tower_service() {
    let mut app = App::new();
    app.get("/hello/:name", |req: Request| async move {
        format!(
            "hello {} on {}",
            req.param("name").unwrap(),
            req.host().unwrap_or("-")
        )
    });
    app.post("/echo", |req: Request| async move {
        let body = req.text().await?;
        Ok::<_, Error>(body)
    });
    let service = app.into_service();

    let req = http::Request::get("http://example.com/hello/ada")
        .body(Body::empty())
        .unwrap();
    let res = service.oneshot(req).await.unwrap();
    assert_eq!(res.status(), 200);
    let body = res.into_body().to_bytes().await.unwrap();
    assert_eq!(body, "hello ada on example.com");

    let req = http::Request::post("/echo")
        .body(http_body_util::Full::new(bytes::Bytes::from("ping")))
        .unwrap();
    let res = service.oneshot(req).await.unwrap();
    assert_eq!(res.into_body().to_bytes().await.unwrap(), "ping");

    let req = http::Request::get("/nope").body(Body::empty()).unwrap();
    assert_eq!(service.oneshot(req).await.unwrap().status(), 404);
}
