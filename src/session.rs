//! Signed cookies and sessions (`cookie-parser` with a secret and
//! `express-session` in Express, axum-extra's `SignedCookieJar` and
//! tower-sessions, actix-session). Needs the `session` feature.
//!
//! ## Signed cookies
//!
//! With a secret ([`App::cookie_secret`](crate::App::cookie_secret)), a
//! cookie can be signed: the client can read it but not change it.
//!
//! ```
//! use vitesse::prelude::*;
//!
//! let mut app = App::new();
//! app.cookie_secret("a secret of at least 32 bytes, from the environment");
//! app.post("/theme/:name", |req: Request| async move {
//!     let theme = req.param("name").unwrap_or("light").to_owned();
//!     res::send_status(204).cookie(req.sign_cookie(Cookie::new("theme", theme)))
//! });
//! app.get("/", |req: Request| async move {
//!     // `None` if the cookie is missing or was tampered with.
//!     req.signed_cookie("theme").unwrap_or_else(|| "light".into())
//! });
//! ```
//!
//! ## Sessions
//!
//! [`middleware::session`](crate::middleware::session) keeps data per visitor on the
//! server, in a [`SessionStore`] ([`MemoryStore`] by default); the browser
//! only holds a random id in a cookie. Handlers read and write it with
//! [`Request::session`](crate::Request::session):
//!
//! ```
//! use vitesse::prelude::*;
//! use vitesse::session::MemoryStore;
//!
//! let mut app = App::new();
//! app.middleware(middleware::session(MemoryStore::new()));
//! app.get("/visits", |req: Request| async move {
//!     let session = req.session();
//!     let visits = session.get::<u32>("visits").unwrap_or(0) + 1;
//!     session.insert("visits", visits)?;
//!     Ok::<_, Error>(format!("visit #{visits}"))
//! });
//! ```

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use hmac::{KeyInit, Mac};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::body::BoxError;
use crate::error::Error;
use crate::handler::{BoxFuture, Middleware, Next};
use crate::request::Request;
use crate::response::{Cookie, IntoResponse, Response, SameSite};

type HmacSha256 = hmac::Hmac<sha2::Sha256>;

/// The secret that signs cookies (see [`App::cookie_secret`](crate::App::cookie_secret)).
#[derive(Clone)]
pub(crate) struct Key(Arc<[u8]>);

impl Key {
    /// # Panics
    /// If the secret is shorter than 32 bytes.
    #[track_caller]
    pub(crate) fn new(secret: &[u8]) -> Key {
        assert!(
            secret.len() >= 32,
            "the cookie secret must be at least 32 bytes long (got {})",
            secret.len()
        );
        Key(secret.into())
    }

    fn mac(&self, name: &str, value: &str) -> HmacSha256 {
        let mut mac = HmacSha256::new_from_slice(&self.0).expect("HMAC accepts any key length");
        mac.update(name.as_bytes());
        mac.update(b"=");
        mac.update(value.as_bytes());
        mac
    }

    /// `value.signature`, the signature covering the name and the value.
    pub(crate) fn sign(&self, name: &str, value: &str) -> String {
        let tag = self.mac(name, value).finalize().into_bytes();
        format!("{value}.{}", base64url(&tag))
    }

    /// The value, if the signature is valid (checked in constant time).
    pub(crate) fn verify<'a>(&self, name: &str, signed: &'a str) -> Option<&'a str> {
        let (value, tag) = signed.rsplit_once('.')?;
        let tag = base64url_decode(tag)?;
        self.mac(name, value).verify_slice(&tag).ok()?;
        Some(value)
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Key(..)")
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64 "URL safe", without padding: safe in a cookie value.
fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
        }
    }
    out
}

fn base64url_decode(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| ALPHABET.iter().position(|&a| a == c).map(|v| v as u32);
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for chunk in text.as_bytes().chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= value(c)? << (18 - 6 * i);
        }
        out.extend_from_slice(&n.to_be_bytes()[1..chunk.len()]);
    }
    Some(out)
}

