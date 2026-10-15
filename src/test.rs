//! Testing an application without opening a port.
//!
//! ```
//! use vitesse::prelude::*;
//! use vitesse::test::TestClient;
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let mut app = App::new();
//! app.get("/hello/:name", |req: Request| async move {
//!     format!("Hello {}!", req.param("name").unwrap())
//! });
//!
//! let client = TestClient::new(app);
//! let res = client.get("/hello/Ada").await;
//! assert_eq!(res.status(), 200);
//! assert_eq!(res.text(), "Hello Ada!");
//! # }
//! ```

use std::collections::BTreeMap;
use std::future::IntoFuture;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};

use bytes::Bytes;
use http::header::{self, HeaderMap, HeaderName, HeaderValue};
use http::{Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::app::{App, AppService};
use crate::handler::BoxFuture;
use crate::request::{ReqBody, Request};
use crate::server::ResponseFuture;

/// A client that calls the application directly, in memory.
#[derive(Clone, Copy)]
pub struct TestClient {
    app: &'static AppService,
}

impl TestClient {
    /// Freezes the application in order to test it.
    pub fn new(app: App) -> Self {
        TestClient { app: app.build() }
    }

    /// A request with an arbitrary method.
    pub fn request(&self, method: Method, uri: &str) -> TestRequest {
        TestRequest {
            app: self.app,
            method,
            uri: uri.to_owned(),
            headers: HeaderMap::new(),
            body: Bytes::new(),
            jar: None,
        }
    }

    /// A client that keeps cookies between requests, like a browser
    /// (supertest's `agent`, Laravel's session-aware HTTP tests): the
    /// cookies set by a response are sent with the next requests, and
    /// removed when they expire (`Max-Age=0`). Each agent has its own jar.
    ///
    /// ```
    /// use vitesse::prelude::*;
    /// use vitesse::test::TestClient;
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let mut app = App::new();
    /// app.post("/login", |_| async { res::send("welcome").cookie(Cookie::new("user", "ada")) });
    /// app.get("/me", |req: Request| async move {
    ///     req.cookie("user").unwrap_or("nobody").to_owned()
    /// });
    ///
    /// let agent = TestClient::new(app).agent();
    /// agent.post("/login").await.assert_ok().assert_cookie("user", "ada");
    /// agent.get("/me").await.assert_text("ada");
    /// assert_eq!(agent.cookie("user").as_deref(), Some("ada"));
    /// # }
    /// ```
    pub fn agent(&self) -> TestAgent {
        TestAgent {
            client: *self,
            jar: Arc::default(),
        }
    }
}

type Jar = Arc<Mutex<BTreeMap<String, String>>>;

/// A [`TestClient`] with a cookie jar (see [`TestClient::agent`]). Clones
/// share the jar.
#[derive(Clone)]
pub struct TestAgent {
    client: TestClient,
    jar: Jar,
}

impl TestAgent {
    /// A request with an arbitrary method, with the cookies of the jar.
    pub fn request(&self, method: Method, uri: &str) -> TestRequest {
        let mut req = self.client.request(method, uri);
        req.jar = Some(self.jar.clone());
        req
    }

    /// `GET uri`
    pub fn get(&self, uri: &str) -> TestRequest {
        self.request(Method::GET, uri)
    }

    /// `POST uri`
    pub fn post(&self, uri: &str) -> TestRequest {
        self.request(Method::POST, uri)
    }

    /// `PUT uri`
    pub fn put(&self, uri: &str) -> TestRequest {
        self.request(Method::PUT, uri)
    }

    /// `PATCH uri`
    pub fn patch(&self, uri: &str) -> TestRequest {
        self.request(Method::PATCH, uri)
    }

    /// `DELETE uri`
    pub fn delete(&self, uri: &str) -> TestRequest {
        self.request(Method::DELETE, uri)
    }

    /// The value of a cookie of the jar.
    pub fn cookie(&self, name: &str) -> Option<String> {
        self.lock().get(name).cloned()
    }

    /// Every cookie of the jar, sorted by name.
    pub fn cookies(&self) -> Vec<(String, String)> {
        self.lock()
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }

    /// Puts a cookie in the jar, as if the application had set it.
    pub fn set_cookie(&self, name: &str, value: &str) -> &Self {
        self.lock().insert(name.to_owned(), value.to_owned());
        self
    }

    /// Empties the jar (a new browser session).
    pub fn clear_cookies(&self) -> &Self {
        self.lock().clear();
        self
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, String>> {
        self.jar.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl std::fmt::Debug for TestAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestAgent")
            .field("cookies", &self.cookies())
            .finish()
    }
}

/// The name, value and whether a `Set-Cookie` header deletes the cookie.
fn parse_set_cookie(header: &str) -> Option<(String, String, bool)> {
    let mut parts = header.split(';');
    let (name, value) = parts.next()?.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let expired = parts.any(|attr| {
        let attr = attr.trim();
        let lower = attr.to_ascii_lowercase();
        lower == "max-age=0" || lower.starts_with("max-age=-") || lower.contains("1970")
    });
    Some((
        name.to_owned(),
        value.trim().trim_matches('"').to_owned(),
        expired,
    ))
}

impl TestClient {
    /// `GET uri`
    pub fn get(&self, uri: &str) -> TestRequest {
        self.request(Method::GET, uri)
    }

    /// `POST uri`
    pub fn post(&self, uri: &str) -> TestRequest {
        self.request(Method::POST, uri)
    }

    /// `PUT uri`
    pub fn put(&self, uri: &str) -> TestRequest {
        self.request(Method::PUT, uri)
    }

    /// `PATCH uri`
    pub fn patch(&self, uri: &str) -> TestRequest {
        self.request(Method::PATCH, uri)
    }

    /// `DELETE uri`
    pub fn delete(&self, uri: &str) -> TestRequest {
        self.request(Method::DELETE, uri)
    }
}

/// A test request, sent with `.await` (or [`TestRequest::send`]).
#[must_use = "a test request is only sent when it is `.await`ed"]
pub struct TestRequest {
    app: &'static AppService,
    method: Method,
    uri: String,
    headers: HeaderMap,
    body: Bytes,
    jar: Option<Jar>,
}

impl TestRequest {
    /// Adds a cookie (`Cookie` header).
    pub fn cookie(mut self, name: &str, value: &str) -> Self {
        let pair = format!("{name}={value}");
        let value = match self
            .headers
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
        {
            Some(existing) => format!("{existing}; {pair}"),
            None => pair,
        };
        let value = HeaderValue::try_from(value).expect("invalid cookie");
        self.headers.insert(header::COOKIE, value);
        self
    }

    /// Adds `Authorization: Bearer <token>`.
    pub fn bearer(self, token: &str) -> Self {
        self.header("authorization", &format!("Bearer {token}"))
    }

    /// Adds a query string serialized from `value` (`?page=2&q=rust`).
    pub fn query<T: Serialize>(mut self, value: &T) -> Self {
        let query = serde_urlencoded::to_string(value).expect("query serialization failed");
        if !query.is_empty() {
            self.uri
                .push(if self.uri.contains('?') { '&' } else { '?' });
            self.uri.push_str(&query);
        }
        self
    }

    /// Adds a header.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        let name = HeaderName::try_from(name).expect("invalid header name");
        let value = HeaderValue::try_from(value).expect("invalid header value");
        self.headers.append(name, value);
        self
    }

    /// Sets a raw body.
    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = body.into();
        self
    }

    /// Sets a JSON body.
    pub fn json<T: Serialize>(mut self, value: &T) -> Self {
        self.body = serde_json::to_vec(value)
            .expect("JSON serialization failed")
            .into();
        self.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self
    }

    /// Sets a form body (`application/x-www-form-urlencoded`).
    pub fn form<T: Serialize>(mut self, value: &T) -> Self {
        self.body = serde_urlencoded::to_string(value)
            .expect("form serialization failed")
            .into();
        self.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        self
    }

    /// Sends the request.
    pub async fn send(mut self) -> TestResponse {
        if let Some(jar) = &self.jar {
            let cookies: Vec<(String, String)> = jar
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .map(|(n, v)| (n.clone(), v.clone()))
                .collect();
            for (name, value) in cookies {
                self = self.cookie(&name, &value);
            }
        }
        let jar = self.jar.take();
        let mut builder = http::Request::builder()
            .method(self.method.clone())
            .uri(&self.uri);
        if let Some(headers) = builder.headers_mut() {
            *headers = self.headers;
        }
        let (head, ()) = builder.body(()).expect("invalid request").into_parts();
        let peer = SocketAddr::from(([127, 0, 0, 1], 40000));
        let body = ReqBody::Stream(crate::Body::from(self.body));
        let req = Request::from_parts(head, body, Some(peer), false, &self.app.shared);
        let res = ResponseFuture::new(self.app, req).await;
        let (parts, body) = res.into_http().into_parts();
        let body = if self.method == Method::HEAD {
            Bytes::new()
        } else {
            body.to_bytes()
                .await
                .expect("failed to read the response body")
        };
        if let Some(jar) = jar {
            let mut jar = jar.lock().unwrap_or_else(PoisonError::into_inner);
            for value in parts.headers.get_all(header::SET_COOKIE) {
                if let Some((name, value, expired)) = value.to_str().ok().and_then(parse_set_cookie)
                {
                    if expired {
                        jar.remove(&name);
                    } else {
                        jar.insert(name, value);
                    }
                }
            }
        }
        TestResponse {
            status: parts.status,
            headers: parts.headers,
            body,
        }
    }
}

