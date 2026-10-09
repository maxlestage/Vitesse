//! Les réponses HTTP : [`Response`], le trait [`IntoResponse`] et les helpers
//! façon Express du module [`res`].

use std::borrow::Cow;
use std::fmt;
use std::time::Duration;

use bytes::Bytes;
use http::StatusCode;
use http::header::{self, HeaderMap, HeaderName, HeaderValue};
use serde::Serialize;

use crate::body::Body;
use crate::error::Error;

pub(crate) const TEXT: &str = "text/plain; charset=utf-8";
pub(crate) const HTML: &str = "text/html; charset=utf-8";
pub(crate) const JSON: &str = "application/json";
pub(crate) const OCTETS: &str = "application/octet-stream";

/// Une réponse HTTP, construite comme avec Express :
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
    pub(crate) inner: http::Response<Body>,
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
            .field("status", &self.inner.status())
            .field("headers", self.inner.headers())
            .field("body", self.inner.body())
            .finish()
    }
}

impl Response {
    /// Une réponse `200 OK` vide.
    #[inline]
    pub fn new() -> Self {
        Response {
            inner: http::Response::new(Body::empty()),
            error: None,
        }
    }

    /// Change le statut (`res.status(404)` en Express).
    #[inline]
    pub fn status(mut self, status: impl IntoStatus) -> Self {
        *self.inner.status_mut() = status.into_status();
        self
    }

    /// Définit un en-tête (remplace la valeur existante). Les noms ou valeurs
    /// invalides sont ignorés.
    #[inline]
    pub fn header<K, V>(mut self, name: K, value: V) -> Self
    where
        K: TryInto<HeaderName>,
        V: TryInto<HeaderValue>,
    {
        self.set_header(name, value);
        self
    }

    /// Ajoute un en-tête sans remplacer les valeurs existantes.
    pub fn append_header<K, V>(mut self, name: K, value: V) -> Self
    where
        K: TryInto<HeaderName>,
        V: TryInto<HeaderValue>,
    {
        if let (Ok(name), Ok(value)) = (name.try_into(), value.try_into()) {
            self.inner.headers_mut().append(name, value);
        }
        self
    }

    /// Définit le `Content-Type` (`res.type()` en Express).
    pub fn content_type(self, value: &str) -> Self {
        self.header(header::CONTENT_TYPE, value)
    }

    /// Définit le corps, sans toucher au `Content-Type`.
    #[inline]
    pub fn send(mut self, body: impl Into<Body>) -> Self {
        *self.inner.body_mut() = body.into();
        self
    }

    /// Corps texte (`text/plain; charset=utf-8`).
    #[inline]
    pub fn text(self, text: impl Into<Body>) -> Self {
        self.static_type(TEXT).send(text)
    }

    /// Corps HTML (`text/html; charset=utf-8`).
    #[inline]
    pub fn html(self, html: impl Into<Body>) -> Self {
        self.static_type(HTML).send(html)
    }

    /// Sérialise `value` en JSON (`res.json()` en Express).
    #[inline]
    pub fn json<T: Serialize>(self, value: T) -> Self {
        let mut buf = Vec::with_capacity(128);
        match serde_json::to_writer(&mut buf, &value) {
            Ok(()) => self.json_bytes(buf),
            Err(e) => Error::internal("échec de la sérialisation JSON")
                .with_source(e)
                .into_response(),
        }
    }

    #[inline]
    pub(crate) fn json_bytes(self, json: impl Into<Body>) -> Self {
        self.static_type(JSON).send(json)
    }

    #[inline]
    pub(crate) fn static_type(mut self, value: &'static str) -> Self {
        self.inner
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static(value));
        self
    }

    /// Ajoute un cookie (`res.cookie()` en Express).
    pub fn cookie(self, cookie: Cookie) -> Self {
        self.append_header(header::SET_COOKIE, cookie.to_string())
    }

    /// Supprime un cookie côté client (`res.clearCookie()` en Express).
    pub fn clear_cookie(self, name: &str) -> Self {
        self.cookie(Cookie::new(name, "").path("/").max_age(Duration::ZERO))
    }

    /// Demande au navigateur de télécharger la réponse sous ce nom de fichier
    /// (`res.attachment()` en Express).
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

    /// Le statut de la réponse.
    #[inline]
    pub fn status_code(&self) -> StatusCode {
        self.inner.status()
    }

    /// Modifie le statut en place.
    pub fn set_status(&mut self, status: impl IntoStatus) -> &mut Self {
        *self.inner.status_mut() = status.into_status();
        self
    }

    /// Les en-têtes.
    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        self.inner.headers()
    }

    /// Les en-têtes, modifiables.
    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        self.inner.headers_mut()
    }

    /// Lit un en-tête.
    pub fn get_header(&self, name: &str) -> Option<&str> {
        self.inner.headers().get(name).and_then(|v| v.to_str().ok())
    }

    /// Définit un en-tête en place. Les noms ou valeurs invalides sont ignorés.
    #[inline]
    pub fn set_header<K, V>(&mut self, name: K, value: V) -> &mut Self
    where
        K: TryInto<HeaderName>,
        V: TryInto<HeaderValue>,
    {
        if let (Ok(name), Ok(value)) = (name.try_into(), value.try_into()) {
            self.inner.headers_mut().insert(name, value);
        }
        self
    }

    /// Le corps.
    pub fn body(&self) -> &Body {
        self.inner.body()
    }

    /// Le corps, modifiable.
    pub fn body_mut(&mut self) -> &mut Body {
        self.inner.body_mut()
    }

    /// Consomme la réponse et renvoie son corps.
    pub fn into_body(self) -> Body {
        self.inner.into_body()
    }

    /// Les extensions (données typées attachées à la réponse).
    pub fn extensions(&self) -> &http::Extensions {
        self.inner.extensions()
    }

    /// Les extensions, modifiables.
    pub fn extensions_mut(&mut self) -> &mut http::Extensions {
        self.inner.extensions_mut()
    }

    /// L'erreur à l'origine de cette réponse, si elle vient d'une [`Error`].
    pub fn error(&self) -> Option<&Error> {
        self.error.as_deref()
    }

    /// Retire l'erreur attachée à la réponse.
    pub fn take_error(&mut self) -> Option<Error> {
        self.error.take().map(|e| *e)
    }

    /// Convertit en [`http::Response`].
    pub fn into_http(self) -> http::Response<Body> {
        if let Some(err) = &self.error {
            if err.status().is_server_error() {
                if let Some(source) = err.source() {
                    eprintln!("[vitesse] erreur {} : {source}", err.status().as_u16());
                }
            }
        }
        self.inner
    }

    /// Construit à partir d'une [`http::Response`].
    pub fn from_http(inner: http::Response<Body>) -> Self {
        Response { inner, error: None }
    }
}

