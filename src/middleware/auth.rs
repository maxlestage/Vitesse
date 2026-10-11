//! HTTP authentication: `Basic` (user name and password) and `Bearer`
//! (token), like tower-http's `ValidateRequestHeader` and
//! actix-web-httpauth.

use std::fmt;

use http::header::{self, HeaderValue};

use crate::error::Error;
use crate::handler::{BoxFuture, Middleware, Next};
use crate::request::Request;
use crate::response::{IntoResponse, Response};

/// Something that checks a user name and a password, for [`basic_auth`]:
/// a closure, a `(user, password)` pair, or a list of pairs (compared in
/// constant time).
pub trait BasicCredentials: Send + Sync + 'static {
    /// Whether this user name and password are valid.
    fn check(&self, user: &str, password: &str) -> bool;
}

impl<F> BasicCredentials for F
where
    F: Fn(&str, &str) -> bool + Send + Sync + 'static,
{
    fn check(&self, user: &str, password: &str) -> bool {
        self(user, password)
    }
}

impl BasicCredentials for (&'static str, &'static str) {
    fn check(&self, user: &str, password: &str) -> bool {
        // `&` et non `&&` : les deux comparaisons ont toujours lieu.
        constant_time_eq(self.0.as_bytes(), user.as_bytes())
            & constant_time_eq(self.1.as_bytes(), password.as_bytes())
    }
}

impl BasicCredentials for (String, String) {
    fn check(&self, user: &str, password: &str) -> bool {
        constant_time_eq(self.0.as_bytes(), user.as_bytes())
            & constant_time_eq(self.1.as_bytes(), password.as_bytes())
    }
}

impl<C: BasicCredentials> BasicCredentials for Vec<C> {
    fn check(&self, user: &str, password: &str) -> bool {
        self.iter()
            .fold(false, |found, c| found | c.check(user, password))
    }
}

impl<C: BasicCredentials, const N: usize> BasicCredentials for [C; N] {
    fn check(&self, user: &str, password: &str) -> bool {
        self.iter()
            .fold(false, |found, c| found | c.check(user, password))
    }
}

/// HTTP `Basic` authentication: answers `401` with `WWW-Authenticate` (the
/// browser then asks for a user name and a password) until the request
/// carries valid credentials. The user name is then available with
/// `req.get::<BasicUser>()`.
///
/// Basic authentication sends the password with every request: only use it
/// over HTTPS.
///
/// ```
/// use vitesse::prelude::*;
/// use vitesse::middleware::BasicUser;
///
/// let mut admin = Router::new();
/// admin.middleware(middleware::basic_auth(("admin", "correct horse")).realm("Admin"));
/// admin.get("/", |req: Request| async move {
///     format!("Hello {}", req.get::<BasicUser>().unwrap().0)
/// });
///
/// // Or a closure, to look the users up somewhere else:
/// let check = middleware::basic_auth(|user: &str, password: &str| {
///     user == "ada" && password == std::env::var("ADA_PASSWORD").unwrap_or_default()
/// });
/// # let _ = check;
/// ```
pub fn basic_auth<C: BasicCredentials>(credentials: C) -> BasicAuth {
    BasicAuth {
        credentials: Box::new(credentials),
        challenge: HeaderValue::from_static("Basic realm=\"Restricted\", charset=\"UTF-8\""),
    }
}

/// The middleware returned by [`basic_auth`].
pub struct BasicAuth {
    credentials: Box<dyn BasicCredentials>,
    challenge: HeaderValue,
}

impl fmt::Debug for BasicAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BasicAuth")
            .field("challenge", &self.challenge)
            .finish_non_exhaustive()
    }
}

/// The user name authenticated by [`basic_auth`], in the request's
/// extensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicUser(pub String);

impl BasicAuth {
    /// The realm shown by the browser (`Restricted` by default).
    pub fn realm(mut self, realm: &str) -> Self {
        let realm: String = realm
            .chars()
            .filter(|c| *c != '"' && *c != '\\' && !c.is_control())
            .collect();
        if let Ok(value) =
            HeaderValue::from_str(&format!("Basic realm=\"{realm}\", charset=\"UTF-8\""))
        {
            self.challenge = value;
        }
        self
    }

    fn credentials(&self, req: &Request) -> Option<(String, String)> {
        let value = req.header(header::AUTHORIZATION)?;
        let (scheme, encoded) = value.trim().split_once(' ')?;
        if !scheme.eq_ignore_ascii_case("basic") {
            return None;
        }
        let decoded = String::from_utf8(base64_decode(encoded.trim())?).ok()?;
        let (user, password) = decoded.split_once(':')?;
        Some((user.to_owned(), password.to_owned()))
    }
}

