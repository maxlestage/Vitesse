//! Problem Details for HTTP APIs ([RFC 9457](https://www.rfc-editor.org/rfc/rfc9457)):
//! errors as `application/problem+json`, the format of ASP.NET Core, Spring
//! and API Platform, which clients and gateways know how to read.
//!
//! Turn it on for the whole application with
//! [`App::problem_details`](crate::App::problem_details): every response
//! produced from an [`Error`] (`404`, `405`, validation `422`, panics, the
//! errors of your handlers) is then sent as a problem document, with the
//! request's path as its `instance`:
//!
//! ```
//! use vitesse::prelude::*;
//!
//! let mut app = App::new();
//! app.problem_details();
//! app.get("/accounts/:id", |_req: Request| async move {
//!     Err::<String, _>(Error::forbidden("Your balance is too low").with_details(json!({ "balance": 30 })))
//! });
//! // GET /accounts/12 → 403, content-type: application/problem+json
//! // {"type":"about:blank","title":"Forbidden","status":403,
//! //  "detail":"Your balance is too low","instance":"/accounts/12","details":{"balance":30}}
//! ```
//!
//! A handler can also answer with a [`Problem`] of its own, with a `type`
//! URI that documents the problem and extension members:
//!
//! ```
//! use vitesse::prelude::*;
//! use vitesse::problem::Problem;
//!
//! let mut app = App::new();
//! app.post("/transfers", |_req: Request| async move {
//!     Problem::new(403)
//!         .type_uri("https://example.com/probs/out-of-credit")
//!         .title("You do not have enough credit.")
//!         .detail("Your current balance is 30, but that costs 50.")
//!         .extension("balance", 30)
//! });
//! ```
//!
//! Without `app.problem_details()`, errors keep Vitesse's compact format,
//! `{"error":"…","details":…}`, and nothing changes on the path of a request.

use std::fmt;

use http::StatusCode;
use http::header::{self, HeaderValue};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::Error;
use crate::handler::{BoxFuture, Middleware, Next};
use crate::request::Request;
use crate::response::{IntoResponse, IntoStatus, Response};

/// The media type of a problem document.
pub const CONTENT_TYPE: &str = "application/problem+json";

/// The members defined by RFC 9457, which extensions cannot replace.
const RESERVED: [&str; 5] = ["type", "title", "status", "detail", "instance"];

/// A problem document (RFC 9457): `type`, `title`, `status`, `detail`,
/// `instance`, and extension members. It is a response by itself, sent as
/// `application/problem+json`.
///
/// ```
/// use vitesse::problem::Problem;
/// use vitesse::json;
///
/// let problem = Problem::new(422)
///     .type_uri("https://example.com/probs/invalid-order")
///     .detail("The order is empty")
///     .extension("items", json!([]));
/// assert_eq!(problem.status(), 422);
/// assert_eq!(problem.to_json()["title"], "Unprocessable Entity");
/// assert_eq!(problem.to_json()["items"], json!([]));
/// ```
#[derive(Clone, PartialEq)]
pub struct Problem {
    status: StatusCode,
    type_uri: Option<String>,
    title: Option<String>,
    detail: Option<String>,
    instance: Option<String>,
    extensions: Map<String, Value>,
}

impl fmt::Debug for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Problem")
            .field("status", &self.status)
            .field("type", &self.type_uri)
            .field("title", &self.title)
            .field("detail", &self.detail)
            .field("instance", &self.instance)
            .field("extensions", &self.extensions)
            .finish()
    }
}

impl Problem {
    /// A problem with this status; its `type` is `about:blank` and its
    /// `title` the reason phrase of the status until you set them.
    pub fn new(status: impl IntoStatus) -> Self {
        Problem {
            status: status.into_status(),
            type_uri: None,
            title: None,
            detail: None,
            instance: None,
            extensions: Map::new(),
        }
    }

    /// A URI that identifies the kind of problem, ideally a page that
    /// documents it (`about:blank` by default).
    pub fn type_uri(mut self, uri: impl Into<String>) -> Self {
        self.type_uri = Some(uri.into());
        self
    }

    /// A short summary of the kind of problem, the same for every
    /// occurrence (the reason phrase of the status by default).
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// An explanation of this occurrence of the problem.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// A URI that identifies this occurrence (the request's path, with
    /// [`App::problem_details`](crate::App::problem_details)).
    pub fn instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    /// Adds an extension member. The standard members (`type`, `title`,
    /// `status`, `detail`, `instance`) cannot be replaced this way: use
    /// their own methods.
    pub fn extension(mut self, name: impl Into<String>, value: impl Into<Value>) -> Self {
        let name = name.into();
        if !RESERVED.contains(&name.as_str()) {
            self.extensions.insert(name, value.into());
        }
        self
    }

    /// Changes the status.
    pub fn with_status(mut self, status: impl IntoStatus) -> Self {
        self.status = status.into_status();
        self
    }

    /// The status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The `type` member (`about:blank` when it was not set).
    pub fn get_type(&self) -> &str {
        self.type_uri.as_deref().unwrap_or("about:blank")
    }

