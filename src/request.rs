//! The HTTP request received by handlers and middlewares.

use std::any::{Any, TypeId};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::mem::ManuallyDrop;
use std::net::{IpAddr, SocketAddr};
use std::pin::pin;
use std::str::FromStr;
use std::sync::{Mutex, OnceLock, PoisonError};

use bytes::{Bytes, BytesMut};
use http::header::{self, HeaderMap, HeaderName, HeaderValue};
use http::{Extensions, Method, Uri, Version};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;

use crate::body::{Body, BoxError};
use crate::error::Error;
use crate::tree::{Captures, Tree};
use crate::util::percent_decode;

/// Data shared by the whole application (state, configuration).
pub(crate) struct Shared {
    pub(crate) state: StateMap,
    pub(crate) body_limit: usize,
}

/// Typed storage for the global state (`app.state(...)`).
#[derive(Default)]
pub(crate) struct StateMap(HashMap<TypeId, Box<dyn Any + Send + Sync>>);

impl StateMap {
    pub(crate) fn insert<T: Send + Sync + 'static>(&mut self, value: T) {
        self.0.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub(crate) fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.0
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref())
    }
}

pub(crate) enum ReqBody {
    Empty,
    Buffered(Bytes),
    Stream(Body),
    Taken,
}

/// The value of a route parameter: a slice of the path, or a decoded string
/// if the segment contained `%XX` escapes.
enum ParamValue {
    Slice(u32, u32),
    Owned(Box<str>),
}

#[derive(Default)]
pub(crate) struct Params {
    names: &'static [Box<str>],
    values: Vec<ParamValue>,
}

/// Position of a header (name and value) in the raw bytes of the request.
#[derive(Clone, Copy, Default)]
struct Slot {
    name: u16,
    name_len: u16,
    value: u16,
    value_len: u16,
}

const INLINE_SLOTS: usize = 12;

/// The headers as received: plain positions, with no copy or allocation
/// (beyond 12 headers, the remaining ones go into a `Vec`).
#[derive(Default)]
pub(crate) struct RawHeaders {
    len: usize,
    inline: [Slot; INLINE_SLOTS],
    extra: Vec<Slot>,
}

impl RawHeaders {
    /// Adds a header; positions must fit in 16 bits.
    #[inline]
    pub(crate) fn push(&mut self, name: usize, name_len: usize, value: usize, value_len: usize) {
        let slot = Slot {
            name: name as u16,
            name_len: name_len as u16,
            value: value as u16,
            value_len: value_len as u16,
        };
        if self.len < INLINE_SLOTS {
            self.inline[self.len] = slot;
        } else {
            self.extra.push(slot);
        }
        self.len += 1;
    }

    #[inline]
    pub(crate) fn clear(&mut self) {
        self.len = 0;
        self.extra.clear();
    }

    #[inline]
    fn copy_from(&mut self, other: &RawHeaders) {
        self.len = other.len;
        self.inline = other.inline;
        self.extra.clone_from(&other.extra);
    }

    #[inline]
    fn iter(&self) -> impl Iterator<Item = Slot> + '_ {
        self.inline[..self.len.min(INLINE_SLOTS)]
            .iter()
            .chain(&self.extra)
            .copied()
    }
}

/// The path of a `/path?query` target.
#[inline]
fn path_of(head: &[u8], target: (u32, u32), path_end: u32) -> &str {
    std::str::from_utf8(&head[target.0 as usize..path_end as usize]).unwrap_or("/")
}

/// Per-thread pool of requests: the box of a finished request (and its
/// buffers) is reused by the next one, without going through the allocator.
// Les boîtes sont voulues : c'est la boîte elle-même qu'on rend à une requête,
// sans recopier son contenu.
#[allow(clippy::vec_box)]
mod pool {
    use std::cell::RefCell;

    use super::Inner;

    const MAX_POOLED: usize = 256;

    thread_local! {
        static POOL: RefCell<Vec<Box<Inner>>> = const { RefCell::new(Vec::new()) };
    }

