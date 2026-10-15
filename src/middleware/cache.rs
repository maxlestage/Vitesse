//! HTTP caching: `ETag`s and `304 Not Modified` for any response (Express's
//! `etag` setting), and a server-side cache of whole responses with a time
//! to live (Laravel's `responsecache`, Rails' action caching, Varnish in a
//! middleware).

use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use bytes::Bytes;
use http::header::{self, HeaderMap, HeaderName, HeaderValue};
use http::{Method, StatusCode};

use crate::handler::{BoxFuture, Middleware, Next};
use crate::request::Request;
use crate::response::Response;

/// A weak `ETag` for a body: its length and a 64-bit hash of its bytes.
fn etag_of(body: &[u8]) -> HeaderValue {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    body.hash(&mut hasher);
    let tag = format!("W/\"{:x}-{:016x}\"", body.len(), hasher.finish());
    HeaderValue::try_from(tag).expect("an ETag made of hexadecimal digits is a valid header")
}

/// Whether `If-None-Match` matches the `ETag` (weak comparison, RFC 9110).
fn none_match(if_none_match: &str, etag: &HeaderValue) -> bool {
    let Ok(etag) = etag.to_str() else {
        return false;
    };
    let opaque = |tag: &str| tag.trim().trim_start_matches("W/").to_owned();
    let etag = opaque(etag);
    if_none_match
        .split(',')
        .any(|tag| tag.trim() == "*" || opaque(tag) == etag)
}

/// `304 Not Modified`, with the headers a `200` would have had for the
/// cache (RFC 9110, section 15.4.5).
fn not_modified(headers: &HeaderMap) -> Response {
    let mut res = Response::new().status(StatusCode::NOT_MODIFIED);
    for name in [
        header::ETAG,
        header::CACHE_CONTROL,
        header::CONTENT_LOCATION,
        header::DATE,
        header::EXPIRES,
        header::VARY,
        header::AGE,
        HeaderName::from_static("x-cache"),
    ] {
        for value in headers.get_all(&name) {
            res.headers_mut().append(name.clone(), value.clone());
        }
    }
    res
}

fn is_read(method: &Method) -> bool {
    *method == Method::GET || *method == Method::HEAD
}

/// Adds a weak `ETag` to the `200` responses of `GET` and `HEAD` requests
/// whose body is in memory (text, JSON, HTML…, not streams nor files, which
/// [`res::file`](crate::res::file) already tags), and answers `304 Not
/// Modified` without a body when the browser already has that version
/// (`If-None-Match`): Express's default `etag`, here opt-in. The handler
/// still runs; the bandwidth is saved. To save the work too, see [`cache`].
///
/// ```
/// use vitesse::prelude::*;
///
/// let mut app = App::new();
/// app.middleware(middleware::etag());
/// app.get("/catalog", |_| async { Json(json!({ "items": ["a", "b"] })) });
/// ```
pub fn etag() -> ETag {
    ETag(())
}

/// The middleware returned by [`etag`].
#[derive(Debug, Clone, Copy)]
pub struct ETag(());

impl Middleware for ETag {
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response> {
        if !is_read(req.method()) {
            return next.run(req);
        }
        let if_none_match = req.header(header::IF_NONE_MATCH).map(str::to_owned);
        let future = next.run(req);
        Box::pin(async move {
            let mut res = future.await;
            if res.status_code() != StatusCode::OK {
                return res;
            }
            let etag = match res.headers().get(header::ETAG) {
                Some(etag) => etag.clone(),
                None => {
                    let Some(body) = res.body().in_memory() else {
                        return res;
                    };
                    let etag = etag_of(&body);
                    res.headers_mut().insert(header::ETAG, etag.clone());
                    etag
                }
            };
            match if_none_match {
                Some(tags) if none_match(&tags, &etag) => not_modified(res.headers()),
                _ => res,
            }
        })
    }
}

