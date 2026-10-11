# Signed cookies and sessions

A cookie keeps a little data in the visitor's browser, which sends it back with every request. With the `session` feature, Vitesse can do two more things with cookies: **sign** them, so that visitors can read them but not change them (Express's `cookie-parser` with a secret), and keep **sessions**, data stored on the server for each visitor while the browser only holds a random id (Express's `express-session`).

## Enable the `session` feature

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["session"] }
```

It adds `app.cookie_secret(...)`, `req.sign_cookie(...)`, `req.signed_cookie(...)`, `middleware::session(...)`, `req.session()` and the `vitesse::session` module.

## The secret

Signing needs a secret: a long random string that only the server knows. Generate one, once:

```sh
openssl rand -base64 48
```

Then pass it to the app through an environment variable:

```rust
let mut app = App::new();
app.cookie_secret(std::env::var("COOKIE_SECRET").expect("COOKIE_SECRET is not set"));
```

- The secret must be at least 32 bytes long: a shorter one makes `cookie_secret` panic at startup.
- Keep it out of the code and out of Git. Anyone who knows it can forge your cookies.
- Every instance of the app needs the same secret. Changing it invalidates every cookie signed with the old one: visitors lose their signed cookies and their sessions, as if they had logged out.

## Signed cookies

`req.sign_cookie(cookie)` signs a cookie before you send it with `.cookie(...)`, and `req.signed_cookie(name)` reads it back and checks the signature:

```rust
use std::time::Duration;
use vitesse::prelude::*;

let mut app = App::new();
app.cookie_secret(std::env::var("COOKIE_SECRET").expect("COOKIE_SECRET is not set"));

app.post("/theme/:name", |req: Request| async move {
    let theme = req.param("name").unwrap_or("light").to_owned();
    let cookie = Cookie::new("theme", theme).max_age(Duration::from_secs(365 * 24 * 3600));
    res::send_status(204).cookie(req.sign_cookie(cookie))
});

app.get("/", |req: Request| async move {
    // `None` if the cookie is missing, or if the visitor changed it.
    let theme = req.signed_cookie("theme").unwrap_or_else(|| "light".into());
    format!("theme: {theme}")
});
```

The cookie goes out as `theme=dark.<signature>`: the value, a dot, then an HMAC-SHA256 of the cookie's name and value computed with the secret. `req.signed_cookie("theme")` computes it again: when it matches, you get `Some("dark")`, without the signature; otherwise you get `None`, as if there were no cookie. `req.cookie("theme")` still returns the raw value, signature included.

What signing guarantees:

- **The value comes from you.** A visitor who changes `dark` to `light` doesn't have the matching signature, and the cookie is ignored.
- **The signature covers the name too**, so the value of one cookie can't be passed off as another.

What it doesn't do:

- **Hide the value.** Signed isn't encrypted: anyone can read `dark` in the browser's developer tools. Don't put secrets or personal data in a cookie, signed or not.
- **Expire it or take it back.** `Max-Age` is only applied by the browser: someone who kept a copy of a signed cookie can send it again later, and it stays valid for as long as the secret doesn't change. For anything that must end on demand, a login for instance, use a [session](#sessions).

`sign_cookie` panics (so the request gets a `500`) when the app has no secret, and `signed_cookie` returns `None`. The value is written as is: keep it to letters, digits and `-_.`. For anything larger or structured, use a session.

## Sessions

A session keeps data on the server for each visitor: who is logged in, a cart, a message to show on the next page. The browser only holds a cookie, `vitesse.sid`, containing a random id that it sends back with each request. Add the `session` middleware, then read and write the session with `req.session()`:

```rust
use vitesse::prelude::*;
use vitesse::session::MemoryStore;

let mut app = App::new();
app.middleware(middleware::session(MemoryStore::new()));