    /// The `title` member (the reason phrase of the status when it was not
    /// set).
    pub fn get_title(&self) -> &str {
        self.title
            .as_deref()
            .unwrap_or_else(|| self.status.canonical_reason().unwrap_or("Error"))
    }

    /// The `detail` member, if any.
    pub fn get_detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    /// The `instance` member, if any.
    pub fn get_instance(&self) -> Option<&str> {
        self.instance.as_deref()
    }

    /// An extension member, if any.
    pub fn get_extension(&self, name: &str) -> Option<&Value> {
        self.extensions.get(name)
    }

    /// The problem document, as JSON.
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self.document()).unwrap_or(Value::Null)
    }

    fn document(&self) -> Document<'_> {
        Document {
            type_uri: self.get_type(),
            title: self.get_title(),
            status: self.status.as_u16(),
            detail: self.detail.as_deref(),
            instance: self.instance.as_deref(),
            extensions: &self.extensions,
        }
    }
}

/// The JSON document, with the standard members first.
#[derive(Serialize)]
struct Document<'a> {
    #[serde(rename = "type")]
    type_uri: &'a str,
    title: &'a str,
    status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instance: Option<&'a str>,
    #[serde(flatten)]
    extensions: &'a Map<String, Value>,
}

impl Serialize for Problem {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.document().serialize(serializer)
    }
}

/// An [`Error`] as a problem: its status, its message as `detail` (unless
/// it only repeats the reason phrase), and its
/// [`details`](Error::details), validation messages included, in the
/// `details` extension member.
impl From<Error> for Problem {
    fn from(error: Error) -> Self {
        Problem::from(&error)
    }
}

impl From<&Error> for Problem {
    fn from(error: &Error) -> Self {
        let mut problem = Problem::new(error.status());
        if Some(error.message()) != error.status().canonical_reason() {
            problem.detail = Some(error.message().to_owned());
        }
        if let Some(details) = error.details() {
            problem
                .extensions
                .insert("details".to_owned(), details.clone());
        }
        problem
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let body = serde_json::to_vec(&self.document()).unwrap_or_default();
        Response::new()
            .status(self.status)
            .header(header::CONTENT_TYPE, HeaderValue::from_static(CONTENT_TYPE))
            .send(body)
    }
}

/// Customizes the problems made from errors (see
/// [`App::problem_details_with`](crate::App::problem_details_with)).
pub(crate) type Customize = Box<dyn Fn(&Error, Problem) -> Problem + Send + Sync>;

/// The middleware installed by `app.problem_details()`: rewrites the
/// responses produced from an [`Error`] as problem documents.
pub(crate) struct ProblemDetails {
    pub(crate) customize: Option<Customize>,
}

impl Middleware for ProblemDetails {
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response> {
        let path = req.path().to_owned();
        let future = next.run(req);
        Box::pin(async move {
            let mut res = future.await;
            let Some(error) = res.take_error() else {
                return res;
            };
            let mut problem = Problem::from(&error).instance(path);
            if let Some(customize) = &self.customize {
                problem = customize(&error, problem);
            }
            let mut out = problem.into_response();
            // Headers set on the way (CORS, `Allow`, `Retry-After`…) stay.
            for (name, value) in res.headers() {
                if name != header::CONTENT_TYPE
                    && name != header::CONTENT_LENGTH
                    && !out.headers().contains_key(name)
                {
                    out.headers_mut().append(name.clone(), value.clone());
                }
            }
            // `app.on_error`, if any, still sees the error and has the last word.
            out.error = Some(Box::new(error));
            out
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_and_members() {
        let doc = Problem::new(404).to_json();
        assert_eq!(
            doc,
            json!({ "type": "about:blank", "title": "Not Found", "status": 404 })
        );
        let doc = Problem::new(StatusCode::CONFLICT)
            .type_uri("https://example.com/probs/taken")
            .title("Taken")
            .detail("ada is taken")
            .instance("/users")
            .extension("suggestions", json!(["ada2"]))
            .extension("status", 200)
            .to_json();
        assert_eq!(doc["type"], "https://example.com/probs/taken");
        assert_eq!(doc["status"], 409);
        assert_eq!(doc["detail"], "ada is taken");
        assert_eq!(doc["instance"], "/users");
        assert_eq!(doc["suggestions"][0], "ada2");
    }

    #[test]
    fn from_error() {
        let problem = Problem::from(Error::not_found("Cannot GET /x"));
        assert_eq!(problem.get_detail(), Some("Cannot GET /x"));
        let problem = Problem::from(Error::from_status(500));
        assert_eq!(problem.get_detail(), None);
        assert_eq!(problem.get_title(), "Internal Server Error");
        let problem = Problem::from(
            Error::unprocessable("Validation failed").with_details(json!({ "a": ["b"] })),
        );
        assert_eq!(
            problem.get_extension("details"),
            Some(&json!({ "a": ["b"] }))
        );
    }

    #[test]
    fn order_of_members() {
        let body = serde_json::to_string(&Problem::new(400).detail("d").extension("a", 1)).unwrap();
        assert_eq!(
            body,
            r#"{"type":"about:blank","title":"Bad Request","status":400,"detail":"d","a":1}"#
        );
    }
}
