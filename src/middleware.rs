//! Ready-to-use middlewares: `logger`, `cors`, `helmet`, `timeout`,
//! `serve_static` and, with the `compress` feature, `compress`.

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

/// Logs every request, like `morgan('dev')`:
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

/// Cuts off requests that take too long with a `503 Service Unavailable`.
pub fn timeout(duration: Duration) -> impl Middleware {
    move |req: Request, next: Next| async move {
        match tokio::time::timeout(duration, next.run(req)).await {
            Ok(res) => res,
            Err(_) => {
                Error::new(StatusCode::SERVICE_UNAVAILABLE, "request timed out").into_response()
            }
        }
    }
}

/// Changes the maximum size of the request body read into memory for the
/// routes it covers (`express.json({ limit })`, axum's `DefaultBodyLimit`,
/// actix-web's `PayloadConfig`): use it on a route with
/// [`.with()`](crate::HandlerExt::with) or on a [`Router`](crate::Router).
/// Beyond the limit, reading the body fails with `413 Payload Too Large`.
///
/// ```
/// use vitesse::prelude::*;
///
/// async fn upload(req: Request) -> vitesse::Result<String> {
///     let bytes = req.bytes().await?;
///     Ok(format!("{} bytes", bytes.len()))
/// }
///
/// let mut app = App::new(); // 1 MiB by default
/// app.post("/upload", upload.with(middleware::body_limit(50 * 1024 * 1024)));
/// ```
///
/// Over HTTP/3, request bodies are read before routing, within
/// [`App::body_limit`](crate::App::body_limit): raise that one if HTTP/3
/// clients upload large files.
pub fn body_limit(bytes: usize) -> BodyLimit {
    BodyLimit(bytes)
}

/// The middleware returned by [`body_limit`].
#[derive(Debug, Clone, Copy)]
pub struct BodyLimit(usize);

impl Middleware for BodyLimit {
    #[inline]
    fn handle(&'static self, mut req: Request, next: Next) -> BoxFuture<Response> {
        req.set_body_limit(self.0);
        next.run(req)
    }
}

/// Serves the files of a directory, letting requests for files that don't
/// exist fall through to the routes (`app.use(express.static('public'))`).
pub fn serve_static(dir: impl Into<PathBuf>) -> ServeDir {
    ServeDir::new(dir)
}

/// Default security headers, like `helmet()`.
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

/// CORS with Express's default configuration (`cors()`): all origins,
/// methods `GET, HEAD, PUT, PATCH, POST, DELETE`.
pub fn cors() -> Cors {
    Cors::new()
}