app.get("/visits", |req: Request| async move {
    let session = req.session();
    let visits = session.get::<u32>("visits").unwrap_or(0) + 1;
    session.insert("visits", visits)?;
    Ok::<_, Error>(format!("visit #{visits}"))
});
```

Each browser counts its own visits. A session holds named values, stored as JSON: any type that implements `Serialize` can go in, and come out as any type that implements `Deserialize`.

| Method | `express-session` | Effect |
|---|---|---|
| `session.get::<T>(key)` | `req.session.key` | The value as an `Option<T>`; `None` if it's missing or of another type |
| `session.insert(key, value)?` | `req.session.key = value` | Stores a value; a `Result`, an `Err` (`500`) if it doesn't serialise |
| `session.remove(key)` | `delete req.session.key` | Removes a value and returns it (`Option<serde_json::Value>`) |
| `session.contains(key)` | `'key' in req.session` | Whether the session holds that key |
| `session.clear()` | | Removes every value, keeping the session |
| `session.is_empty()` | | Whether the session holds no value |
| `session.regenerate()` | `req.session.regenerate()` | A new id, the same data: call it at login |
| `session.destroy()` | `req.session.destroy()` | Deletes the session, in the store and in the browser: logout |
| `session.id()` | `req.sessionID` | The id, an `Option<String>`: `None` until the session is first saved |

- Add the middleware with `app.middleware(...)`, so that every route has a session. `req.session()` panics (and the request gets a `500`) on a route the middleware doesn't cover.
- `get` returns an owned value: ask for a `String`, not a `&str`.
- `Session` is `Clone`, and a clone is a handle on the same session.
- In a [WebSocket](websocket.md#parameters-headers-and-state) handler (`app.ws`), `req.session()` holds the session as it was when the connection was accepted. Changes made there aren't saved, because the middleware has already sent its response (the `101`): change the session in a regular route.

### Logging in and out

```rust
use serde::Deserialize;
use vitesse::prelude::*;
use vitesse::session::MemoryStore;

#[derive(Deserialize)]
struct Login {
    email: String,
    password: String,
}

async fn login(req: Request) -> vitesse::Result<Redirect> {
    let form: Login = req.form().await?;
    let Some(user_id) = check_password(&form.email, &form.password).await else {
        return Err(Error::unauthorized("wrong email or password"));
    };
    let session = req.session();
    session.regenerate(); // a new id: the one the visitor had until now is worthless
    session.insert("user_id", user_id)?;
    Ok(Redirect::see_other("/account"))
}

async fn account(req: Request) -> vitesse::Result<String> {
    let user_id: u64 = req
        .session()
        .get("user_id")
        .ok_or_else(|| Error::unauthorized("please log in"))?;
    Ok(format!("Hello, user #{user_id}"))
}

async fn logout(req: Request) -> Redirect {
    req.session().destroy();
    Redirect::see_other("/")
}

let mut app = App::new();
app.middleware(middleware::session(MemoryStore::new()));
app.post("/login", login);
app.get("/account", account);
app.post("/logout", logout);
```

`check_password` is your own function: it looks the user up and verifies the password hash (with argon2 or bcrypt, for instance), returning `Option<u64>`.

- **`regenerate()` at login** defeats session fixation: an attacker who got the victim to use a session id they know (on a shared computer, through a subdomain) can't use it once the victim has logged in. The data is kept, a cart for instance.
- **`destroy()` at logout** deletes the session from the store, so a copy of the cookie is worthless, and tells the browser to delete the cookie.
- Logout is a `POST`: a `GET` can be triggered by any link or image on another site.
- After `destroy()`, the session stays destroyed until the end of the request: a value inserted afterwards isn't saved. To show a message after logging out, put it in the redirect's URL (`/?bye`) or in a plain cookie.

To protect a group of routes, check the session in a [middleware](middleware.md) and pass the user on to the handlers:

```rust
#[derive(Clone)]
struct UserId(u64);

async fn require_login(mut req: Request, next: Next) -> Response {
    match req.session().get::<u64>("user_id") {
        Some(id) => {
            req.set(UserId(id));
            next.run(req).await
        }
        None => Redirect::to("/login").into_response(),
    }
}

let mut account = Router::new();
account.middleware(require_login);
account.get("/", |req: Request| async move {
    let user = req.get::<UserId>().unwrap();
    format!("Hello, user #{}", user.0)
});
app.mount("/account", account);
```

Global middleware runs before router middleware, so `require_login` finds the session opened by `app.middleware(middleware::session(...))`.

### Options

`middleware::session(store)` returns a builder:

```rust
use std::time::Duration;
use vitesse::SameSite;