    #[inline]
    pub(super) fn take() -> Option<Box<Inner>> {
        POOL.try_with(|pool| pool.borrow_mut().pop()).ok().flatten()
    }

    #[inline]
    pub(super) fn give(inner: Box<Inner>) {
        let _ = POOL.try_with(|pool| {
            let mut pool = pool.borrow_mut();
            if pool.len() < MAX_POOLED {
                pool.push(inner);
            }
        });
    }
}

/// An HTTP request (`req` in Express).
///
/// All methods take `&self`, including the ones that read the body, so you
/// can keep a borrowed parameter while reading the JSON.
///
/// ```
/// use vitesse::prelude::*;
/// # #[derive(serde::Deserialize)] struct Patch { name: String }
///
/// async fn update(req: Request) -> vitesse::Result<String> {
///     let id: u64 = req.param_as("id")?;
///     let page = req.query("page");
///     let patch: Patch = req.json().await?;
///     Ok(format!("user {id} -> {} (page {page:?})", patch.name))
/// }
/// ```
pub struct Request {
    /// Returned to the thread's pool by `Drop`.
    inner: ManuallyDrop<Box<Inner>>,
}

/// The contents of the request, behind a pointer: the request travels
/// through middlewares and handlers by moving only 8 bytes.
struct Inner {
    method: Method,
    version: Version,
    /// The raw bytes of the head, referenced by `target` and `raw_headers`.
    head: Vec<u8>,
    /// Position of the target (`/path?query`) in `head`, and of the `?`.
    target: (u32, u32),
    path_end: u32,
    raw_headers: RawHeaders,
    /// Only built when asked for.
    headers: OnceLock<Box<HeaderMap>>,
    uri: OnceLock<Box<Uri>>,
    extensions: Extensions,
    body: Mutex<ReqBody>,
    /// Positions found by the router (reused buffer).
    captures: Captures,
    params: Params,
    remote: Option<SocketAddr>,
    /// The request carries `Range`, `If-Range`, `If-None-Match` or
    /// `If-Modified-Since` (spotted while parsing, at no extra cost).
    conditional: bool,
    shared: &'static Shared,
}

/// The headers that make [`res::file`](crate::res::file) answer `304` or
/// `206` instead of `200`.
pub(crate) const CONDITIONAL_HEADERS: [&str; 4] =
    ["range", "if-range", "if-none-match", "if-modified-since"];

impl Inner {
    fn new(shared: &'static Shared) -> Self {
        Inner {
            method: Method::GET,
            version: Version::HTTP_11,
            head: Vec::new(),
            target: (0, 0),
            path_end: 0,
            raw_headers: RawHeaders::default(),
            headers: OnceLock::new(),
            uri: OnceLock::new(),
            extensions: Extensions::new(),
            body: Mutex::new(ReqBody::Empty),
            captures: Vec::new(),
            params: Params::default(),
            remote: None,
            conditional: false,
            shared,
        }
    }

    /// A box from the pool, or a new one.
    #[inline]
    fn take(shared: &'static Shared) -> Box<Inner> {
        match pool::take() {
            Some(mut inner) => {
                inner.shared = shared;
                inner
            }
            None => Box::new(Inner::new(shared)),
        }
    }

    /// Clears the request so it can be reused (keeping its buffers).
    fn reset(&mut self) {
        if self.head.capacity() > 16 * 1024 {
            self.head = Vec::new();
        } else {
            self.head.clear();
        }
        self.raw_headers.clear();
        self.headers = OnceLock::new();
        self.uri = OnceLock::new();
        self.extensions.clear();
        *self.body.get_mut().unwrap_or_else(PoisonError::into_inner) = ReqBody::Empty;
        self.captures.clear();
        self.params.names = &[];
        self.params.values.clear();
        self.remote = None;
        self.conditional = false;
    }

    /// Appends the target to the end of `head` and points to it.
    fn push_target(&mut self, target: &[u8]) {
        let start = self.head.len();
        self.head
            .extend_from_slice(if target.is_empty() { b"/" } else { target });
        self.set_target(start, self.head.len());
    }