impl IntoFuture for TestRequest {
    type Output = TestResponse;
    type IntoFuture = BoxFuture<TestResponse>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.send())
    }
}

/// The response to a test request, with its body already read.
#[derive(Debug)]
pub struct TestResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl TestResponse {
    /// The status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// A header.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    /// All headers.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// The raw body.
    pub fn bytes(&self) -> &Bytes {
        &self.body
    }

    /// The body as text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The body deserialized from JSON (panics if it is invalid).
    pub fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("invalid JSON body ({e}): {}", self.text()))
    }
}

/// Fluent assertions (Laravel's HTTP tests, supertest's `expect`): each one
/// panics with the response in the message when it fails, and returns the
/// response to chain the next one.
///
/// ```
/// use vitesse::prelude::*;
/// use vitesse::test::TestClient;
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// let mut app = App::new();
/// app.post("/users", |_| async {
///     (201, Json(json!({ "user": { "id": 7, "name": "Ada", "tags": ["admin"] } })))
/// });
///
/// let client = TestClient::new(app);
/// client
///     .post("/users")
///     .json(&json!({ "name": "Ada" }))
///     .await
///     .assert_status(201)
///     .assert_header("content-type", "application/json")
///     .assert_json_path("user.name", "Ada")
///     .assert_json_path("user.tags.0", "admin")
///     .assert_json_includes(json!({ "user": { "id": 7 } }))
///     .assert_json_missing("user.password");
/// # }
/// ```
impl TestResponse {
    #[track_caller]
    fn fail(&self, what: std::fmt::Arguments<'_>) -> ! {
        let text = self.text();
        let body = if text.len() > 2000 {
            format!("{}…", &text[..text.floor_char_boundary_compat(2000)])
        } else {
            text
        };
        panic!("{what}\nresponse: {}\nbody: {body}", self.status)
    }

