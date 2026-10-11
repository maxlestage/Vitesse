//! Cookies signés et sessions : signature, cycle de vie d'une session
//! (création, lecture, régénération, destruction) et stockage maison.
#![cfg(feature = "session")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use vitesse::prelude::*;
use vitesse::session::{MemoryStore, Record, SessionStore};
use vitesse::test::{TestClient, TestResponse};
use vitesse::{BoxError, BoxFuture};

const SECRET: &str = "0123456789abcdef0123456789abcdef-test";

/// The `name=value` part of the `Set-Cookie` header.
fn cookie_of(res: &TestResponse) -> Option<String> {
    res.header("set-cookie")
        .map(|c| c.split(';').next().unwrap().to_owned())
}

#[tokio::test]
async fn signed_cookies() {
    let mut app = App::new();
    app.cookie_secret(SECRET);
    app.get("/set", |req: Request| async move {
        res::send_status(204).cookie(req.sign_cookie(Cookie::new("theme", "dark")))
    });
    app.get("/read", |req: Request| async move {
        req.signed_cookie("theme").unwrap_or_else(|| "none".into())
    });
    let c = TestClient::new(app);

    let set = c.get("/set").await;
    let cookie = cookie_of(&set).unwrap();
    assert!(cookie.starts_with("theme=dark."), "{cookie}");
    assert_eq!(
        c.get("/read").header("cookie", &cookie).await.text(),
        "dark"
    );
    // Modifié par le client : ignoré.
    let forged = cookie.replacen("dark", "light", 1);
    assert_eq!(
        c.get("/read").header("cookie", &forged).await.text(),
        "none"
    );
    assert_eq!(
        c.get("/read").header("cookie", "theme=dark").await.text(),
        "none"
    );
}

fn app(store: impl SessionStore) -> App {
    let mut app = App::new();
    app.trust_proxy(true);
    app.middleware(middleware::session(store).max_age(Duration::from_secs(3600)));
    app.get("/", |_| async { "anonymous" });
    app.get("/visits", |req: Request| async move {
        let session = req.session();
        let visits = session.get::<u32>("visits").unwrap_or(0) + 1;
        session.insert("visits", visits)?;
        Ok::<_, Error>(format!("visit #{visits}"))
    });
    app.post("/login", |req: Request| async move {
        let session = req.session();
        session.regenerate();
        session.insert("user", "ada")?;
        Ok::<_, Error>("logged in")
    });
    app.get("/me", |req: Request| async move {
        req.session()
            .get::<String>("user")
            .unwrap_or_else(|| "nobody".into())
    });
    app.post("/logout", |req: Request| async move {
        req.session().destroy();
        "bye"
    });
    app
}

#[tokio::test]
async fn session_lifecycle() {
    let c = TestClient::new(app(MemoryStore::new()));

    // Pas de session pour qui ne fait que passer.
    assert!(c.get("/").await.header("set-cookie").is_none());

    let first = c.get("/visits").await;
    assert_eq!(first.text(), "visit #1");
    let set = first.header("set-cookie").unwrap();
    assert!(set.contains("HttpOnly"), "{set}");
    assert!(set.contains("SameSite=Lax"), "{set}");
    assert!(set.contains("Max-Age=3600"), "{set}");
    assert!(!set.contains("Secure"), "{set}");
    let cookie = cookie_of(&first).unwrap();
    assert!(cookie.starts_with("vitesse.sid="));

    let second = c.get("/visits").header("cookie", &cookie).await;
    assert_eq!(second.text(), "visit #2");
    assert_eq!(cookie_of(&second).unwrap(), cookie, "same session id");
    // Lire sans modifier ne renvoie pas de cookie.
    assert!(
        c.get("/me")
            .header("cookie", &cookie)
            .await
            .header("set-cookie")
            .is_none()
    );

    // Connexion : nouvel identifiant, données conservées.
    let login = c.post("/login").header("cookie", &cookie).await;
    let logged = cookie_of(&login).unwrap();
    assert_ne!(logged, cookie);
    assert_eq!(c.get("/me").header("cookie", &logged).await.text(), "ada");
    assert_eq!(
        c.get("/visits").header("cookie", &logged).await.text(),
        "visit #3"
    );
    // L'ancien identifiant ne vaut plus rien.
    assert_eq!(
        c.get("/me").header("cookie", &cookie).await.text(),
        "nobody"
    );

    // Déconnexion : session supprimée, cookie effacé.
    let logout = c.post("/logout").header("cookie", &logged).await;
    assert!(logout.header("set-cookie").unwrap().contains("Max-Age=0"));
    assert_eq!(
        c.get("/me").header("cookie", &logged).await.text(),
        "nobody"
    );

    // Un identifiant inventé ouvre une session vide.
    let r = c
        .get("/me")
        .header("cookie", "vitesse.sid=AAAAAAAAAAAAAAAAAAAAAA")
        .await;
    assert_eq!(r.text(), "nobody");

    // Derrière un proxy en HTTPS, le cookie est `Secure`.
    let r = c.get("/visits").header("x-forwarded-proto", "https").await;
    assert!(r.header("set-cookie").unwrap().contains("Secure"));
}