/// Convertit un code (`404`, `StatusCode::NOT_FOUND`…) en [`StatusCode`].
/// Un code invalide devient `500`.
pub trait IntoStatus {
    /// Effectue la conversion.
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

/// Tout ce qu'un handler peut renvoyer.
///
/// | Type renvoyé | Réponse |
/// |---|---|
/// | `&'static str`, `String` | `200`, `text/plain` |
/// | [`Json<T>`], `serde_json::Value` | `200`, `application/json` |
/// | [`Html<T>`] | `200`, `text/html` |
/// | `Bytes`, `Vec<u8>` | `200`, `application/octet-stream` |
/// | `()` | `200`, corps vide |
/// | [`StatusCode`] | ce statut, avec sa raison en texte |
/// | `(statut, T)` | `T` avec ce statut (ex. `(201, Json(user))`) |
/// | `Option<T>` | `T`, ou `404` si `None` |
/// | `Result<T, E>` | `T` ou l'erreur `E` |
/// | [`Error`] | le statut de l'erreur et `{"error": "..."}` |
pub trait IntoResponse {
    /// Effectue la conversion.
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
        Response::new().static_type(OCTETS).send(self)
    }
}

impl IntoResponse for Vec<u8> {
    #[inline]
    fn into_response(self) -> Response {
        Response::new().static_type(OCTETS).send(self)
    }
}

impl IntoResponse for &'static [u8] {
    #[inline]
    fn into_response(self) -> Response {
        Response::new().static_type(OCTETS).send(self)
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

/// Réponse JSON : `Json(valeur)`.
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

/// Réponse HTML : `Html("<h1>Salut</h1>")`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Html<T>(pub T);

impl<T: Into<Body>> IntoResponse for Html<T> {
    #[inline]
    fn into_response(self) -> Response {
        Response::new().html(self.0)
    }
}

/// Redirection HTTP.
#[derive(Debug, Clone)]
pub struct Redirect {
    status: StatusCode,
    location: String,
}

impl Redirect {
    /// `302 Found` (le défaut d'Express).
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

    /// `303 See Other` (typiquement après un POST).
    pub fn see_other(location: impl Into<String>) -> Self {
        Redirect {
            status: StatusCode::SEE_OTHER,
            location: location.into(),
        }
    }

    /// `307 Temporary Redirect` (conserve la méthode).
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
            Err(_) => Error::internal("URL de redirection invalide").into_response(),
        }
    }
}

/// Attribut `SameSite` d'un cookie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    /// `SameSite=Strict`
    Strict,
    /// `SameSite=Lax`
    Lax,
    /// `SameSite=None` (nécessite `Secure`)
    None,
}

/// Un cookie à envoyer avec [`Response::cookie`].
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
    /// Crée un cookie avec `Path=/`.
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

    /// Attribut `Path`.
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Attribut `Domain`.
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// Attribut `Max-Age`.
    pub fn max_age(mut self, max_age: Duration) -> Self {
        self.max_age = Some(max_age);
        self
    }

    /// Attribut `Secure`.
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// Attribut `HttpOnly`.
    pub fn http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }

    /// Attribut `SameSite`.
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

/// Les raccourcis `res.*` d'Express, sous forme de fonctions.
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

    /// Réponse texte.
    #[inline]
    pub fn text(text: impl Into<Body>) -> Response {
        Response::new().text(text)
    }

    /// Réponse HTML.
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

    /// `res.sendStatus(code)` : le statut et sa raison en texte.
    pub fn send_status(status: impl IntoStatus) -> Response {
        let status = status.into_status();
        Response::new()
            .status(status)
            .text(status.canonical_reason().unwrap_or(""))
    }

    /// `res.sendFile(path)` : envoie un fichier (404 s'il n'existe pas).
    pub async fn file(path: impl AsRef<Path>) -> Response {
        crate::static_files::send_file(path.as_ref(), None).await
    }

    /// `res.download(path, name)` : envoie un fichier en pièce jointe.
    pub async fn download(path: impl AsRef<Path>, filename: &str) -> Response {
        let res = file(path).await;
        if res.status_code().is_success() {
            res.attachment(filename)
        } else {
            res
        }
    }
}