    /// The status is `status`.
    #[track_caller]
    pub fn assert_status(&self, status: impl crate::IntoStatus) -> &Self {
        let status = status.into_status();
        if self.status != status {
            self.fail(format_args!(
                "expected status {status}, got {}",
                self.status
            ));
        }
        self
    }

    /// The status is a success (`2xx`).
    #[track_caller]
    pub fn assert_ok(&self) -> &Self {
        if !self.status.is_success() {
            self.fail(format_args!("expected a 2xx status, got {}", self.status));
        }
        self
    }

    /// The status is `404 Not Found`.
    #[track_caller]
    pub fn assert_not_found(&self) -> &Self {
        self.assert_status(StatusCode::NOT_FOUND)
    }

    /// The response redirects (`3xx`) to `location`.
    #[track_caller]
    pub fn assert_redirect(&self, location: &str) -> &Self {
        if !self.status.is_redirection() {
            self.fail(format_args!("expected a redirect, got {}", self.status));
        }
        self.assert_header("location", location)
    }

    /// The header `name` is `value` (the content type may have parameters:
    /// `application/json` matches `application/json; charset=utf-8`).
    #[track_caller]
    pub fn assert_header(&self, name: &str, value: &str) -> &Self {
        match self.header(name) {
            Some(actual)
                if actual == value
                    || (name.eq_ignore_ascii_case("content-type")
                        && actual.split(';').next().map(str::trim) == Some(value)) => {}
            actual => self.fail(format_args!(
                "expected header {name}: {value}, got {actual:?}"
            )),
        }
        self
    }