/// A new session id: 128 random bits from the operating system.
fn new_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("the operating system's random generator failed");
    base64url(&bytes)
}

/// The data of a session, as stored.
pub type Record = Map<String, Value>;

/// Where sessions are kept: in memory ([`MemoryStore`]), or in a database
/// of yours (Redis, PostgreSQL…) by implementing these three methods.
///
/// Every future must own its data (`'static`): clone the id and the record
/// before the `async` block.
pub trait SessionStore: Send + Sync + 'static {
    /// The record of a session, or `None` if it doesn't exist (or expired).
    fn load(&self, id: &str) -> BoxFuture<Result<Option<Record>, BoxError>>;

    /// Creates or replaces a session, which expires after `ttl`.
    fn save(&self, id: &str, record: &Record, ttl: Duration) -> BoxFuture<Result<(), BoxError>>;

    /// Deletes a session.
    fn delete(&self, id: &str) -> BoxFuture<Result<(), BoxError>>;
}

/// Sessions in the memory of the process: for development, tests, or a
/// single instance whose sessions may be lost on restart.
#[derive(Default)]
pub struct MemoryStore {
    sessions: Mutex<HashMap<String, (Record, Instant)>>,
}

impl fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryStore")
            .field("sessions", &self.lock().len())
            .finish()
    }
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        MemoryStore::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, (Record, Instant)>> {
        self.sessions.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl SessionStore for MemoryStore {
    fn load(&self, id: &str) -> BoxFuture<Result<Option<Record>, BoxError>> {
        let now = Instant::now();
        let record = self
            .lock()
            .get(id)
            .filter(|(_, expires)| *expires > now)
            .map(|(record, _)| record.clone());
        Box::pin(std::future::ready(Ok(record)))
    }

    fn save(&self, id: &str, record: &Record, ttl: Duration) -> BoxFuture<Result<(), BoxError>> {
        let now = Instant::now();
        let mut sessions = self.lock();
        // De temps en temps, on oublie les sessions expirées.
        if sessions.len() >= 1024 && sessions.len().is_power_of_two() {
            sessions.retain(|_, (_, expires)| *expires > now);
        }
        sessions.insert(id.to_owned(), (record.clone(), now + ttl));
        Box::pin(std::future::ready(Ok(())))
    }

    fn delete(&self, id: &str) -> BoxFuture<Result<(), BoxError>> {
        self.lock().remove(id);
        Box::pin(std::future::ready(Ok(())))
    }
}

/// The session of a request (`req.session` in Express), from
/// [`Request::session`](crate::Request::session). Cloning it gives another
/// handle on the same session.
///
/// The session is saved when the response leaves the middleware: it stays
/// readable afterwards (in a WebSocket handler, for example), but changes
/// made then are not saved.
#[derive(Clone)]
pub struct Session {
    state: Arc<Mutex<State>>,
}

struct State {
    id: Option<String>,
    record: Record,
    changed: bool,
    destroyed: bool,
    regenerate: bool,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.lock();
        f.debug_struct("Session")
            .field("keys", &state.record.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl Session {
    fn new(id: Option<String>, record: Record) -> Self {
        Session {
            state: Arc::new(Mutex::new(State {
                id,
                record,
                changed: false,
                destroyed: false,
                regenerate: false,
            })),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A value, deserialized; `None` if it is missing or of another type.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let value = self.lock().record.get(key)?.clone();
        serde_json::from_value(value).ok()
    }

    /// Stores a value (anything that serializes to JSON).
    pub fn insert<T: Serialize>(&self, key: &str, value: T) -> Result<(), Error> {
        let value = serde_json::to_value(value)
            .map_err(|e| Error::internal("session serialization failed").with_source(e))?;
        let mut state = self.lock();
        state.record.insert(key.to_owned(), value);
        state.changed = true;
        Ok(())
    }

    /// Removes a value, returning it.
    pub fn remove(&self, key: &str) -> Option<Value> {
        let mut state = self.lock();
        let removed = state.record.remove(key);
        state.changed |= removed.is_some();
        removed
    }

    /// Whether the session holds a value for this key.
    pub fn contains(&self, key: &str) -> bool {
        self.lock().record.contains_key(key)
    }

    /// Removes every value, keeping the session.
    pub fn clear(&self) {
        let mut state = self.lock();
        state.changed |= !state.record.is_empty();
        state.record.clear();
    }

    /// Whether the session holds no value.
    pub fn is_empty(&self) -> bool {
        self.lock().record.is_empty()
    }

    /// Deletes the session, in the store and in the browser (log out).
    pub fn destroy(&self) {
        let mut state = self.lock();
        state.destroyed = true;
        state.record.clear();
    }

    /// Gives the session a new id, keeping its data: call it when the user
    /// logs in, so that an id planted before (session fixation) is useless.
    pub fn regenerate(&self) {
        let mut state = self.lock();
        state.regenerate = true;
        state.changed = true;
    }

    /// The id of the session, if it is already saved.
    pub fn id(&self) -> Option<String> {
        self.lock().id.clone()
    }
}

/// Server-side sessions (`express-session`): see the [module](crate::session)
/// documentation. The session is saved, and its cookie sent, only when a
/// handler changes it: anonymous visitors get no cookie.
///
/// The cookie is `HttpOnly`, `SameSite=Lax`, signed when the application
/// has a [`cookie_secret`](crate::App::cookie_secret), and `Secure` when the
/// request arrived over HTTPS (see [`App::trust_proxy`](crate::App::trust_proxy)
/// behind a proxy).
pub fn session<S: SessionStore>(store: S) -> Sessions {
    Sessions {
        store: Box::new(store),
        cookie: "vitesse.sid".into(),
        max_age: Duration::from_secs(24 * 3600),
        secure: None,
        same_site: SameSite::Lax,
        domain: None,
        path: "/".into(),
        rolling: false,
    }
}

/// The middleware returned by [`session`].
pub struct Sessions {
    store: Box<dyn SessionStore>,
    cookie: String,
    max_age: Duration,
    secure: Option<bool>,
    same_site: SameSite,
    domain: Option<String>,
    path: String,
    rolling: bool,
}

impl fmt::Debug for Sessions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sessions")
            .field("cookie", &self.cookie)
            .field("max_age", &self.max_age)
            .finish_non_exhaustive()
    }
}

impl Sessions {
    /// The name of the cookie (`vitesse.sid` by default).
    pub fn cookie_name(mut self, name: &str) -> Self {
        self.cookie = name.to_owned();
        self
    }

    /// How long a session lives after its last change (1 day by default),
    /// in the store and in the browser.
    pub fn max_age(mut self, max_age: Duration) -> Self {
        self.max_age = max_age;
        self
    }

    /// Extends the session on every request, not only when it changes
    /// (`false` by default: one store write per request).
    pub fn rolling(mut self, rolling: bool) -> Self {
        self.rolling = rolling;
        self
    }

    /// Forces the `Secure` attribute (by default, set when the request
    /// arrived over HTTPS).
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = Some(secure);
        self
    }

    /// The `SameSite` attribute (`Lax` by default).
    pub fn same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = same_site;
        self
    }

    /// The `Domain` attribute, to share the session with subdomains.
    pub fn domain(mut self, domain: &str) -> Self {
        self.domain = Some(domain.to_owned());
        self
    }

    /// The `Path` attribute (`/` by default).
    pub fn path(mut self, path: &str) -> Self {
        self.path = path.to_owned();
        self
    }

    fn cookie(&self, value: String, secure: bool, max_age: Duration) -> Cookie {
        let mut cookie = Cookie::new(self.cookie.clone(), value)
            .path(self.path.clone())
            .http_only(true)
            .secure(secure)
            .same_site(self.same_site)
            .max_age(max_age);
        if let Some(domain) = &self.domain {
            cookie = cookie.domain(domain.clone());
        }
        cookie
    }
}