app.middleware(
    middleware::session(MemoryStore::new())
        .cookie_name("sid")
        .max_age(Duration::from_secs(30 * 24 * 3600)) // 30 days
        .rolling(true)
        .same_site(SameSite::Strict),
);
```

| Option | Default | Effect |
|---|---|---|
| `.cookie_name(name)` | `"vitesse.sid"` | The cookie's name (`name` in express-session) |
| `.max_age(duration)` | 1 day | How long a session lives after its last change, in the store and in the browser (`cookie.maxAge`) |
| `.rolling(true)` | `false` | Extends the session on every request, so that it ends after `max_age` of inactivity; it costs one store write per request (`rolling`) |
| `.secure(bool)` | `Secure` when the request came over HTTPS | Forces the `Secure` attribute on or off (`cookie.secure`) |
| `.same_site(SameSite::Strict)` | `SameSite::Lax` | The `SameSite` attribute (`cookie.sameSite`) |
| `.domain("example.com")` | none: this host only | The `Domain` attribute, to share the session with subdomains (`cookie.domain`) |
| `.path("/app")` | `/` | The `Path` attribute (`cookie.path`) |

### When the cookie is sent

A session is saved, and its cookie sent, only when a handler changes it: `insert`, `remove` of a value that was there, `clear` of a session that wasn't empty, `regenerate`. With `rolling(true)`, an existing session is also saved on every request.

- A visitor who only reads pages gets no cookie and takes no room in the store, like `saveUninitialized: false` in express-session.
- Reading the session doesn't send the cookie again, and doesn't extend the session either (unless `rolling(true)`).
- The session is saved when the response leaves the middleware, once the handler (and the middlewares after it) have returned. A change made later, in a task started with `tokio::spawn`, isn't saved.
- An id that the store doesn't know (expired, deleted, invented by the client) opens an empty session, which gets a new id if something is written to it: Vitesse never adopts an id chosen by the client.
- Two simultaneous requests from the same visitor each work on their own copy of the session, and the last one to finish wins, as with express-session. Keep sessions small, and keep data that changes often in your database.

### Cookie attributes

The session cookie is:

- **`HttpOnly`**: the page's JavaScript can't read it, so an XSS flaw can't steal the session.
- **`SameSite=Lax`** by default: the browser leaves it out of the requests that other sites trigger, except plain links (see [CSRF](#csrf)).
- **`Secure` when the request arrived over HTTPS**: the browser then only sends it back over HTTPS. Behind a reverse proxy that terminates TLS, the app receives plain HTTP: declare the proxy with [`app.trust_proxy`](requests.md#behind-a-proxy-trust_proxy), so that its `X-Forwarded-Proto: https` counts, or force the attribute with `.secure(true)`.
- **Signed** when the app has a [`cookie_secret`](#the-secret). The id is already 128 random bits from the operating system, impossible to guess; the signature lets Vitesse reject a forged id without asking the store.

> [!IMPORTANT]
> Over plain HTTP, anyone on the same network (a public Wi-Fi) can read the session cookie and use it. In production, serve the app over HTTPS: see [HTTPS and HTTP/2](https.md) and [Going to production](production.md).

## Stores

### `MemoryStore`

`MemoryStore::new()` keeps sessions in the memory of the process. It needs nothing, which makes it ideal for development, tests and small apps, but:

- sessions are lost when the server restarts: each deployment logs everyone out;
- they aren't shared between instances: behind a load balancer, a visitor logged in on one server isn't logged in on the others;
- expired sessions are removed from time to time, as new ones arrive.

### Your own store

For Redis, PostgreSQL or any other database, implement the three methods of the `SessionStore` trait. A session's data is a `Record`, a `serde_json::Map`, easy to save as JSON text. Here is the shape of a SQL store, with a `HashMap` standing in for the table so that the example runs as is; the comments give the matching queries:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use vitesse::session::{Record, SessionStore};
use vitesse::tokio::sync::RwLock;
use vitesse::{BoxError, BoxFuture, serde_json};

/// Stands in for a table `sessions (id TEXT PRIMARY KEY, data TEXT, expires_at TIMESTAMP)`.
/// A real store holds a connection pool here (`sqlx::PgPool`, `deadpool_redis::Pool`…).
#[derive(Clone, Default)]
struct SqlStore {
    table: Arc<RwLock<HashMap<String, (String, SystemTime)>>>,
}

impl SessionStore for SqlStore {
    fn load(&self, id: &str) -> BoxFuture<Result<Option<Record>, BoxError>> {
        let (table, id) = (self.table.clone(), id.to_owned());
        Box::pin(async move {
            // SELECT data FROM sessions WHERE id = $1 AND expires_at > now()
            let table = table.read().await;
            match table.get(&id) {
                Some((data, expires_at)) if *expires_at > SystemTime::now() => {
                    Ok(Some(serde_json::from_str(data)?))
                }
                _ => Ok(None),
            }
        })
    }

    fn save(&self, id: &str, record: &Record, ttl: Duration) -> BoxFuture<Result<(), BoxError>> {
        let (table, id) = (self.table.clone(), id.to_owned());
        let data = serde_json::to_string(record);
        Box::pin(async move {
            // INSERT INTO sessions (id, data, expires_at) VALUES ($1, $2, now() + $3)
            // ON CONFLICT (id) DO UPDATE SET data = $2, expires_at = now() + $3
            table.write().await.insert(id, (data?, SystemTime::now() + ttl));
            Ok(())
        })
    }

    fn delete(&self, id: &str) -> BoxFuture<Result<(), BoxError>> {
        let (table, id) = (self.table.clone(), id.to_owned());
        Box::pin(async move {
            // DELETE FROM sessions WHERE id = $1
            table.write().await.remove(&id);
            Ok(())
        })
    }
}

let mut app = App::new();
app.middleware(middleware::session(SqlStore::default()));
```