    fn set_target(&mut self, start: usize, end: usize) {
        let path_end = self.head[start..end]
            .iter()
            .position(|&b| b == b'?')
            .map_or(end, |i| start + i);
        self.target = (start as u32, end as u32);
        self.path_end = path_end as u32;
    }

    #[inline]
    fn path(&self) -> &str {
        path_of(&self.head, self.target, self.path_end)
    }

    #[inline]
    fn query(&self) -> Option<&str> {
        if self.path_end >= self.target.1 {
            return None;
        }
        std::str::from_utf8(&self.head[self.path_end as usize + 1..self.target.1 as usize]).ok()
    }

    #[inline]
    fn target_bytes(&self) -> &[u8] {
        &self.head[self.target.0 as usize..self.target.1 as usize]
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        // SAFETY : `inner` n'est plus jamais lu après ce point.
        let mut inner = unsafe { ManuallyDrop::take(&mut self.inner) };
        inner.reset();
        pool::give(inner);
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("method", &self.inner.method)
            .field("uri", self.uri())
            .field("version", &self.inner.version)
            .field("headers", self.headers())
            .finish()
    }
}

impl Request {
    /// Builds a request read from the network; `head` is the raw head,
    /// copied into a recycled box.
    #[inline]
    pub(crate) fn from_wire(
        head: &[u8],
        method: Method,
        version: Version,
        target: (usize, usize),
        raw_headers: &RawHeaders,
        remote: Option<SocketAddr>,
        shared: &'static Shared,
    ) -> Self {
        let mut inner = Inner::take(shared);
        inner.method = method;
        inner.version = version;
        inner.head.extend_from_slice(head);
        inner.raw_headers.copy_from(raw_headers);
        inner.remote = remote;
        let (start, end) = target;
        if head.get(start) == Some(&b'/') {
            inner.set_target(start, end);
        } else {
            // Forme absolue (`GET http://hôte/chemin`) ou `*` : rare, on passe par `Uri`.
            match Uri::try_from(&head[start..end]) {
                Ok(uri) => {
                    let pq = uri
                        .path_and_query()
                        .map_or("/", |pq| pq.as_str())
                        .to_owned();
                    inner.push_target(pq.as_bytes());
                    inner.uri = OnceLock::from(Box::new(uri));
                }
                Err(_) => inner.set_target(start, end),
            }
        }
        Request {
            inner: ManuallyDrop::new(inner),
        }
    }

    /// Builds a request from an [`http::Request`] (test client).
    pub(crate) fn from_parts(
        parts: http::request::Parts,
        body: ReqBody,
        remote: Option<SocketAddr>,
        shared: &'static Shared,
    ) -> Self {
        let mut inner = Inner::take(shared);
        inner.method = parts.method;
        inner.version = parts.version;
        let pq = parts.uri.path_and_query().map_or("/", |pq| pq.as_str());
        inner.push_target(pq.as_bytes());
        inner.conditional = CONDITIONAL_HEADERS
            .iter()
            .any(|name| parts.headers.contains_key(*name));
        inner.headers = OnceLock::from(Box::new(parts.headers));
        inner.uri = OnceLock::from(Box::new(parts.uri));
        inner.extensions = parts.extensions;
        *inner.body.get_mut().unwrap_or_else(PoisonError::into_inner) = body;
        inner.remote = remote;
        Request {
            inner: ManuallyDrop::new(inner),
        }
    }

    /// Notes that the request carries a conditional or partial header.
    #[inline]
    pub(crate) fn mark_conditional(&mut self) {
        self.inner.conditional = true;
    }

    /// See [`CONDITIONAL_HEADERS`].
    #[inline]
    pub(crate) fn is_conditional(&self) -> bool {
        self.inner.conditional
    }

