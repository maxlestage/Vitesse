//! HTTP responses: [`Response`], the [`IntoResponse`] trait and the
//! Express-style helpers of the [`res`] module.

use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt;
use std::sync::LazyLock;
use std::time::Duration;

use bytes::Bytes;
use http::StatusCode;
use http::header::{self, HeaderMap, HeaderName, HeaderValue};
use serde::Serialize;

use crate::body::Body;
use crate::error::Error;

/// The most common content types: they are stored as a simple code and
/// written in one block, without going through a `HeaderMap`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Ctype {
    None = 0,
    Text,
    Html,
    Json,
    Octets,
}

impl Ctype {
    const VALUES: [&'static str; 5] = [
        "",
        "text/plain; charset=utf-8",
        "text/html; charset=utf-8",
        "application/json",
        "application/octet-stream",
    ];

    const LINES: [&'static [u8]; 5] = [
        b"",
        b"content-type: text/plain; charset=utf-8\r\n",
        b"content-type: text/html; charset=utf-8\r\n",
        b"content-type: application/json\r\n",
        b"content-type: application/octet-stream\r\n",
    ];

    #[inline]
    fn value(self) -> &'static str {
        Self::VALUES[self as usize]
    }

    /// The complete header line, ready to be written.
    #[inline]
    pub(crate) fn line(self) -> &'static [u8] {
        Self::LINES[self as usize]
    }
}

/// The headers seen by [`Response::headers`] when only a common content type
/// is set.
static CTYPE_HEADERS: LazyLock<[HeaderMap; 5]> = LazyLock::new(|| {
    std::array::from_fn(|i| {
        let mut map = HeaderMap::new();
        if i > 0 {
            map.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(Ctype::VALUES[i]),
            );
        }
        map
    })
});

/// An HTTP response, built just like with Express:
///
/// ```
/// use vitesse::prelude::*;
///
/// let res = Response::new()
///     .status(201)
///     .header("x-powered-by", "Vitesse")
///     .json(json!({ "id": 1 }));
/// assert_eq!(res.status_code(), 201);
/// ```
pub struct Response {
    pub(crate) status: StatusCode,
    /// Common content type, as long as no `HeaderMap` exists.
    pub(crate) ctype: Ctype,
    /// The headers, created on the first header that is not a `ctype`.
    pub(crate) headers: Option<Box<HeaderMap>>,
    pub(crate) body: Body,
    pub(crate) extensions: http::Extensions,
    pub(crate) error: Option<Box<Error>>,
}

impl Default for Response {
    fn default() -> Self {
        Response::new()
    }
}

impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("status", &self.status)
            .field("headers", self.headers())
            .field("body", &self.body)
            .finish()
    }
}

impl Response {
    /// An empty `200 OK` response.
    #[inline]
    pub fn new() -> Self {
        Response {
            status: StatusCode::OK,
            ctype: Ctype::None,
            headers: None,
            body: Body::empty(),
            extensions: http::Extensions::new(),
            error: None,
        }
    }

    /// Changes the status (`res.status(404)` in Express).
    #[inline]
    pub fn status(mut self, status: impl IntoStatus) -> Self {
        self.status = status.into_status();
        self
    }

    /// Sets a header (replacing any existing value). Invalid names or values
    /// are ignored.
    #[inline]
    pub fn header<K, V>(mut self, name: K, value: V) -> Self
    where
        K: TryInto<HeaderName>,
        V: TryInto<HeaderValue>,
    {
        self.set_header(name, value);
        self
    }

    /// Adds a header without replacing existing values.
    pub fn append_header<K, V>(mut self, name: K, value: V) -> Self
    where
        K: TryInto<HeaderName>,
        V: TryInto<HeaderValue>,
    {
        if let (Ok(name), Ok(value)) = (name.try_into(), value.try_into()) {
            self.headers_mut().append(name, value);
        }
        self
    }

    /// Sets the `Content-Type` (`res.type()` in Express).
    pub fn content_type(self, value: &str) -> Self {
        self.header(header::CONTENT_TYPE, value)
    }

    /// Sets the body, without touching the `Content-Type`.
    #[inline]
    pub fn send(mut self, body: impl Into<Body>) -> Self {
        self.body = body.into();
        self
    }