impl Middleware for BasicAuth {
    fn handle(&'static self, mut req: Request, next: Next) -> BoxFuture<Response> {
        match self.credentials(&req) {
            Some((user, password)) if self.credentials.check(&user, &password) => {
                req.set(BasicUser(user));
                next.run(req)
            }
            _ => {
                let res = Error::unauthorized("authentication required")
                    .into_response()
                    .header(header::WWW_AUTHENTICATE, self.challenge.clone());
                Box::pin(std::future::ready(res))
            }
        }
    }
}

/// Something that checks a bearer token, for [`bearer_auth`]: a closure, a
/// token, or a list of tokens (compared in constant time).
pub trait BearerToken: Send + Sync + 'static {
    /// Whether this token is valid.
    fn check(&self, token: &str) -> bool;
}

impl<F> BearerToken for F
where
    F: Fn(&str) -> bool + Send + Sync + 'static,
{
    fn check(&self, token: &str) -> bool {
        self(token)
    }
}

impl BearerToken for &'static str {
    fn check(&self, token: &str) -> bool {
        constant_time_eq(self.as_bytes(), token.as_bytes())
    }
}

impl BearerToken for String {
    fn check(&self, token: &str) -> bool {
        constant_time_eq(self.as_bytes(), token.as_bytes())
    }
}

impl<T: BearerToken> BearerToken for Vec<T> {
    fn check(&self, token: &str) -> bool {
        self.iter().fold(false, |found, t| found | t.check(token))
    }
}

impl<T: BearerToken, const N: usize> BearerToken for [T; N] {
    fn check(&self, token: &str) -> bool {
        self.iter().fold(false, |found, t| found | t.check(token))
    }
}

/// `Bearer` token authentication (`Authorization: Bearer <token>`), for
/// APIs: `401` with `WWW-Authenticate: Bearer` without a valid token.
///
/// ```
/// use vitesse::prelude::*;
///
/// let token = std::env::var("API_TOKEN").unwrap_or_else(|_| "dev-token".into());
/// let mut api = Router::new();
/// api.middleware(middleware::bearer_auth(token));
/// api.get("/stats", |_| async { Json(json!({ "users": 42 })) });
/// ```
pub fn bearer_auth<T: BearerToken>(token: T) -> BearerAuth {
    BearerAuth {
        token: Box::new(token),
    }
}

/// The middleware returned by [`bearer_auth`].
pub struct BearerAuth {
    token: Box<dyn BearerToken>,
}

impl fmt::Debug for BearerAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BearerAuth").finish_non_exhaustive()
    }
}

impl Middleware for BearerAuth {
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response> {
        let token = req
            .header(header::AUTHORIZATION)
            .map(str::trim)
            .and_then(|v| v.split_once(' '))
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
            .map(|(_, token)| token.trim());
        if token.is_some_and(|t| self.token.check(t)) {
            return next.run(req);
        }
        let challenge = if token.is_some() {
            "Bearer error=\"invalid_token\""
        } else {
            "Bearer"
        };
        let res = Error::unauthorized("authentication required")
            .into_response()
            .header(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static(challenge),
            );
        Box::pin(std::future::ready(res))
    }
}

/// Compares two secrets in a time that doesn't depend on where they differ
/// (only on their lengths), so that the response time doesn't leak them.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let diff = a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y));
    std::hint::black_box(diff) == 0
}

/// Decodes standard base64 (with or without padding).
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn value(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    }
    let input = input.trim_end_matches('=').as_bytes();
    if input.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    for chunk in input.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= value(c)? << (18 - 6 * i);
        }
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64() {
        assert_eq!(base64_decode("YWRtaW46c2VjcmV0").unwrap(), b"admin:secret");
        assert_eq!(base64_decode("YQ==").unwrap(), b"a");
        assert_eq!(base64_decode("YWI=").unwrap(), b"ab");
        assert_eq!(base64_decode("YWJj").unwrap(), b"abc");
        assert_eq!(base64_decode("").unwrap(), b"");
        assert!(base64_decode("Y").is_none());
        assert!(base64_decode("!!!!").is_none());
    }

    #[test]
    fn credentials() {
        assert!(("a", "b").check("a", "b"));
        assert!(!("a", "b").check("a", "c"));
        assert!([("a", "b"), ("c", "d")].check("c", "d"));
        assert!(!vec![("a", "b")].check("c", "d"));
        assert!((|u: &str, _: &str| u == "x").check("x", ""));
        assert!("t0k3n".check("t0k3n"));
        assert!(!"t0k3n".check("t0k3"));
        assert!(["a", "b"].check("b"));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
    }
}