/// Configurable CORS middleware.
///
/// ```
/// use vitesse::prelude::*;
/// use std::time::Duration;
///
/// let mut app = App::new();
/// app.middleware(
///     middleware::cors()
///         .allow_origin("https://example.com")
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
    /// Allows all origins.
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

    /// Allows only this origin (can be called several times). `"*"` allows
    /// every origin.
    pub fn allow_origin(mut self, origin: &str) -> Self {
        if origin != "*" {
            self.origins.push(origin.trim_end_matches('/').to_owned());
        }
        self
    }

    /// Allowed methods.
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

    /// Allowed headers (default: the ones requested by the browser).
    pub fn allow_headers(mut self, headers: &str) -> Self {
        self.headers = Some(headers.to_owned());
        self
    }

    /// Response headers the browser is allowed to read.
    pub fn expose_headers(mut self, headers: &str) -> Self {
        self.expose = Some(headers.to_owned());
        self
    }

    /// Allows cookies and authentication.
    ///
    /// Browsers only send credentials to explicitly allowed origins: combine
    /// it with [`Cors::allow_origin`]. With every origin allowed, the
    /// response says `*` and browsers refuse credentialed requests.
    pub fn allow_credentials(mut self, allow: bool) -> Self {
        self.credentials = allow;
        self
    }

    /// How long the results of preflight requests (`OPTIONS`) can be cached.
    pub fn max_age(mut self, max_age: Duration) -> Self {
        self.max_age = Some(max_age.as_secs());
        self
    }

    /// The `Access-Control-Allow-Origin` value for this origin, if it is allowed.
    fn allowed_origin(&self, origin: &str) -> Option<HeaderValue> {
        if self.origins.is_empty() {
            // Comme Express : `*`, même avec `allow_credentials(true)`. Les
            // navigateurs refusent alors les requêtes avec cookies, ce qui
            // oblige à lister les origines de confiance. Renvoyer l'origine
            // reçue laisserait n'importe quel site lire les réponses d'un
            // utilisateur connecté.
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

/// Compresses responses with Brotli, or with gzip when the browser does not accept
/// Brotli (`Accept-Encoding`), like `compression()` in Express. Needs the `compress`
/// feature.
///
/// Only bodies worth it are compressed: HTML, CSS, JavaScript, JSON, XML, SVG,
/// WebAssembly and the other text-like types, of 1 KiB or more, with a known size and
/// not encoded already; images, videos and archives pass through. The response gets
/// `Content-Encoding` and `Vary: Accept-Encoding`, and the compression runs on a
/// blocking thread, so the workers keep answering other requests. Add it before the
/// other middlewares, so that every response goes through it.
///
/// ```
/// use vitesse::prelude::*;
///
/// let mut app = App::new();
/// app.middleware(middleware::compress());
/// ```
#[cfg(feature = "compress")]
pub fn compress() -> impl Middleware {
    |req: Request, next: Next| async move {
        let encoding = compression::preferred(req.header(header::ACCEPT_ENCODING).unwrap_or(""));
        let res = next.run(req).await;
        match encoding {
            Some(encoding) => compression::apply(res, encoding).await,
            None => res,
        }
    }
}

#[cfg(feature = "compress")]
mod compression {
    use std::io::Write;

    use http::header;

    use crate::body::Body;
    use crate::response::Response;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum Encoding {
        Brotli,
        Gzip,
    }

    impl Encoding {
        fn name(self) -> &'static str {
            match self {
                Encoding::Brotli => "br",
                Encoding::Gzip => "gzip",
            }
        }
    }

    /// Below this size the headers would cost more than the bytes saved.
    const MIN: u64 = 1024;
    /// Above this size the body is streamed as it is rather than held twice in memory.
    const MAX: u64 = 16 * 1024 * 1024;

    /// The encoding to use for an `Accept-Encoding` header: Brotli when accepted, else
    /// gzip, else none. A quality of `0` refuses an encoding; `*` accepts gzip.
    pub(super) fn preferred(accept: &str) -> Option<Encoding> {
        let (mut brotli, mut gzip) = (false, false);
        for part in accept.split(',') {
            let mut pieces = part.split(';');
            let name = pieces.next().unwrap_or("").trim();
            let q = pieces
                .find_map(|p| p.trim().strip_prefix("q="))
                .and_then(|q| q.trim().parse::<f32>().ok())
                .unwrap_or(1.0);
            if q <= 0.0 {
                continue;
            }
            match name {
                "br" => brotli = true,
                "gzip" | "*" => gzip = true,
                _ => {}
            }
        }
        if brotli {
            Some(Encoding::Brotli)
        } else if gzip {
            Some(Encoding::Gzip)
        } else {
            None
        }
    }

    /// Whether a content type is worth compressing.
    pub(super) fn compressible(content_type: &str) -> bool {
        let ty = content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        ty.starts_with("text/")
            || ty.ends_with("+json")
            || ty.ends_with("+xml")
            || matches!(
                ty.as_str(),
                "application/javascript"
                    | "application/x-javascript"
                    | "application/json"
                    | "application/xml"
                    | "application/wasm"
                    | "image/svg+xml"
                    | "font/ttf"
                    | "font/otf"
            )
    }

    fn encode(bytes: &[u8], encoding: Encoding) -> std::io::Result<Vec<u8>> {
        let out = Vec::with_capacity(bytes.len() / 2);
        match encoding {
            Encoding::Brotli => {
                let mut writer = brotli::CompressorWriter::new(out, 4096, 5, 22);
                writer.write_all(bytes)?;
                Ok(writer.into_inner())
            }
            Encoding::Gzip => {
                let mut encoder =
                    flate2::write::GzEncoder::new(out, flate2::Compression::default());
                encoder.write_all(bytes)?;
                encoder.finish()
            }
        }
    }

    /// Compresses the body of `res` when it is worth it (see [`super::compress`]).
    pub(super) async fn apply(mut res: Response, encoding: Encoding) -> Response {
        let worth = res.error().is_none()
            && !matches!(res.status_code().as_u16(), 204 | 304)
            && res.get_header("content-encoding").is_none()
            && res.get_header("content-type").is_some_and(compressible)
            && res.body().size().is_some_and(|n| (MIN..=MAX).contains(&n));
        if !worth {
            return res;
        }
        let body = std::mem::take(res.body_mut());
        let Ok(bytes) = body.to_bytes().await else {
            res.set_status(500);
            return res;
        };
        let original = bytes.clone();
        match tokio::task::spawn_blocking(move || encode(&bytes, encoding)).await {
            Ok(Ok(encoded)) => {
                let headers = res.headers_mut();
                headers.remove(header::CONTENT_LENGTH);
                let vary = match headers.get(header::VARY).and_then(|v| v.to_str().ok()) {
                    Some(v) if v.to_ascii_lowercase().contains("accept-encoding") => v.to_owned(),
                    Some(v) => format!("{v}, accept-encoding"),
                    None => "accept-encoding".to_owned(),
                };
                res.set_header(header::VARY, vary);
                res.set_header(header::CONTENT_ENCODING, encoding.name());
                *res.body_mut() = Body::from(encoded);
            }
            _ => *res.body_mut() = Body::from(original),
        }
        res
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_browser_chooses_the_encoding() {
            assert_eq!(preferred("gzip, deflate, br"), Some(Encoding::Brotli));
            assert_eq!(preferred("gzip, deflate"), Some(Encoding::Gzip));
            assert_eq!(preferred("br;q=0, gzip;q=0.5"), Some(Encoding::Gzip));
            assert_eq!(preferred("*"), Some(Encoding::Gzip));
            assert_eq!(preferred("identity"), None);
            assert_eq!(preferred(""), None);
            assert_eq!(preferred("gzip;q=0, br;q=0"), None);
        }

        #[test]
        fn text_like_types_are_worth_it() {
            for ty in [
                "text/html; charset=utf-8",
                "application/javascript",
                "application/json",
                "application/manifest+json",
                "image/svg+xml",
                "application/wasm",
                "application/xml; charset=utf-8",
            ] {
                assert!(compressible(ty), "{ty}");
            }
            for ty in [
                "image/png",
                "video/mp4",
                "application/zip",
                "font/woff2",
                "",
            ] {
                assert!(!compressible(ty), "{ty}");
            }
        }

        #[test]
        fn both_encoders_round_trip() {
            use std::io::Read;

            let text = "<p>hello</p>".repeat(300);
            let br = encode(text.as_bytes(), Encoding::Brotli).unwrap();
            let mut decoded = Vec::new();
            brotli::Decompressor::new(br.as_slice(), 4096)
                .read_to_end(&mut decoded)
                .unwrap();
            assert_eq!(decoded, text.as_bytes());
            assert!(br.len() < text.len() / 10);
            let gz = encode(text.as_bytes(), Encoding::Gzip).unwrap();
            let mut decoded = String::new();
            flate2::read::GzDecoder::new(gz.as_slice())
                .read_to_string(&mut decoded)
                .unwrap();
            assert_eq!(decoded, text);
        }
    }
}