    /// Text body (`text/plain; charset=utf-8`).
    #[inline]
    pub fn text(self, text: impl Into<Body>) -> Self {
        self.with_ctype(Ctype::Text).send(text)
    }

    /// HTML body (`text/html; charset=utf-8`).
    #[inline]
    pub fn html(self, html: impl Into<Body>) -> Self {
        self.with_ctype(Ctype::Html).send(html)
    }

    /// Serializes `value` as JSON (`res.json()` in Express).
    #[inline]
    pub fn json<T: Serialize>(self, value: T) -> Self {
        let mut buf = Vec::with_capacity(128);
        match serde_json::to_writer(&mut buf, &value) {
            Ok(()) => self.json_bytes(buf),
            Err(e) => Error::internal("JSON serialization failed")
                .with_source(e)
                .into_response(),
        }
    }

    #[inline]
    pub(crate) fn json_bytes(self, json: impl Into<Body>) -> Self {
        self.with_ctype(Ctype::Json).send(json)
    }

    #[inline]
    pub(crate) fn with_ctype(mut self, ctype: Ctype) -> Self {
        match &mut self.headers {
            Some(map) => {
                map.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(ctype.value()),
                );
            }
            None => self.ctype = ctype,
        }
        self
    }

    /// Adds a cookie (`res.cookie()` in Express).
    pub fn cookie(self, cookie: Cookie) -> Self {
        self.append_header(header::SET_COOKIE, cookie.to_string())
    }

    /// Deletes a cookie on the client side (`res.clearCookie()` in Express).
    pub fn clear_cookie(self, name: &str) -> Self {
        self.cookie(Cookie::new(name, "").path("/").max_age(Duration::ZERO))
    }

    /// Asks the browser to download the response under this file name
    /// (`res.attachment()` in Express).
    pub fn attachment(self, filename: &str) -> Self {
        let safe: String = filename
            .chars()
            .map(|c| {
                if c == '"' || c == '\\' || c.is_control() {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        let ascii: String = safe
            .chars()
            .map(|c| if c.is_ascii() { c } else { '_' })
            .collect();
        let mut value = format!("attachment; filename=\"{ascii}\"");
        if ascii != safe {
            value.push_str("; filename*=UTF-8''");
            for b in safe.bytes() {
                if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                    value.push(b as char);
                } else {
                    value.push_str(&format!("%{b:02X}"));
                }
            }
        }
        self.header(header::CONTENT_DISPOSITION, value)
    }

    /// The status of the response.
    #[inline]
    pub fn status_code(&self) -> StatusCode {
        self.status
    }

    /// Changes the status in place.
    pub fn set_status(&mut self, status: impl IntoStatus) -> &mut Self {
        self.status = status.into_status();
        self
    }

    /// The headers.
    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        match &self.headers {
            Some(map) => map,
            None => &CTYPE_HEADERS[self.ctype as usize],
        }
    }

    /// The headers, mutably.
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        if self.headers.is_none() {
            let mut map = header_pool::take();
            if self.ctype != Ctype::None {
                map.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(self.ctype.value()),
                );
                self.ctype = Ctype::None;
            }
            self.headers = Some(map);
        }
        match &mut self.headers {
            Some(map) => map,
            None => unreachable!("headers created above"),
        }
    }

    /// Reads a header.
    pub fn get_header(&self, name: &str) -> Option<&str> {
        self.headers().get(name).and_then(|v| v.to_str().ok())
    }

    /// Sets a header in place. Invalid names or values are ignored.
    #[inline]
    pub fn set_header<K, V>(&mut self, name: K, value: V) -> &mut Self
    where
        K: TryInto<HeaderName>,
        V: TryInto<HeaderValue>,
    {
        if let (Ok(name), Ok(value)) = (name.try_into(), value.try_into()) {
            self.headers_mut().insert(name, value);
        }
        self
    }

    /// The body.
    pub fn body(&self) -> &Body {
        &self.body
    }

    /// The body, mutably.
    pub fn body_mut(&mut self) -> &mut Body {
        &mut self.body
    }

    /// Consumes the response and returns its body.
    pub fn into_body(self) -> Body {
        self.body
    }

    /// The extensions (typed data attached to the response).
    pub fn extensions(&self) -> &http::Extensions {
        &self.extensions
    }

    /// The extensions, mutably.
    pub fn extensions_mut(&mut self) -> &mut http::Extensions {
        &mut self.extensions
    }

    /// The error behind this response, if it was produced from an [`Error`].
    pub fn error(&self) -> Option<&Error> {
        self.error.as_deref()
    }

    /// Removes the error attached to the response.
    pub fn take_error(&mut self) -> Option<Error> {
        self.error.take().map(|e| *e)
    }

    /// Logs the cause of an unhandled 5xx error.
    pub(crate) fn log_server_error(&self) {
        if let Some(err) = &self.error {
            if err.status().is_server_error() {
                if let Some(source) = err.source() {
                    eprintln!("[vitesse] error {}: {source}", err.status().as_u16());
                }
            }
        }
    }

    /// Converts into an [`http::Response`].
    pub fn into_http(self) -> http::Response<Body> {
        self.log_server_error();
        let headers = match self.headers {
            Some(map) => *map,
            None => CTYPE_HEADERS[self.ctype as usize].clone(),
        };
        let mut res = http::Response::new(self.body);
        *res.status_mut() = self.status;
        *res.headers_mut() = headers;
        *res.extensions_mut() = self.extensions;
        res
    }

    /// Builds from an [`http::Response`].
    pub fn from_http(res: http::Response<Body>) -> Self {
        let (parts, body) = res.into_parts();
        Response {
            status: parts.status,
            ctype: Ctype::None,
            headers: (!parts.headers.is_empty()).then(|| Box::new(parts.headers)),
            body,
            extensions: parts.extensions,
            error: None,
        }
    }
}

