//! Fichiers statiques (`express.static`, `res.sendFile`).

use std::fs::Metadata;
use std::io::{self, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, UNIX_EPOCH};

use bytes::Bytes;
use http::header::{self, HeaderValue};
use http::{Method, StatusCode};
use http_body::{Frame, SizeHint};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, ReadBuf};

use crate::body::Body;
use crate::error::Error;
use crate::handler::{BoxFuture, Handler, Middleware, Next};
use crate::request::Request;
use crate::response::{IntoResponse, Response};
use crate::util::{http_date, mime_for, percent_decode};

/// En dessous de cette taille, un fichier est lu d'un coup ; au-delà, il est
/// envoyé en flux.
const SMALL_FILE: u64 = 256 * 1024;
const CHUNK: usize = 64 * 1024;

/// Sert les fichiers d'un dossier.
///
/// - **Comme route**, avec [`App::static_dir`](crate::App::static_dir) ou
///   [`App::serve_dir`](crate::App::serve_dir) : `GET /static/*`.
/// - **Comme middleware** (`app.use(express.static('public'))`) : si le
///   fichier n'existe pas, la requête continue vers les routes.
///
/// Gère `index.html`, `ETag` / `Last-Modified` (réponses `304`), les requêtes
/// partielles (`Range`) et refuse les chemins qui sortent du dossier ainsi que
/// les fichiers cachés (`.env`, `.git`…).
///
/// ```
/// use vitesse::prelude::*;
/// use std::time::Duration;
///
/// let mut app = App::new();
/// app.static_dir("/assets", "./public");
/// app.serve_dir("/docs", ServeDir::new("./site").max_age(Duration::from_secs(3600)));
/// app.middleware(ServeDir::new("./www")); // à la racine, avant les routes
/// ```
#[derive(Debug, Clone)]
pub struct ServeDir {
    root: PathBuf,
    index: Option<String>,
    max_age: u64,
    dotfiles: bool,
}

impl ServeDir {
    /// Sert le dossier `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        ServeDir {
            root: root.into(),
            index: Some("index.html".into()),
            max_age: 0,
            dotfiles: false,
        }
    }

    /// Fichier servi pour un dossier (`index.html` par défaut, `None` pour désactiver).
    pub fn index(mut self, index: Option<&str>) -> Self {
        self.index = index.map(str::to_owned);
        self
    }

    /// Durée de cache navigateur (`Cache-Control: max-age`).
    pub fn max_age(mut self, max_age: Duration) -> Self {
        self.max_age = max_age.as_secs();
        self
    }

    /// Autorise les fichiers et dossiers cachés (commençant par `.`).
    pub fn dotfiles(mut self, allow: bool) -> Self {
        self.dotfiles = allow;
        self
    }

    /// Cherche et sert `relative` ; `None` si le fichier n'existe pas.
    async fn serve(&self, req: &Request, relative: &str) -> Option<Response> {
        let mut path = self.root.clone();
        for segment in relative.split('/') {
            if segment.is_empty() || segment == "." {
                continue;
            }
            if segment == ".."
                || segment.contains('\\')
                || segment.contains('\0')
                || (!self.dotfiles && segment.starts_with('.'))
                || !Path::new(segment)
                    .components()
                    .all(|c| matches!(c, Component::Normal(_)))
            {
                return None;
            }
            path.push(segment);
        }

        let mut meta = tokio::fs::metadata(&path).await.ok()?;
        if meta.is_dir() {
            let index = self.index.as_ref()?;
            // Comme Express : `/docs` -> `/docs/` pour que les liens relatifs marchent.
            if !req.path().ends_with('/') {
                let mut location = format!("{}/", req.path());
                if let Some(q) = req.query_string() {
                    location.push('?');
                    location.push_str(q);
                }
                return Some(crate::Redirect::permanent(location).into_response());
            }
            path.push(index);
            meta = tokio::fs::metadata(&path).await.ok()?;
        }
        if !meta.is_file() {
            return None;
        }
        Some(file_response(&path, &meta, Some(req), self.max_age).await)
    }
}

impl Handler for ServeDir {
    fn call(&'static self, req: Request) -> BoxFuture<Response> {
        Box::pin(async move {
            let relative = req.param("*").unwrap_or("");
            match self.serve(&req, relative).await {
                Some(res) => res,
                None => Error::not_found(format!("Cannot {} {}", req.method(), req.path()))
                    .into_response(),
            }
        })
    }
}

impl Middleware for ServeDir {
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response> {
        if *req.method() != Method::GET && *req.method() != Method::HEAD {
            return next.run(req);
        }
        Box::pin(async move {
            let relative = percent_decode(req.path()).into_owned();
            if let Some(res) = self.serve(&req, &relative).await {
                return res;
            }
            next.run(req).await
        })
    }
}

/// `res.sendFile` : envoie un fichier précis.
pub(crate) async fn send_file(path: &Path, req: Option<&Request>) -> Response {
    match tokio::fs::metadata(path).await {
        Ok(meta) if meta.is_file() => file_response(path, &meta, req, 0).await,
        _ => Error::from_status(StatusCode::NOT_FOUND).into_response(),
    }
}