impl Middleware for Sessions {
    fn handle(&'static self, mut req: Request, next: Next) -> BoxFuture<Response> {
        Box::pin(async move {
            let key = req.cookie_key();
            let id = req
                .cookie(&self.cookie)
                .and_then(|raw| match key {
                    Some(key) => key.verify(&self.cookie, raw),
                    None => Some(raw),
                })
                .filter(|id| id.len() == 22 && id.bytes().all(|b| ALPHABET.contains(&b)))
                .map(str::to_owned);
            let (id, record) = match id {
                Some(id) => match self.store.load(&id).await {
                    Ok(Some(record)) => (Some(id), record),
                    Ok(None) => (None, Record::new()),
                    Err(e) => {
                        return Error::internal("Internal Server Error")
                            .with_source(e)
                            .into_response();
                    }
                },
                None => (None, Record::new()),
            };
            let had_cookie = req.cookie(&self.cookie).is_some();
            let secure = self.secure.unwrap_or_else(|| req.secure());
            let session = Session::new(id, record);
            req.set(session.clone());
            let mut res = next.run(req).await;

            // Une copie : la session reste lisible après la réponse (dans un
            // handler WebSocket, qui tourne après le `101`).
            let (old_id, record, changed, destroyed, regenerate) = {
                let mut state = session.lock();
                let snapshot = (
                    state.id.clone(),
                    state.record.clone(),
                    state.changed,
                    state.destroyed,
                    state.regenerate,
                );
                state.changed = false;
                state.regenerate = false;
                snapshot
            };
            if destroyed {
                if let Some(id) = &old_id {
                    if let Err(e) = self.store.delete(id).await {
                        eprintln!("[vitesse] session store error: {e}");
                    }
                }
                if had_cookie {
                    res = res.cookie(self.cookie(String::new(), secure, Duration::ZERO));
                }
                return res;
            }
            let id = match old_id {
                Some(id) if regenerate => {
                    if let Err(e) = self.store.delete(&id).await {
                        eprintln!("[vitesse] session store error: {e}");
                    }
                    None
                }
                id => id,
            };
            let save = changed || (self.rolling && id.is_some());
            if !save || (id.is_none() && record.is_empty()) {
                return res;
            }
            let id = id.unwrap_or_else(new_id);
            session.lock().id = Some(id.clone());
            if let Err(e) = self.store.save(&id, &record, self.max_age).await {
                return Error::internal("Internal Server Error")
                    .with_source(e)
                    .into_response();
            }
            let value = match key {
                Some(key) => key.sign(&self.cookie, &id),
                None => id,
            };
            res.cookie(self.cookie(value, secure, self.max_age))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_round_trip() {
        for len in 0..40 {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            let text = base64url(&bytes);
            assert!(!text.contains(['+', '/', '=']));
            assert_eq!(base64url_decode(&text).unwrap(), bytes);
        }
        assert_eq!(base64url(b"hello"), "aGVsbG8");
        assert!(base64url_decode("a").is_none());
        assert!(base64url_decode("a+bc").is_none());
    }

    #[test]
    fn signatures() {
        let key = Key::new(&[7; 32]);
        let signed = key.sign("theme", "dark");
        assert!(signed.starts_with("dark."));
        assert_eq!(key.verify("theme", &signed), Some("dark"));
        // Une autre valeur, un autre nom ou une autre clé : refusé.
        let forged = signed.replacen("dark", "light", 1);
        assert_eq!(key.verify("theme", &forged), None);
        assert_eq!(key.verify("lang", &signed), None);
        assert_eq!(Key::new(&[8; 32]).verify("theme", &signed), None);
        assert_eq!(key.verify("theme", "dark"), None);
    }

    #[test]
    #[should_panic(expected = "at least 32 bytes")]
    fn short_secrets_are_refused() {
        Key::new(b"too short");
    }

    #[test]
    fn ids_are_random() {
        let (a, b) = (new_id(), new_id());
        assert_ne!(a, b);
        assert_eq!(a.len(), 22);
    }
}