/// Per-thread pool of already allocated `HeaderMap`s: a response reuses the
/// maps of a previous response instead of allocating new ones.
// Les boîtes sont voulues : une réponse reçoit la boîte elle-même.
#[allow(clippy::vec_box)]
pub(crate) mod header_pool {
    use super::*;

    const MAX_POOLED: usize = 128;

    thread_local! {
        static POOL: RefCell<Vec<Box<HeaderMap>>> = const { RefCell::new(Vec::new()) };
    }

    #[inline]
    pub(crate) fn take() -> Box<HeaderMap> {
        POOL.try_with(|pool| pool.borrow_mut().pop())
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    #[inline]
    pub(crate) fn recycle(mut map: Box<HeaderMap>) {
        if map.capacity() == 0 || map.capacity() > 64 {
            return;
        }
        map.clear();
        let _ = POOL.try_with(|pool| {
            let mut pool = pool.borrow_mut();
            if pool.len() < MAX_POOLED {
                pool.push(map);
            }
        });
    }
}

/// Converts a code (`404`, `StatusCode::NOT_FOUND`…) into a [`StatusCode`].
/// An invalid code becomes `500`.
pub trait IntoStatus {
    /// Performs the conversion.
    fn into_status(self) -> StatusCode;
}

impl IntoStatus for StatusCode {
    #[inline]
    fn into_status(self) -> StatusCode {
        self
    }
}

impl IntoStatus for u16 {
    #[inline]
    fn into_status(self) -> StatusCode {
        StatusCode::from_u16(self).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

impl IntoStatus for i32 {
    #[inline]
    fn into_status(self) -> StatusCode {
        u16::try_from(self).map_or(StatusCode::INTERNAL_SERVER_ERROR, u16::into_status)
    }
}

/// Anything a handler can return.
///
/// | Return type | Response |
/// |---|---|
/// | `&'static str`, `String` | `200`, `text/plain` |
/// | [`Json<T>`], `serde_json::Value` | `200`, `application/json` |
/// | [`Html<T>`] | `200`, `text/html` |
/// | `Bytes`, `Vec<u8>` | `200`, `application/octet-stream` |
/// | `()` | `200`, empty body |
/// | [`StatusCode`] | that status, with its reason phrase as text |
/// | `(status, T)` | `T` with that status (e.g. `(201, Json(user))`) |
/// | `Option<T>` | `T`, or `404` if `None` |
/// | `Result<T, E>` | `T` or the error `E` |
/// | [`Error`] | the status of the error and `{"error": "..."}` |
pub trait IntoResponse {
    /// Performs the conversion.
    fn into_response(self) -> Response;
}

impl IntoResponse for Response {
    #[inline]
    fn into_response(self) -> Response {
        self
    }
}

impl IntoResponse for http::Response<Body> {
    #[inline]
    fn into_response(self) -> Response {
        Response::from_http(self)
    }
}

impl IntoResponse for Body {
    #[inline]
    fn into_response(self) -> Response {
        Response::new().send(self)
    }
}

impl IntoResponse for () {
    #[inline]
    fn into_response(self) -> Response {
        Response::new()
    }
}

impl IntoResponse for &'static str {
    #[inline]
    fn into_response(self) -> Response {
        Response::new().text(self)
    }
}

impl IntoResponse for String {
    #[inline]
    fn into_response(self) -> Response {
        Response::new().text(self)
    }
}

impl IntoResponse for Cow<'static, str> {
    #[inline]
    fn into_response(self) -> Response {
        Response::new().text(self)
    }
}

impl IntoResponse for Bytes {
    #[inline]
    fn into_response(self) -> Response {
        Response::new().with_ctype(Ctype::Octets).send(self)
    }
}

impl IntoResponse for Vec<u8> {
    #[inline]
    fn into_response(self) -> Response {
        Response::new().with_ctype(Ctype::Octets).send(self)
    }
}

impl IntoResponse for &'static [u8] {
    #[inline]
    fn into_response(self) -> Response {
        Response::new().with_ctype(Ctype::Octets).send(self)
    }
}