/// Caches whole responses in memory for `ttl` (Laravel's `responsecache`,
/// Rails' action caching): the first `GET` of a URL runs the handler, the
/// following ones, until the entry expires, are answered from memory
/// without running it, with an `ETag` (and `304 Not Modified` when the
/// browser has the same version), `Age`, `X-Cache: HIT` or `MISS`, and a
/// `Cache-Control: public, max-age=…` unless the handler set one.
///
/// Only the `200` responses with a body in memory are kept, and never those
/// that set a cookie or say `Cache-Control: no-store`, `private` or
/// `no-cache`; requests with an `Authorization` header are not cached. The
/// key is the path and the query string, or yours with [`ResponseCache::key`]
/// (add what the response depends on: the language, the user…).
///
/// Clone it before giving it to the application to invalidate entries
/// when the data changes:
///
/// ```
/// use std::time::Duration;
/// use vitesse::prelude::*;
///
/// let cache = middleware::cache(Duration::from_secs(60)).max_entries(1_000);
///
/// let mut app = App::new();
/// app.get("/posts", (|_| async { Json(vec!["Hello"]) }).with(cache.clone()));
/// app.post("/posts", move |_req: Request| {
///     let cache = cache.clone();
///     async move {
///         // … save the post, then:
///         cache.invalidate("/posts");
///         StatusCode::CREATED
///     }
/// });
/// ```
pub fn cache(ttl: Duration) -> ResponseCache {
    ResponseCache {
        ttl,
        max_entries: 10_000,
        max_body: 1024 * 1024,
        cache_control: true,
        key: None,
        store: Arc::new(Store::default()),
    }
}

type KeyFn = Arc<dyn Fn(&Request) -> Option<String> + Send + Sync>;

/// The middleware returned by [`cache`]. Clones share their entries.
#[derive(Clone)]
pub struct ResponseCache {
    ttl: Duration,
    max_entries: usize,
    max_body: usize,
    cache_control: bool,
    key: Option<KeyFn>,
    store: Arc<Store>,
}

impl fmt::Debug for ResponseCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponseCache")
            .field("ttl", &self.ttl)
            .field("max_entries", &self.max_entries)
            .field("max_body", &self.max_body)
            .field("entries", &self.len())
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
struct Store {
    entries: Mutex<HashMap<String, Arc<Entry>>>,
}

struct Entry {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
    etag: HeaderValue,
    stored: Instant,
    expires: Instant,
}

impl ResponseCache {
    /// The maximum number of responses kept (10,000 by default): beyond
    /// it, the expired ones are dropped, then the oldest.
    pub fn max_entries(mut self, max: usize) -> Self {
        self.max_entries = max.max(1);
        self
    }

    /// The largest body kept, in bytes (1 MiB by default).
    pub fn max_body(mut self, bytes: usize) -> Self {
        self.max_body = bytes;
        self
    }

    /// Whether to add `Cache-Control: public, max-age=<ttl>` to the cached
    /// responses that have no `Cache-Control` (yes by default). With
    /// `false`, browsers and proxies are told nothing: only the server
    /// caches.
    pub fn cache_control(mut self, add: bool) -> Self {
        self.cache_control = add;
        self
    }

    /// Computes the key of a request; `None` does not cache it. By default,
    /// the path and the query string.
    ///
    /// ```
    /// use std::time::Duration;
    /// use vitesse::prelude::*;
    ///
    /// // One entry per path and language; logged-in users are not cached.
    /// let cache = middleware::cache(Duration::from_secs(30)).key(|req: &Request| {
    ///     if req.cookie("session").is_some() {
    ///         return None;
    ///     }
    ///     let lang = req.accepts_language(&["en", "fr"]).unwrap_or("en");
    ///     Some(format!("{lang}:{}", req.path()))
    /// });
    /// ```
    pub fn key<F>(mut self, key: F) -> Self
    where
        F: Fn(&Request) -> Option<String> + Send + Sync + 'static,
    {
        self.key = Some(Arc::new(key));
        self
    }

    /// Removes the entry of this key (with the default key: a path, with its
    /// query string if it had one).
    pub fn invalidate(&self, key: &str) {
        self.lock().remove(key);
    }

    /// Removes every entry whose key starts with `prefix` (`"/posts"`
    /// removes `/posts`, `/posts?page=2` and `/posts/42`).
    pub fn invalidate_prefix(&self, prefix: &str) {
        self.lock().retain(|key, _| !key.starts_with(prefix));
    }

    /// Removes every entry.
    pub fn clear(&self) {
        self.lock().clear();
    }

    /// The number of entries (expired ones included until they are dropped).
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// The cache has no entry.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<Entry>>> {
        self.store
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn key_of(&self, req: &Request) -> Option<String> {
        if let Some(key) = &self.key {
            return key(req);
        }
        Some(match req.query_string() {
            Some(query) => format!("{}?{query}", req.path()),
            None => req.path().to_owned(),
        })
    }

