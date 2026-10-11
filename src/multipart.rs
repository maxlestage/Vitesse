//! `multipart/form-data` forms: file uploads and HTML forms with
//! `enctype="multipart/form-data"`, read as a stream with
//! [multer](https://crates.io/crates/multer) (like axum's `Multipart`).
//! Needs the `multipart` feature.
//!
//! ```
//! use vitesse::prelude::*;
//!
//! async fn upload(req: Request) -> vitesse::Result<String> {
//!     let mut form = req.multipart()?;
//!     let mut saved = Vec::new();
//!     while let Some(field) = form.next_field().await? {
//!         match field.file_name() {
//!             // A file: streamed to disk, without holding it in memory.
//!             Some(name) => {
//!                 let name = name.replace(['/', '\\'], "_");
//!                 let size = field.save(format!("/tmp/{name}")).await?;
//!                 saved.push(format!("{name} ({size} bytes)"));
//!             }
//!             // A text input.
//!             None => {
//!                 let name = field.name().unwrap_or("").to_owned();
//!                 let value = field.text().await?;
//!                 saved.push(format!("{name}={value}"));
//!             }
//!         }
//!     }
//!     Ok(saved.join(", "))
//! }
//!
//! let mut app = App::new();
//! app.post("/upload", upload.with(middleware::body_limit(100 * 1024 * 1024)));
//! ```
//!
//! The whole body is limited by [`Request::body_limit`](crate::Request::body_limit)
//! (`413` beyond): raise it for the routes that receive files with
//! [`middleware::body_limit`](crate::middleware::body_limit).

use std::fmt;
use std::path::Path;

use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use http_body_util::BodyDataStream;
use tokio::io::AsyncWriteExt;

use crate::error::Error;
use crate::request::Request;

/// The fields of a `multipart/form-data` body, in order (see
/// [`Request::multipart`]).
pub struct Multipart {
    inner: multer::Multipart<'static>,
}

impl fmt::Debug for Multipart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Multipart").finish_non_exhaustive()
    }
}

impl Multipart {
    pub(crate) fn from_request(req: &Request) -> Result<Multipart, Error> {
        let content_type = req.content_type().unwrap_or("");
        let boundary = multer::parse_boundary(content_type).map_err(|_| {
            Error::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "expected a multipart/form-data body",
            )
        })?;
        let constraints = multer::Constraints::new()
            .size_limit(multer::SizeLimit::new().whole_stream(req.body_limit() as u64));
        let stream = BodyDataStream::new(req.take_body());
        Ok(Multipart {
            inner: multer::Multipart::with_constraints(stream, boundary, constraints),
        })
    }

    /// The next field, or `None` at the end of the form. A field must be
    /// read (or dropped) before asking for the next one.
    pub async fn next_field(&mut self) -> Result<Option<Field>, Error> {
        match self.inner.next_field().await {
            Ok(field) => Ok(field.map(|inner| Field { inner })),
            Err(e) => Err(error(e)),
        }
    }
}

/// A field of a multipart form: a text input or a file.
pub struct Field {
    inner: multer::Field<'static>,
}

impl fmt::Debug for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Field")
            .field("name", &self.name())
            .field("file_name", &self.file_name())
            .field("content_type", &self.content_type())
            .finish_non_exhaustive()
    }
}

impl Field {
    /// The name of the input (`<input name="avatar">`).
    pub fn name(&self) -> Option<&str> {
        self.inner.name()
    }

    /// The name of the file on the client's machine, for a file. Never use
    /// it as a path as is: it comes from the client.
    pub fn file_name(&self) -> Option<&str> {
        self.inner.file_name()
    }

    /// The `Content-Type` of the field (`image/png`…), if the client sent one.
    pub fn content_type(&self) -> Option<&str> {
        self.inner.content_type().map(|m| m.as_ref())
    }

    /// All the headers of the field.
    pub fn headers(&self) -> &HeaderMap {
        self.inner.headers()
    }

    /// The next chunk of the content, as it arrives; `None` at the end.
    pub async fn chunk(&mut self) -> Result<Option<Bytes>, Error> {
        self.inner.chunk().await.map_err(error)
    }

    /// The whole content, in memory.
    pub async fn bytes(self) -> Result<Bytes, Error> {
        self.inner.bytes().await.map_err(error)
    }

    /// The whole content as UTF-8 text; `400` if it is invalid.
    pub async fn text(self) -> Result<String, Error> {
        let bytes = self.bytes().await?;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| Error::bad_request("a form field is not valid UTF-8"))
    }

    /// Writes the content to a file, chunk by chunk, and returns its size.
    /// The file is created, or truncated if it exists; if the upload fails
    /// halfway (`413`, client gone), the partial file is removed.
    pub async fn save(mut self, path: impl AsRef<Path>) -> Result<u64, Error> {
        let path = path.as_ref();
        let mut file = tokio::fs::File::create(path).await?;
        let mut size = 0;
        let written: Result<(), Error> = async {
            while let Some(chunk) = self.chunk().await? {
                file.write_all(&chunk).await?;
                size += chunk.len() as u64;
            }
            file.flush().await?;
            Ok(())
        }
        .await;
        if let Err(e) = written {
            drop(file);
            let _ = tokio::fs::remove_file(path).await;
            return Err(e);
        }
        Ok(size)
    }
}

fn error(e: multer::Error) -> Error {
    match e {
        multer::Error::StreamSizeExceeded { .. } | multer::Error::FieldSizeExceeded { .. } => {
            Error::payload_too_large()
        }
        multer::Error::StreamReadFailed(source) => {
            Error::bad_request("failed to read the request body").with_source(source)
        }
        other => Error::bad_request(format!("invalid multipart body: {other}")),
    }
}