impl IntoResponse for serde_json::Value {
    #[inline]
    fn into_response(self) -> Response {
        Response::new().json(self)
    }
}

impl IntoResponse for StatusCode {
    #[inline]
    fn into_response(self) -> Response {
        res::send_status(self)
    }
}

impl<S: IntoStatus, T: IntoResponse> IntoResponse for (S, T) {
    #[inline]
    fn into_response(self) -> Response {
        self.1.into_response().status(self.0)
    }
}

impl<T: IntoResponse, E: IntoResponse> IntoResponse for Result<T, E> {
    #[inline]
    fn into_response(self) -> Response {
        match self {
            Ok(v) => v.into_response(),
            Err(e) => e.into_response(),
        }
    }
}

impl<T: IntoResponse> IntoResponse for Option<T> {
    #[inline]
    fn into_response(self) -> Response {
        match self {
            Some(v) => v.into_response(),
            None => Error::from_status(StatusCode::NOT_FOUND).into_response(),
        }
    }
}

/// JSON response: `Json(value)`.
///
/// ```
/// use vitesse::prelude::*;
/// # #[derive(serde::Serialize)] struct User { id: u32 }
/// async fn show(_req: Request) -> Json<User> {
///     Json(User { id: 1 })
/// }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct Json<T>(pub T);

impl<T: Serialize> IntoResponse for Json<T> {
    #[inline]
    fn into_response(self) -> Response {
        Response::new().json(self.0)
    }
}

/// HTML response: `Html("<h1>Hello</h1>")`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Html<T>(pub T);

impl<T: Into<Body>> IntoResponse for Html<T> {
    #[inline]
    fn into_response(self) -> Response {
        Response::new().html(self.0)
    }
}

/// HTTP redirect.
#[derive(Debug, Clone)]
pub struct Redirect {
    status: StatusCode,
    location: String,
}

impl Redirect {
    /// `302 Found` (Express's default).
    pub fn to(location: impl Into<String>) -> Self {
        Redirect {
            status: StatusCode::FOUND,
            location: location.into(),
        }
    }

    /// `301 Moved Permanently`.
    pub fn permanent(location: impl Into<String>) -> Self {
        Redirect {
            status: StatusCode::MOVED_PERMANENTLY,
            location: location.into(),
        }
    }

    /// `303 See Other` (typically after a POST).
    pub fn see_other(location: impl Into<String>) -> Self {
        Redirect {
            status: StatusCode::SEE_OTHER,
            location: location.into(),
        }
    }

    /// `307 Temporary Redirect` (keeps the method).
    pub fn temporary(location: impl Into<String>) -> Self {
        Redirect {
            status: StatusCode::TEMPORARY_REDIRECT,
            location: location.into(),
        }
    }
}

impl IntoResponse for Redirect {
    fn into_response(self) -> Response {
        match HeaderValue::try_from(self.location) {
            Ok(location) => Response::new()
                .status(self.status)
                .header(header::LOCATION, location),
            Err(_) => Error::internal("invalid redirect URL").into_response(),
        }
    }
}