    /// Gives the request its body.
    #[inline]
    pub(crate) fn put_body(&mut self, body: ReqBody) {
        *self
            .inner
            .body
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner) = body;
    }

    /// Looks up the route of the request in the tree; the positions of the
    /// parameters are kept for [`Request::set_params`].
    #[inline]
    pub(crate) fn find_route<T>(&mut self, tree: &'static Tree<T>) -> Option<&'static T> {
        let inner = &mut **self.inner;
        let path = path_of(&inner.head, inner.target, inner.path_end);
        tree.find(path, &mut inner.captures)
    }

    /// Records the parameters of the route found by the router.
    #[inline]
    pub(crate) fn set_params(&mut self, names: &'static [Box<str>]) {
        let inner = &mut **self.inner;
        let path = path_of(&inner.head, inner.target, inner.path_end);
        inner.params.names = names;
        inner.params.values.clear();
        for &(a, b) in &inner.captures {
            inner.params.values.push(match percent_decode(&path[a..b]) {
                Cow::Borrowed(_) => ParamValue::Slice(a as u32, b as u32),
                Cow::Owned(s) => ParamValue::Owned(s.into_boxed_str()),
            });
        }
    }

    // ----- Ligne de requête -------------------------------------------------

    /// The HTTP method (`req.method`).
    #[inline]
    pub fn method(&self) -> &Method {
        &self.inner.method
    }

    /// The full URI (path + query string).
    pub fn uri(&self) -> &Uri {
        self.inner.uri.get_or_init(|| {
            Box::new(
                Uri::try_from(self.inner.target_bytes()).unwrap_or_else(|_| Uri::from_static("/")),
            )
        })
    }

    /// The path, without the query string (`req.path`).
    #[inline]
    pub fn path(&self) -> &str {
        self.inner.path()
    }

    /// Replaces the URI (e.g. URL rewriting in a global middleware).
    pub fn set_uri(&mut self, uri: Uri) {
        // Les paramètres pointent dans l'ancien chemin : on les rend autonomes.
        let inner = &mut **self.inner;
        let path = path_of(&inner.head, inner.target, inner.path_end);
        for value in &mut inner.params.values {
            if let ParamValue::Slice(a, b) = *value {
                *value = ParamValue::Owned(Box::from(&path[a as usize..b as usize]));
            }
        }
        // La nouvelle cible s'ajoute après les en-têtes bruts, toujours valides.
        let pq = uri
            .path_and_query()
            .map_or("/", |pq| pq.as_str())
            .to_owned();
        inner.push_target(pq.as_bytes());
        inner.uri = OnceLock::from(Box::new(uri));
    }

    /// The HTTP version.
    #[inline]
    pub fn version(&self) -> Version {
        self.inner.version
    }

    // ----- En-têtes ---------------------------------------------------------

    fn raw_name(&self, slot: Slot) -> &[u8] {
        &self.inner.head[slot.name as usize..][..slot.name_len as usize]
    }

    fn raw_value(&self, slot: Slot) -> &[u8] {
        &self.inner.head[slot.value as usize..][..slot.value_len as usize]
    }

    /// All headers (built on first access).
    pub fn headers(&self) -> &HeaderMap {
        self.inner.headers.get_or_init(|| {
            let mut map = HeaderMap::with_capacity(self.inner.raw_headers.len);
            for slot in self.inner.raw_headers.iter() {
                let name = HeaderName::from_bytes(self.raw_name(slot));
                let value = HeaderValue::from_bytes(self.raw_value(slot));
                if let (Ok(name), Ok(value)) = (name, value) {
                    map.append(name, value);
                }
            }
            Box::new(map)
        })
    }

    /// All headers, mutably.
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        self.headers();
        self.inner
            .headers
            .get_mut()
            .expect("headers are initialized")
    }

    /// A header as text (`req.get('host')`); the name is matched
    /// case-insensitively.
    #[inline]
    pub fn header(&self, name: impl AsRef<str>) -> Option<&str> {
        let name = name.as_ref();
        if let Some(map) = self.inner.headers.get() {
            // Comme le chemin rapide ci-dessous : tout UTF-8 valide est accepté
            // (`HeaderValue::to_str` refuserait les accents).
            return map
                .get(name)
                .and_then(|v| std::str::from_utf8(v.as_bytes()).ok());
        }
        self.inner
            .raw_headers
            .iter()
            .find(|&slot| self.raw_name(slot).eq_ignore_ascii_case(name.as_bytes()))
            .and_then(|slot| std::str::from_utf8(self.raw_value(slot)).ok())
    }

    /// All the values of a repeated header.
    pub fn header_all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        let from_map = self.inner.headers.get().map(|map| {
            map.get_all(name)
                .into_iter()
                .filter_map(|v| std::str::from_utf8(v.as_bytes()).ok())
        });
        let from_raw = if from_map.is_none() {
            Some(
                self.inner
                    .raw_headers
                    .iter()
                    .filter(move |&slot| self.raw_name(slot).eq_ignore_ascii_case(name.as_bytes()))
                    .filter_map(move |slot| std::str::from_utf8(self.raw_value(slot)).ok()),
            )
        } else {
            None
        };
        from_map
            .into_iter()
            .flatten()
            .chain(from_raw.into_iter().flatten())
    }

    /// The `Content-Type`.
    pub fn content_type(&self) -> Option<&str> {
        self.header(header::CONTENT_TYPE)
    }

    /// Checks the type of the body (`req.is('json')`): accepts a full type
    /// (`application/json`), a subtype (`json`) or a wildcard (`text/*`).
    pub fn is(&self, ty: &str) -> bool {
        let Some(ct) = self.content_type() else {
            return false;
        };
        let mime = ct.split(';').next().unwrap_or("").trim();
        let Some((main, sub)) = mime.split_once('/') else {
            return false;
        };
        match ty.split_once('/') {
            Some((m, "*")) => main.eq_ignore_ascii_case(m),
            Some(_) => mime.eq_ignore_ascii_case(ty),
            None => {
                sub.eq_ignore_ascii_case(ty)
                    || sub
                        .rsplit_once('+')
                        .is_some_and(|(_, s)| s.eq_ignore_ascii_case(ty))
            }
        }
    }

    /// The requested host, without the port (`req.hostname`).
    pub fn hostname(&self) -> Option<&str> {
        let host = match self.header(header::HOST) {
            Some(host) => host,
            None => self.inner.uri.get()?.host()?,
        };
        if host.starts_with('[') {
            return host.split_once(']').map(|(h, _)| &h[1..]);
        }
        Some(host.rsplit_once(':').map_or(host, |(h, _)| h))
    }

    /// The value of a cookie (`req.cookies.name`).
    pub fn cookie(&self, name: &str) -> Option<&str> {
        for s in self.header_all("cookie") {
            for pair in s.split(';') {
                if let Some((k, v)) = pair.split_once('=') {
                    if k.trim() == name {
                        let v = v.trim();
                        return Some(
                            v.strip_prefix('"')
                                .and_then(|v| v.strip_suffix('"'))
                                .unwrap_or(v),
                        );
                    }
                }
            }
        }
        None
    }

    // ----- Paramètres de route et query string ------------------------------

    /// A route parameter (`req.params.id` for the route `/users/:id`).
    ///
    /// For a wildcard `*path`, the name is `path` (or `*` for an anonymous
    /// wildcard).
    #[inline]
    pub fn param(&self, name: &str) -> Option<&str> {
        let i = self.inner.params.names.iter().position(|n| &**n == name)?;
        Some(match self.inner.params.values.get(i)? {
            ParamValue::Slice(a, b) => &self.inner.path()[*a as usize..*b as usize],
            ParamValue::Owned(s) => s,
        })
    }

    /// A route parameter converted to the desired type; `400` on failure.
    ///
    /// ```
    /// # use vitesse::prelude::*;
    /// async fn show(req: Request) -> vitesse::Result<String> {
    ///     let id: u32 = req.param_as("id")?;
    ///     Ok(format!("user {id}"))
    /// }
    /// ```
    pub fn param_as<T: FromStr>(&self, name: &str) -> Result<T, Error> {
        let raw = self
            .param(name)
            .ok_or_else(|| Error::bad_request(format!("missing parameter '{name}'")))?;
        raw.parse()
            .map_err(|_| Error::bad_request(format!("invalid parameter '{name}': '{raw}'")))
    }

    /// All route parameters, as `(name, value)` pairs.
    pub fn params(&self) -> impl Iterator<Item = (&str, &str)> {
        self.inner
            .params
            .names
            .iter()
            .filter_map(|n| Some((&**n, self.param(n)?)))
    }

    /// The raw query string (`a=1&b=2`).
    #[inline]
    pub fn query_string(&self) -> Option<&str> {
        self.inner.query()
    }

    /// A decoded query string parameter (`req.query.page`).
    ///
    /// Only allocates if the value contains encoded characters.
    pub fn query(&self, name: &str) -> Option<Cow<'_, str>> {
        let q = self.inner.query()?;
        form_urlencoded::parse(q.as_bytes())
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }

    /// All the pairs of the query string.
    pub fn query_pairs(&self) -> impl Iterator<Item = (Cow<'_, str>, Cow<'_, str>)> {
        form_urlencoded::parse(self.inner.query().unwrap_or("").as_bytes())
    }

    /// Deserializes the query string into a struct; `400` on failure.
    ///
    /// ```
    /// # use vitesse::prelude::*;
    /// #[derive(serde::Deserialize)]
    /// struct Search { q: String, page: Option<u32> }
    ///
    /// async fn search(req: Request) -> vitesse::Result<String> {
    ///     let s: Search = req.query_as()?;
    ///     Ok(format!("{} p.{}", s.q, s.page.unwrap_or(1)))
    /// }
    /// ```
    pub fn query_as<T: DeserializeOwned>(&self) -> Result<T, Error> {
        serde_urlencoded::from_str(self.inner.query().unwrap_or(""))
            .map_err(|e| Error::bad_request(format!("invalid query string: {e}")))
    }

    // ----- Connexion --------------------------------------------------------

    /// The client's address (IP + port).
    #[inline]
    pub fn remote_addr(&self) -> Option<SocketAddr> {
        self.inner.remote
    }

    /// The client's IP (`req.ip`). Behind a proxy, look at the
    /// `X-Forwarded-For` header instead.
    #[inline]
    pub fn ip(&self) -> Option<IpAddr> {
        self.inner.remote.map(|a| a.ip())
    }

    // ----- État global et données locales -----------------------------------

    /// The global state registered with `app.state(value)` (`app.locals`).
    ///
    /// # Panics
    /// If no state of this type was registered (the panic is turned into a
    /// `500`). See [`Request::try_state`].
    #[inline]
    pub fn state<T: Send + Sync + 'static>(&self) -> &'static T {
        self.try_state().unwrap_or_else(|| {
            panic!(
                "no state of type `{}`: call `app.state(...)` at startup",
                std::any::type_name::<T>()
            )
        })
    }

    /// Like [`Request::state`], without panicking.
    #[inline]
    pub fn try_state<T: Send + Sync + 'static>(&self) -> Option<&'static T> {
        self.inner.shared.state.get::<T>()
    }

    /// Reads a value attached to this request by a middleware (`res.locals`).
    #[inline]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.inner.extensions.get::<T>()
    }

    /// Attaches a value to this request (e.g. the authenticated user).
    #[inline]
    pub fn set<T: Clone + Send + Sync + 'static>(&mut self, value: T) -> Option<T> {
        self.inner.extensions.insert(value)
    }

    /// The extensions of the request.
    pub fn extensions(&self) -> &Extensions {
        &self.inner.extensions
    }

    /// The extensions of the request, mutably.
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.inner.extensions
    }

    // ----- Corps ------------------------------------------------------------

    fn lock_body(&self) -> std::sync::MutexGuard<'_, ReqBody> {
        self.inner
            .body
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Reads the whole body (limited by `app.body_limit`, `413` beyond that).
    ///
    /// The result is cached, so this can be called several times.
    pub async fn bytes(&self) -> Result<Bytes, Error> {
        let limit = self.inner.shared.body_limit;
        let body = {
            let mut guard = self.lock_body();
            match &*guard {
                ReqBody::Empty => return Ok(Bytes::new()),
                ReqBody::Buffered(bytes) if bytes.len() > limit => {
                    return Err(Error::payload_too_large());
                }
                ReqBody::Buffered(bytes) => return Ok(bytes.clone()),
                _ => std::mem::replace(&mut *guard, ReqBody::Taken),
            }
        };
        let bytes = match body {
            ReqBody::Stream(stream) => collect(stream, limit).await?,
            ReqBody::Empty | ReqBody::Buffered(_) => unreachable!("handled above"),
            ReqBody::Taken => {
                return Err(Error::internal(
                    "the request body has already been consumed",
                ));
            }
        };
        *self.lock_body() = ReqBody::Buffered(bytes.clone());
        Ok(bytes)
    }

    /// Reads the body as UTF-8 text; `400` if it is invalid.
    pub async fn text(&self) -> Result<String, Error> {
        let bytes = self.bytes().await?;
        std::str::from_utf8(&bytes)
            .map(str::to_owned)
            .map_err(|_| Error::bad_request("the body is not valid UTF-8"))
    }

    /// Deserializes a JSON body (`express.json()`); `400` if it is invalid.
    pub async fn json<T: DeserializeOwned>(&self) -> Result<T, Error> {
        let bytes = self.bytes().await?;
        serde_json::from_slice(&bytes).map_err(|e| Error::bad_request(format!("invalid JSON: {e}")))
    }

    /// Deserializes an `application/x-www-form-urlencoded` form
    /// (`express.urlencoded()`); `400` if it is invalid.
    pub async fn form<T: DeserializeOwned>(&self) -> Result<T, Error> {
        let bytes = self.bytes().await?;
        serde_urlencoded::from_bytes(&bytes)
            .map_err(|e| Error::bad_request(format!("invalid form data: {e}")))
    }

    /// Takes the raw body as a stream, without reading it (uploads,
    /// proxies…).
    pub fn take_body(&self) -> Body {
        match std::mem::replace(&mut *self.lock_body(), ReqBody::Taken) {
            ReqBody::Empty => Body::empty(),
            ReqBody::Buffered(bytes) => Body::from(bytes),
            ReqBody::Stream(body) => body,
            ReqBody::Taken => Body::empty(),
        }
    }

    /// Replaces the body (e.g. in a middleware that decompresses it).
    pub fn set_body(&mut self, body: impl Into<Body>) {
        *self
            .inner
            .body
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner) = ReqBody::Stream(body.into());
    }
}

/// Reads a body into memory, up to a maximum size.
async fn collect<B>(body: B, limit: usize) -> Result<Bytes, Error>
where
    B: http_body::Body<Data = Bytes>,
    B::Error: Into<BoxError>,
{
    if body.size_hint().lower() > limit as u64 {
        return Err(Error::payload_too_large());
    }
    let mut body = pin!(body);
    let mut first: Option<Bytes> = None;
    let mut rest: Option<BytesMut> = None;
    let mut total = 0usize;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|e| {
            Error::bad_request("failed to read the request body").with_source(e.into())
        })?;
        let Ok(data) = frame.into_data() else {
            continue;
        };
        total += data.len();
        if total > limit {
            return Err(Error::payload_too_large());
        }
        if let Some(buf) = rest.as_mut() {
            buf.extend_from_slice(&data);
        } else if let Some(prev) = first.take() {
            let mut buf = BytesMut::with_capacity(prev.len() + data.len());
            buf.extend_from_slice(&prev);
            buf.extend_from_slice(&data);
            rest = Some(buf);
        } else {
            first = Some(data);
        }
    }
    Ok(match rest {
        Some(buf) => buf.freeze(),
        None => first.unwrap_or_default(),
    })
}
