//! Middlewares prêts à l'emploi : `logger`, `cors`, `helmet`, `timeout`,
//! `serve_static`.

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use http::header::{self, HeaderName, HeaderValue};
use http::{Method, StatusCode};

use crate::error::Error;
use crate::handler::{BoxFuture, Middleware, Next};
use crate::request::Request;
use crate::response::{IntoResponse, Response};
use crate::static_files::ServeDir;

/// Journalise chaque requête, façon `morgan('dev')` :
///
/// ```text
/// GET /users/42 200 0.084 ms
/// ```
pub fn logger() -> impl Middleware {
    let color = std::io::stdout().is_terminal();
    move |req: Request, next: Next| async move {
        let start = Instant::now();
        let method = req.method().clone();
        let uri = req.uri().clone();
        let res = next.run(req).await;
        let status = res.status_code().as_u16();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let line = if color {
            let code = match status {
                500.. => 31,
                400.. => 33,
                300.. => 36,
                _ => 32,
            };
            format!("{method} {uri} \x1b[{code}m{status}\x1b[0m {ms:.3} ms\n")
        } else {
            format!("{method} {uri} {status} {ms:.3} ms\n")
        };
        let _ = std::io::stdout().lock().write_all(line.as_bytes());
        res
    }
}

/// Coupe les requêtes trop longues avec une `503 Service Unavailable`.
pub fn timeout(duration: Duration) -> impl Middleware {
    move |req: Request, next: Next| async move {
        match tokio::time::timeout(duration, next.run(req)).await {
            Ok(res) => res,
            Err(_) => Error::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "délai de traitement dépassé",
            )
            .into_response(),
        }
    }
}

/// Sert les fichiers d'un dossier, en laissant passer vers les routes ce qui
/// n'existe pas (`app.use(express.static('public'))`).
pub fn serve_static(dir: impl Into<PathBuf>) -> ServeDir {
    ServeDir::new(dir)
}

/// En-têtes de sécurité par défaut, façon `helmet()`.
pub fn helmet() -> impl Middleware {
    const HEADERS: [(&str, &str); 9] = [
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "SAMEORIGIN"),
        ("x-dns-prefetch-control", "off"),
        ("x-download-options", "noopen"),
        ("x-permitted-cross-domain-policies", "none"),
        ("x-xss-protection", "0"),
        ("referrer-policy", "no-referrer"),
        ("cross-origin-opener-policy", "same-origin"),
        (
            "strict-transport-security",
            "max-age=31536000; includeSubDomains",
        ),
    ];
    |req: Request, next: Next| async move {
        let mut res = next.run(req).await;
        let headers = res.headers_mut();
        for (name, value) in HEADERS {
            headers
                .entry(HeaderName::from_static(name))
                .or_insert(HeaderValue::from_static(value));
        }
        res
    }
}

/// CORS avec la configuration par défaut d'Express (`cors()`) : toutes les
/// origines, méthodes `GET, HEAD, PUT, PATCH, POST, DELETE`.
pub fn cors() -> Cors {
    Cors::new()
}

/// Middleware CORS configurable.
///
/// ```
/// use vitesse::prelude::*;
/// use std::time::Duration;
///
/// let mut app = App::new();
/// app.middleware(
///     middleware::cors()
///         .allow_origin("https://monsite.fr")
///         .allow_credentials(true)
///         .max_age(Duration::from_secs(600)),
/// );
/// ```
#[derive(Debug, Clone)]
pub struct Cors {
    origins: Vec<String>,
    methods: String,
    headers: Option<String>,
    expose: Option<String>,
    credentials: bool,
    max_age: Option<u64>,
}

impl Default for Cors {
    fn default() -> Self {
        Cors::new()
    }
}

impl Cors {
    /// Toutes les origines autorisées.
    pub fn new() -> Self {
        Cors {
            origins: Vec::new(),
            methods: "GET, HEAD, PUT, PATCH, POST, DELETE".into(),
            headers: None,
            expose: None,
            credentials: false,
            max_age: None,
        }
    }

    /// N'autorise que cette origine (cumulable). `"*"` autorise tout.
    pub fn allow_origin(mut self, origin: &str) -> Self {
        if origin != "*" {
            self.origins.push(origin.trim_end_matches('/').to_owned());
        }
        self
    }

    /// Méthodes autorisées.
    pub fn allow_methods<I>(mut self, methods: I) -> Self
    where
        I: IntoIterator<Item = Method>,
    {
        self.methods = methods
            .into_iter()
            .map(|m| m.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        self
    }

    /// En-têtes autorisés (par défaut : ceux demandés par le navigateur).
    pub fn allow_headers(mut self, headers: &str) -> Self {
        self.headers = Some(headers.to_owned());
        self
    }

    /// En-têtes de réponse lisibles par le navigateur.
    pub fn expose_headers(mut self, headers: &str) -> Self {
        self.expose = Some(headers.to_owned());
        self
    }

    /// Autorise les cookies et l'authentification.
    pub fn allow_credentials(mut self, allow: bool) -> Self {
        self.credentials = allow;
        self
    }

    /// Durée de cache des requêtes préliminaires (`OPTIONS`).
    pub fn max_age(mut self, max_age: Duration) -> Self {
        self.max_age = Some(max_age.as_secs());
        self
    }

    /// La valeur d'`Access-Control-Allow-Origin` pour cette origine, si elle est autorisée.
    fn allowed_origin(&self, origin: &str) -> Option<HeaderValue> {
        if self.origins.is_empty() {
            if self.credentials {
                // `*` est interdit avec les cookies : on renvoie l'origine.
                return HeaderValue::from_str(origin).ok();
            }
            return Some(HeaderValue::from_static("*"));
        }
        if self.origins.iter().any(|o| o == origin) {
            return HeaderValue::from_str(origin).ok();
        }
        None
    }

    fn apply(&self, res: &mut Response, origin: HeaderValue) {
        let echo = origin != "*";
        let headers = res.headers_mut();
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        if echo {
            headers.append(header::VARY, HeaderValue::from_static("Origin"));
        }
        if self.credentials {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
                HeaderValue::from_static("true"),
            );
        }
        if let Some(expose) = self
            .expose
            .as_deref()
            .and_then(|e| HeaderValue::from_str(e).ok())
        {
            headers.insert(header::ACCESS_CONTROL_EXPOSE_HEADERS, expose);
        }
    }
}

impl Middleware for Cors {
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response> {
        // Pas d'en-tête `Origin` : ce n'est pas une requête CORS.
        let Some(origin) = req.header(header::ORIGIN) else {
            return next.run(req);
        };
        let allowed = self.allowed_origin(origin);
        let preflight = *req.method() == Method::OPTIONS
            && req
                .headers()
                .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD);

        if preflight {
            let mut res = Response::new().status(StatusCode::NO_CONTENT);
            if let Some(origin) = allowed {
                self.apply(&mut res, origin);
                res.set_header(header::ACCESS_CONTROL_ALLOW_METHODS, self.methods.as_str());
                let headers = self
                    .headers
                    .as_deref()
                    .or_else(|| req.header(header::ACCESS_CONTROL_REQUEST_HEADERS));
                if let Some(h) = headers {
                    res.set_header(header::ACCESS_CONTROL_ALLOW_HEADERS, h);
                }
                if let Some(max_age) = self.max_age {
                    res.set_header(header::ACCESS_CONTROL_MAX_AGE, max_age);
                }
            }
            return Box::pin(std::future::ready(res));
        }

        Box::pin(async move {
            let mut res = next.run(req).await;
            if let Some(origin) = allowed {
                self.apply(&mut res, origin);
            }
            res
        })
    }
}