/// The `SameSite` attribute of a cookie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    /// `SameSite=Strict`
    Strict,
    /// `SameSite=Lax`
    Lax,
    /// `SameSite=None` (requires `Secure`)
    None,
}

/// A cookie to send with [`Response::cookie`].
///
/// ```
/// use vitesse::{Cookie, SameSite};
/// use std::time::Duration;
///
/// let c = Cookie::new("session", "abc")
///     .http_only(true)
///     .same_site(SameSite::Lax)
///     .max_age(Duration::from_secs(3600));
/// assert_eq!(c.to_string(), "session=abc; Path=/; Max-Age=3600; HttpOnly; SameSite=Lax");
/// ```
#[derive(Debug, Clone)]
pub struct Cookie {
    name: String,
    value: String,
    path: Option<String>,
    domain: Option<String>,
    max_age: Option<Duration>,
    secure: bool,
    http_only: bool,
    same_site: Option<SameSite>,
}

impl Cookie {
    /// Creates a cookie with `Path=/`.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Cookie {
            name: name.into(),
            value: value.into(),
            path: Some("/".into()),
            domain: None,
            max_age: None,
            secure: false,
            http_only: false,
            same_site: None,
        }
    }

    /// The `Path` attribute.
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// The `Domain` attribute.
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// The `Max-Age` attribute.
    pub fn max_age(mut self, max_age: Duration) -> Self {
        self.max_age = Some(max_age);
        self
    }

    /// The `Secure` attribute.
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// The `HttpOnly` attribute.
    pub fn http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }

    /// The `SameSite` attribute.
    pub fn same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = Some(same_site);
        self
    }
}

impl fmt::Display for Cookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.name, self.value)?;
        if let Some(path) = &self.path {
            write!(f, "; Path={path}")?;
        }
        if let Some(domain) = &self.domain {
            write!(f, "; Domain={domain}")?;
        }
        if let Some(max_age) = self.max_age {
            write!(f, "; Max-Age={}", max_age.as_secs())?;
        }
        if self.http_only {
            f.write_str("; HttpOnly")?;
        }
        if self.secure {
            f.write_str("; Secure")?;
        }
        match self.same_site {
            Some(SameSite::Strict) => f.write_str("; SameSite=Strict")?,
            Some(SameSite::Lax) => f.write_str("; SameSite=Lax")?,
            Some(SameSite::None) => f.write_str("; SameSite=None")?,
            None => {}
        }
        Ok(())
    }
}

/// Express's `res.*` shortcuts, as functions.
///
/// ```
/// use vitesse::prelude::*;
///
/// async fn create(_req: Request) -> Response {
///     res::status(201).json(json!({ "ok": true }))
/// }
/// ```
pub mod res {
    use std::path::Path;

    use super::*;

    /// `res.status(code)`
    #[inline]
    pub fn status(status: impl IntoStatus) -> Response {
        Response::new().status(status)
    }

    /// `res.send(body)`
    #[inline]
    pub fn send(body: impl Into<Body>) -> Response {
        Response::new().send(body)
    }

    /// A text response.
    #[inline]
    pub fn text(text: impl Into<Body>) -> Response {
        Response::new().text(text)
    }

    /// An HTML response.
    #[inline]
    pub fn html(html: impl Into<Body>) -> Response {
        Response::new().html(html)
    }

    /// `res.json(value)`
    #[inline]
    pub fn json<T: Serialize>(value: T) -> Response {
        Response::new().json(value)
    }

    /// `res.redirect(url)` (302).
    pub fn redirect(location: impl Into<String>) -> Response {
        Redirect::to(location).into_response()
    }

    /// `res.sendStatus(code)`: the status, with its reason phrase as text.
    pub fn send_status(status: impl IntoStatus) -> Response {
        let status = status.into_status();
        Response::new()
            .status(status)
            .text(status.canonical_reason().unwrap_or(""))
    }

    /// `res.sendFile(path)`: sends a file (404 if it does not exist).
    pub async fn file(path: impl AsRef<Path>) -> Response {
        crate::static_files::send_file(path.as_ref(), None).await
    }

    /// `res.download(path, name)`: sends a file as an attachment.
    pub async fn download(path: impl AsRef<Path>, filename: &str) -> Response {
        let res = file(path).await;
        if res.status_code().is_success() {
            res.attachment(filename)
        } else {
            res
        }
    }
}