    /// The header `name` is absent.
    #[track_caller]
    pub fn assert_header_missing(&self, name: &str) -> &Self {
        if let Some(actual) = self.header(name) {
            self.fail(format_args!("expected no header {name}, got {actual:?}"));
        }
        self
    }

    /// The body is exactly `text`.
    #[track_caller]
    pub fn assert_text(&self, text: &str) -> &Self {
        if self.text() != text {
            self.fail(format_args!("expected the body {text:?}"));
        }
        self
    }

    /// The body contains `text` (Laravel's `assertSee`).
    #[track_caller]
    pub fn assert_see(&self, text: &str) -> &Self {
        if !self.text().contains(text) {
            self.fail(format_args!("expected the body to contain {text:?}"));
        }
        self
    }

    /// The body does not contain `text` (Laravel's `assertDontSee`).
    #[track_caller]
    pub fn assert_dont_see(&self, text: &str) -> &Self {
        if self.text().contains(text) {
            self.fail(format_args!("expected the body not to contain {text:?}"));
        }
        self
    }

    /// The body is exactly this JSON.
    #[track_caller]
    pub fn assert_json(&self, expected: impl Into<Value>) -> &Self {
        let expected = expected.into();
        let actual = self.json_value();
        if actual != expected {
            self.fail(format_args!("expected the JSON {expected}"));
        }
        self
    }

    /// The value at `path` is `expected`. The path is made of keys and
    /// indexes separated by dots (`user.tags.0`), or is a JSON pointer
    /// (`/user/tags/0`).
    #[track_caller]
    pub fn assert_json_path(&self, path: &str, expected: impl Into<Value>) -> &Self {
        let expected = expected.into();
        match self.json_path(path) {
            Some(actual) if actual == expected => {}
            actual => self.fail(format_args!(
                "expected {expected} at {path:?}, got {}",
                actual.map_or("nothing".to_owned(), |v| v.to_string())
            )),
        }
        self
    }

    /// Nothing is at `path` (see [`assert_json_path`](Self::assert_json_path)).
    #[track_caller]
    pub fn assert_json_missing(&self, path: &str) -> &Self {
        if let Some(actual) = self.json_path(path) {
            self.fail(format_args!("expected nothing at {path:?}, got {actual}"));
        }
        self
    }

    /// The array or the object at `path` has `count` elements.
    #[track_caller]
    pub fn assert_json_count(&self, path: &str, count: usize) -> &Self {
        let actual = match self.json_path(path) {
            Some(Value::Array(a)) => Some(a.len()),
            Some(Value::Object(o)) => Some(o.len()),
            _ => None,
        };
        if actual != Some(count) {
            self.fail(format_args!(
                "expected {count} elements at {path:?}, got {actual:?}"
            ));
        }
        self
    }

