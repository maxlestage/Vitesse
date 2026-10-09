//! Le type d'erreur HTTP de Vitesse.

use std::borrow::Cow;
use std::fmt;

use http::StatusCode;

use crate::body::BoxError;
use crate::response::{IntoResponse, IntoStatus, Response};

/// Alias pratique : `vitesse::Result<T>` = `Result<T, vitesse::Error>`.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Une erreur qui se transforme en réponse HTTP.
///
/// N'importe quelle erreur standard se convertit en `Error` avec `?` : elle
/// devient alors une `500 Internal Server Error` dont le détail n'est **pas**
/// envoyé au client (il est seulement journalisé). Pour renvoyer un message au
/// client, utilisez les constructeurs comme [`Error::bad_request`] ou
/// [`Error::not_found`].
///
/// ```
/// use vitesse::Error;
///
/// fn check(age: i32) -> vitesse::Result<i32> {
///     if age < 0 {
///         return Err(Error::bad_request("l'âge doit être positif"));
///     }
///     Ok(age)
/// }
/// # assert!(check(-1).is_err());
/// ```
pub struct Error {
    status: StatusCode,
    message: Cow<'static, str>,
    source: Option<BoxError>,
}

impl Error {
    /// Crée une erreur avec un statut et un message envoyé au client.
    pub fn new(status: impl IntoStatus, message: impl Into<Cow<'static, str>>) -> Self {
        Error {
            status: status.into_status(),
            message: message.into(),
            source: None,
        }
    }

    /// Crée une erreur dont le message est la raison standard du statut
    /// (ex. `Not Found` pour 404).
    pub fn from_status(status: impl IntoStatus) -> Self {
        let status = status.into_status();
        Error::new(status, status.canonical_reason().unwrap_or("Error"))
    }

    /// Attache la cause d'origine (journalisée pour les erreurs 5xx).
    pub fn with_source(mut self, source: impl Into<BoxError>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// `400 Bad Request`
    pub fn bad_request(message: impl Into<Cow<'static, str>>) -> Self {
        Error::new(StatusCode::BAD_REQUEST, message)
    }

    /// `401 Unauthorized`
    pub fn unauthorized(message: impl Into<Cow<'static, str>>) -> Self {
        Error::new(StatusCode::UNAUTHORIZED, message)
    }

    /// `403 Forbidden`
    pub fn forbidden(message: impl Into<Cow<'static, str>>) -> Self {
        Error::new(StatusCode::FORBIDDEN, message)
    }

    /// `404 Not Found`
    pub fn not_found(message: impl Into<Cow<'static, str>>) -> Self {
        Error::new(StatusCode::NOT_FOUND, message)
    }

    /// `409 Conflict`
    pub fn conflict(message: impl Into<Cow<'static, str>>) -> Self {
        Error::new(StatusCode::CONFLICT, message)
    }

    /// `413 Payload Too Large`
    pub fn payload_too_large() -> Self {
        Error::from_status(StatusCode::PAYLOAD_TOO_LARGE)
    }

    /// `422 Unprocessable Entity`
    pub fn unprocessable(message: impl Into<Cow<'static, str>>) -> Self {
        Error::new(StatusCode::UNPROCESSABLE_ENTITY, message)
    }

    /// `500 Internal Server Error`
    pub fn internal(message: impl Into<Cow<'static, str>>) -> Self {
        Error::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    /// Le statut HTTP.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Le message envoyé au client.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// La cause d'origine, s'il y en a une.
    pub fn source(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        self.source.as_deref()
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("status", &self.status)
            .field("message", &self.message)
            .field("source", &self.source)
            .finish()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.status.as_u16(), self.message)?;
        if let Some(source) = &self.source {
            write!(f, " ({source})")?;
        }
        Ok(())
    }
}

/// Toute erreur standard devient une `500` (détail non exposé au client).
impl<E> From<E> for Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(err: E) -> Self {
        Error::from_status(StatusCode::INTERNAL_SERVER_ERROR).with_source(err)
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            error: &'a str,
        }
        let body = serde_json::to_vec(&Payload {
            error: &self.message,
        })
        .unwrap_or_default();
        let mut res = Response::new().status(self.status).json_bytes(body);
        res.error = Some(Box::new(self));
        res
    }
}