/// Construit la réponse pour un fichier existant.
async fn file_response(
    path: &Path,
    meta: &Metadata,
    req: Option<&Request>,
    max_age: u64,
) -> Response {
    let len = meta.len();
    let modified = meta.modified().ok();
    let mtime = modified
        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let etag = format!("W/\"{len:x}-{mtime:x}\"");
    let last_modified = modified.map(http_date);

    let mut res = Response::new()
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static(mime_for(path)),
        )
        .header(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"))
        .header(header::CACHE_CONTROL, format!("public, max-age={max_age}"))
        .header(header::ETAG, etag.as_str());
    if let Some(lm) = &last_modified {
        res = res.header(header::LAST_MODIFIED, lm.as_str());
    }

    let mut range = None;
    let mut head = false;
    if let Some(req) = req {
        head = *req.method() == Method::HEAD;
        // Requêtes conditionnelles : le navigateur a déjà la bonne version.
        let fresh = match req.header(header::IF_NONE_MATCH) {
            Some(inm) => inm.split(',').any(|t| {
                let t = t.trim();
                t == "*" || t.trim_start_matches("W/") == etag.trim_start_matches("W/")
            }),
            None => matches!(
                (req.header(header::IF_MODIFIED_SINCE), &last_modified),
                (Some(ims), Some(lm)) if ims == lm
            ),
        };
        if fresh {
            return res.status(StatusCode::NOT_MODIFIED);
        }
        if let Some(value) = req.header(header::RANGE) {
            match parse_range(value, len) {
                Some(Ok(r)) => range = Some(r),
                Some(Err(())) => {
                    return Error::from_status(StatusCode::RANGE_NOT_SATISFIABLE)
                        .into_response()
                        .header(header::CONTENT_RANGE, format!("bytes */{len}"));
                }
                None => {}
            }
        }
    }

    let (start, count) = match range {
        Some((start, end)) => {
            res = res
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"));
            (start, end - start + 1)
        }
        None => (0, len),
    };

    if head {
        return res.header(header::CONTENT_LENGTH, count);
    }

    match read_file(path, start, count, len).await {
        Ok(body) => res.send(body),
        Err(e) => Error::internal("lecture du fichier impossible")
            .with_source(e)
            .into_response(),
    }
}

/// Lit `count` octets à partir de `start` : d'un coup si c'est petit, en flux sinon.
async fn read_file(path: &Path, start: u64, count: u64, len: u64) -> io::Result<Body> {
    if start == 0 && count == len && count <= SMALL_FILE {
        let data = tokio::fs::read(path).await?;
        // Le fichier a pu changer entre-temps : on s'en tient à ce qu'on a lu.
        return Ok(Body::from(data));
    }
    let mut file = tokio::fs::File::open(path).await?;
    if start > 0 {
        file.seek(SeekFrom::Start(start)).await?;
    }
    if count <= SMALL_FILE {
        let mut data = vec![0; count as usize];
        file.read_exact(&mut data).await?;
        return Ok(Body::from(data));
    }
    Ok(Body::wrap(FileBody {
        file,
        remaining: count,
        buf: vec![0; CHUNK],
    }))
}

/// Analyse un en-tête `Range: bytes=a-b` (une seule plage).
///
/// `None` : en-tête ignoré ; `Some(Err)` : plage impossible (416).
fn parse_range(value: &str, len: u64) -> Option<Result<(u64, u64), ()>> {
    let spec = value.trim().strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None; // Plages multiples : on renvoie le fichier entier.
    }
    let (a, b) = spec.split_once('-')?;
    let (a, b) = (a.trim(), b.trim());
    let range = if a.is_empty() {
        // Les `n` derniers octets.
        let n: u64 = b.parse().ok()?;
        if n == 0 || len == 0 {
            return Some(Err(()));
        }
        (len.saturating_sub(n), len - 1)
    } else {
        let start: u64 = a.parse().ok()?;
        let end: u64 = if b.is_empty() {
            len.saturating_sub(1)
        } else {
            b.parse().ok()?
        };
        if start >= len || end < start {
            return Some(Err(()));
        }
        (start, end.min(len - 1))
    };
    Some(Ok(range))
}

/// Un fichier envoyé par morceaux de 64 Kio.
struct FileBody {
    file: tokio::fs::File,
    remaining: u64,
    buf: Vec<u8>,
}

impl http_body::Body for FileBody {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, io::Error>>> {
        let this = self.get_mut();
        if this.remaining == 0 {
            return Poll::Ready(None);
        }
        let want = this.remaining.min(CHUNK as u64) as usize;
        let mut buf = ReadBuf::new(&mut this.buf[..want]);
        match Pin::new(&mut this.file).poll_read(cx, &mut buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
            Poll::Ready(Ok(())) => {
                let n = buf.filled().len();
                if n == 0 {
                    return Poll::Ready(Some(Err(io::ErrorKind::UnexpectedEof.into())));
                }
                this.remaining -= n as u64;
                Poll::Ready(Some(Ok(Frame::data(Bytes::copy_from_slice(
                    &this.buf[..n],
                )))))
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.remaining == 0
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.remaining)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_range;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-9", 100), Some(Ok((0, 9))));
        assert_eq!(parse_range("bytes=90-", 100), Some(Ok((90, 99))));
        assert_eq!(parse_range("bytes=-10", 100), Some(Ok((90, 99))));
        assert_eq!(parse_range("bytes=50-500", 100), Some(Ok((50, 99))));
        assert_eq!(parse_range("bytes=100-", 100), Some(Err(())));
        assert_eq!(parse_range("bytes=5-1", 100), Some(Err(())));
        assert_eq!(parse_range("bytes=0-1,5-6", 100), None);
        assert_eq!(parse_range("items=0-1", 100), None);
        assert_eq!(parse_range("bytes=x-1", 100), None);
    }
}