    /// The body contains this JSON: every key of `subset` has the same
    /// value, recursively, other keys being ignored; arrays must match
    /// element by element (Laravel's `assertJson`).
    #[track_caller]
    pub fn assert_json_includes(&self, subset: impl Into<Value>) -> &Self {
        let subset = subset.into();
        if !includes(&self.json_value(), &subset) {
            self.fail(format_args!("expected the JSON to include {subset}"));
        }
        self
    }

    /// The status is `422` and the validation errors name these fields
    /// (Laravel's `assertJsonValidationErrors`), in the format of
    /// [`validate`](crate::validate) as in the problem details of
    /// [`App::problem_details`](crate::App::problem_details).
    #[track_caller]
    pub fn assert_validation_errors(&self, fields: &[&str]) -> &Self {
        self.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        let json = self.json_value();
        for field in fields {
            if json.get("details").and_then(|d| d.get(*field)).is_none() {
                self.fail(format_args!("expected a validation error for {field:?}"));
            }
        }
        self
    }

    /// The response sets the cookie `name` to `value`.
    #[track_caller]
    pub fn assert_cookie(&self, name: &str, value: &str) -> &Self {
        match self.cookie(name) {
            Some(actual) if actual == value => {}
            actual => self.fail(format_args!(
                "expected the cookie {name}={value}, got {actual:?}"
            )),
        }
        self
    }

    /// The value of a cookie set by the response (`Set-Cookie`).
    pub fn cookie(&self, name: &str) -> Option<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok().and_then(parse_set_cookie))
            .find(|(n, _, _)| n == name)
            .map(|(_, value, _)| value)
    }

    /// The value at `path` in the JSON body, if any (see
    /// [`assert_json_path`](Self::assert_json_path)).
    pub fn json_path(&self, path: &str) -> Option<Value> {
        let json: Value = serde_json::from_slice(&self.body).ok()?;
        let pointer = if path.starts_with('/') || path.is_empty() {
            path.to_owned()
        } else {
            path.split('.')
                .map(|key| format!("/{}", key.replace('~', "~0").replace('/', "~1")))
                .collect()
        };
        json.pointer(&pointer).cloned()
    }

    #[track_caller]
    fn json_value(&self) -> Value {
        match serde_json::from_slice(&self.body) {
            Ok(value) => value,
            Err(e) => self.fail(format_args!("expected a JSON body ({e})")),
        }
    }
}

/// `subset` is included in `value`.
fn includes(value: &Value, subset: &Value) -> bool {
    match (value, subset) {
        (Value::Object(value), Value::Object(subset)) => subset
            .iter()
            .all(|(key, sub)| value.get(key).is_some_and(|v| includes(v, sub))),
        (Value::Array(value), Value::Array(subset)) => {
            value.len() == subset.len() && value.iter().zip(subset).all(|(v, s)| includes(v, s))
        }
        _ => value == subset,
    }
}

trait FloorCharBoundary {
    fn floor_char_boundary_compat(&self, index: usize) -> usize;
}

impl FloorCharBoundary for str {
    fn floor_char_boundary_compat(&self, mut index: usize) -> usize {
        while !self.is_char_boundary(index) {
            index -= 1;
        }
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn set_cookie_headers() {
        assert_eq!(
            parse_set_cookie("sid=abc; Path=/; HttpOnly"),
            Some(("sid".into(), "abc".into(), false))
        );
        assert_eq!(
            parse_set_cookie("sid=; Path=/; Max-Age=0"),
            Some(("sid".into(), "".into(), true))
        );
        assert_eq!(parse_set_cookie("garbage"), None);
    }

    #[test]
    fn json_inclusion() {
        let value = json!({ "a": 1, "b": { "c": [1, { "d": 2, "e": 3 }] } });
        assert!(includes(&value, &json!({ "b": { "c": [1, { "d": 2 }] } })));
        assert!(!includes(&value, &json!({ "b": { "c": [1] } })));
        assert!(!includes(&value, &json!({ "z": null })));
    }
}