    /// Keeps the response if it can be cached; returns it with its
    /// cache headers.
    fn store(&self, key: String, mut res: Response) -> Response {
        let cacheable = res.status_code() == StatusCode::OK
            && res.error().is_none()
            && !res.headers().contains_key(header::SET_COOKIE)
            && !res
                .headers()
                .get_all(header::CACHE_CONTROL)
                .iter()
                .filter_map(|v| v.to_str().ok())
                .any(|v| {
                    let v = v.to_ascii_lowercase();
                    v.contains("no-store") || v.contains("private") || v.contains("no-cache")
                });
        let body = match res.body().in_memory() {
            Some(body) if cacheable && body.len() <= self.max_body => body,
            _ => return res,
        };
        let etag = match res.headers().get(header::ETAG) {
            Some(etag) => etag.clone(),
            None => {
                let etag = etag_of(&body);
                res.headers_mut().insert(header::ETAG, etag.clone());
                etag
            }
        };
        if self.cache_control && !res.headers().contains_key(header::CACHE_CONTROL) {
            let value = format!("public, max-age={}", self.ttl.as_secs());
            if let Ok(value) = HeaderValue::try_from(value) {
                res.headers_mut().insert(header::CACHE_CONTROL, value);
            }
        }
        let now = Instant::now();
        let entry = Arc::new(Entry {
            status: res.status_code(),
            headers: res.headers().clone(),
            body,
            etag,
            stored: now,
            expires: now + self.ttl,
        });
        {
            let mut entries = self.lock();
            if entries.len() >= self.max_entries && !entries.contains_key(&key) {
                entries.retain(|_, entry| entry.expires > now);
                if entries.len() >= self.max_entries {
                    let oldest = entries
                        .iter()
                        .min_by_key(|(_, entry)| entry.stored)
                        .map(|(key, _)| key.clone());
                    if let Some(oldest) = oldest {
                        entries.remove(&oldest);
                    }
                }
            }
            entries.insert(key, entry);
        }
        res.headers_mut()
            .insert("x-cache", HeaderValue::from_static("MISS"));
        res
    }
}

impl Entry {
    fn response(&self, if_none_match: Option<&str>) -> Response {
        let mut headers = self.headers.clone();
        headers.insert(
            header::AGE,
            HeaderValue::from(self.stored.elapsed().as_secs()),
        );
        headers.insert("x-cache", HeaderValue::from_static("HIT"));
        if if_none_match.is_some_and(|tags| none_match(tags, &self.etag)) {
            return not_modified(&headers);
        }
        let mut res = Response::new().status(self.status).send(self.body.clone());
        *res.headers_mut() = headers;
        res
    }
}

impl Middleware for ResponseCache {
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response> {
        if !is_read(req.method()) || req.headers().contains_key(header::AUTHORIZATION) {
            return next.run(req);
        }
        let Some(key) = self.key_of(&req) else {
            return next.run(req);
        };
        let if_none_match = req.header(header::IF_NONE_MATCH).map(str::to_owned);
        let hit = self
            .lock()
            .get(&key)
            .filter(|entry| entry.expires > Instant::now())
            .cloned();
        if let Some(entry) = hit {
            let res = entry.response(if_none_match.as_deref());
            return Box::pin(std::future::ready(res));
        }
        let future = next.run(req);
        Box::pin(async move {
            let res = self.store(key, future.await);
            match (&if_none_match, res.headers().get(header::ETAG)) {
                (Some(tags), Some(etag))
                    if res.status_code() == StatusCode::OK && none_match(tags, etag) =>
                {
                    not_modified(res.headers())
                }
                _ => res,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etags() {
        let a = etag_of(b"hello");
        assert_eq!(a, etag_of(b"hello"));
        assert_ne!(a, etag_of(b"hellp"));
        assert!(a.to_str().unwrap().starts_with("W/\"5-"));
        let strong = a.to_str().unwrap().trim_start_matches("W/").to_owned();
        assert!(none_match(&strong, &a));
        assert!(none_match(&format!("\"x\", {}", a.to_str().unwrap()), &a));
        assert!(none_match("*", &a));
        assert!(!none_match("\"x\"", &a));
    }
}