#[tokio::test]
async fn session_cookies_are_signed_with_the_secret() {
    let mut app = app(MemoryStore::new());
    app.cookie_secret(SECRET);
    let c = TestClient::new(app);
    let cookie = cookie_of(&c.get("/visits").await).unwrap();
    let (_, value) = cookie.split_once('=').unwrap();
    assert!(value.contains('.'), "{value}");
    assert_eq!(
        c.get("/visits").header("cookie", &cookie).await.text(),
        "visit #2"
    );
    // L'identifiant seul, sans signature, est refusé.
    let unsigned = cookie.split('.').next().unwrap();
    assert_eq!(
        c.get("/visits").header("cookie", unsigned).await.text(),
        "visit #1"
    );
}

/// A store of our own, that counts its calls.
#[derive(Default, Clone)]
struct Counting {
    inner: Arc<MemoryStore>,
    calls: Arc<Mutex<Vec<&'static str>>>,
}

impl SessionStore for Counting {
    fn load(&self, id: &str) -> BoxFuture<Result<Option<Record>, BoxError>> {
        self.calls.lock().unwrap().push("load");
        self.inner.load(id)
    }

    fn save(&self, id: &str, record: &Record, ttl: Duration) -> BoxFuture<Result<(), BoxError>> {
        self.calls.lock().unwrap().push("save");
        self.inner.save(id, record, ttl)
    }

    fn delete(&self, id: &str) -> BoxFuture<Result<(), BoxError>> {
        self.calls.lock().unwrap().push("delete");
        self.inner.delete(id)
    }
}

#[tokio::test]
async fn custom_stores() {
    let store = Counting::default();
    let calls = store.calls.clone();
    let c = TestClient::new(app(store));
    let cookie = cookie_of(&c.get("/visits").await).unwrap();
    c.get("/me").header("cookie", &cookie).await;
    c.post("/logout").header("cookie", &cookie).await;
    assert_eq!(*calls.lock().unwrap(), ["save", "load", "load", "delete"]);
}

#[tokio::test]
async fn session_without_the_middleware_is_a_500() {
    let mut app = App::new();
    app.get("/", |req: Request| async move {
        let _ = req.session();
        "unreachable"
    });
    assert_eq!(TestClient::new(app).get("/").await.status(), 500);
}

#[cfg(feature = "ws")]
#[tokio::test]
async fn the_session_is_readable_in_a_websocket_handler() {
    use futures_util::StreamExt;

    let mut app = app(MemoryStore::new());
    app.ws("/ws", |req, mut socket| async move {
        let user = req
            .session()
            .get::<String>("user")
            .unwrap_or_else(|| "nobody".into());
        let _ = socket.send(user).await;
    });
    let server = app.bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());

    // Connexion en HTTP, puis WebSocket avec le cookie de session.
    let mut tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tcp.write_all(
        b"POST /login HTTP/1.1\r\nhost: x\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
    )
    .await
    .unwrap();
    let mut out = String::new();
    tcp.read_to_string(&mut out).await.unwrap();
    let cookie = out
        .lines()
        .find_map(|l| l.strip_prefix("set-cookie: "))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();

    let mut request =
        tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(format!(
            "ws://{addr}/ws"
        ))
        .unwrap();
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let message = ws.next().await.unwrap().unwrap();
    assert_eq!(message.into_text().unwrap().as_str(), "ada");
}