- Each method returns a `BoxFuture` that must own its data (`'static`): clone the id and the pool, and serialise the record, **before** the `async` block.
- `load` runs for each request that carries a session cookie, `save` when the session changed, and `delete` on `destroy()` and `regenerate()`.
- `?` turns any error into a `BoxError`. An error from `load` or `save` gives the request a `500` (the cause is logged); an error from `delete` is only logged.
- With Redis, `SET id data EX ttl`, `GET id` and `DEL id` are all you need, and Redis forgets expired sessions by itself. In a SQL table, delete the expired rows from time to time, in a task started with the app.

### tower-sessions

If you already use [tower-sessions](https://crates.io/crates/tower-sessions) and its stores, it works too, through the `tower` feature: see [Sessions with tower-sessions](from-axum-actix.md#sessions-with-tower-sessions). Its API is async (`session.get(...).await?`), and it ignores `app.cookie_secret`.

## CSRF

Browsers attach cookies to requests automatically, including requests triggered by another site: a form on another site that posts to your `/transfer` route would carry your visitor's session. That's cross-site request forgery (CSRF).

`SameSite=Lax`, the session cookie's default, blocks most of it: browsers don't send the cookie with a `POST`, `PUT`, `PATCH` or `DELETE` coming from another site, nor with its `fetch` calls or images, only with plain navigations (links), which are `GET` requests. Your part is to make sure that a `GET` never changes anything.

Add a CSRF token when that isn't enough: when you switch to `SameSite::None` (the app runs in an iframe on another site, or a payment provider posts forms back to it), when your subdomains don't all trust each other (they are the "same site"), or when older browsers matter. Keep a random token in the session, put it in a hidden field of each form, and compare the two on every `POST`:

```rust
use serde::Deserialize;
use vitesse::prelude::*;

/// The CSRF token of this session, created on first use.
fn csrf_token(req: &Request) -> vitesse::Result<String> {
    let session = req.session();
    if let Some(token) = session.get::<String>("csrf") {
        return Ok(token);
    }
    let token = format!("{:032x}", rand::random::<u128>()); // cargo add rand
    session.insert("csrf", &token)?;
    Ok(token)
}

app.get("/transfer", |req: Request| async move {
    let token = csrf_token(&req)?;
    Ok::<_, Error>(Html(format!(
        r#"<form method="post"><input type="hidden" name="csrf" value="{token}">…</form>"#
    )))
});

#[derive(Deserialize)]
struct Transfer {
    csrf: String,
    amount: u64,
}

app.post("/transfer", |req: Request| async move {
    let form: Transfer = req.form().await?;
    let expected = req.session().get::<String>("csrf").unwrap_or_default();
    if expected.is_empty() || !middleware::constant_time_eq(form.csrf.as_bytes(), expected.as_bytes()) {
        return Err(Error::forbidden("invalid CSRF token"));
    }
    Ok(format!("{} transferred", form.amount))
});
```

With [templates](templates.md), pass the token in the context: `req.render("transfer.html", json!({ "csrf": csrf_token(&req)? }))`.

## Testing

`TestClient` doesn't keep cookies between requests: read the `Set-Cookie` header of a response, and send its `name=value` part back in a `cookie` header, as a browser would:

```rust
use vitesse::test::{TestClient, TestResponse};

/// The `name=value` part of the response's `Set-Cookie` header.
fn cookie_of(res: &TestResponse) -> String {
    let set_cookie = res.header("set-cookie").expect("no Set-Cookie header");
    set_cookie.split(';').next().unwrap().to_owned()
}

#[tokio::test]
async fn counts_visits_per_visitor() {
    let client = TestClient::new(my_api::app());

    let first = client.get("/visits").await;
    assert_eq!(first.text(), "visit #1");
    let cookie = cookie_of(&first); // "vitesse.sid=…"

    let second = client.get("/visits").header("cookie", &cookie).await;
    assert_eq!(second.text(), "visit #2");

    // Another visitor, without the cookie, starts over.
    assert_eq!(client.get("/visits").await.text(), "visit #1");
}
```

The same goes for signed cookies, and changing the value is a good way to check that a forged cookie is refused: `cookie.replacen("dark", "light", 1)`. More in [Testing](testing.md#testing-sessions).
