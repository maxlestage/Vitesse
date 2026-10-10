//! HTTP response bodies.

use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use http_body::{Frame, SizeHint};
use http_body_util::BodyExt;
use http_body_util::combinators::UnsyncBoxBody;

/// A type-erased error that can be sent between threads.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// The body of a response.
///
/// The common cases (empty, text, JSON, bytes) are stored as is, with no
/// extra allocation; streaming goes through [`Body::from_stream`] or
/// [`Body::wrap`].
pub struct Body {
    kind: Kind,
}

pub(crate) enum Kind {
    Empty,
    Full(Bytes),
    Stream(UnsyncBoxBody<Bytes, BoxError>),
}

impl Body {
    /// An empty body.
    #[inline]
    pub const fn empty() -> Self {
        Body { kind: Kind::Empty }
    }

    /// Wraps any [`http_body::Body`] (e.g. the body of a request, to build a
    /// proxy).
    pub fn wrap<B>(body: B) -> Self
    where
        B: http_body::Body<Data = Bytes> + Send + 'static,
        B::Error: Into<BoxError>,
    {
        Body {
            kind: Kind::Stream(body.map_err(Into::into).boxed_unsync()),
        }
    }

    /// Creates a body sent as a stream (*chunked*) from a [`Stream`].
    pub fn from_stream<S, D, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<D, E>> + Send + 'static,
        D: Into<Bytes>,
        E: Into<BoxError>,
    {
        Body::wrap(StreamBody {
            stream: Box::pin(stream),
        })
    }

    /// The raw content, for the HTTP engine.
    #[inline]
    pub(crate) fn into_kind(self) -> Kind {
        self.kind
    }

    /// The number of bytes, if the size is known in advance.
    pub fn size(&self) -> Option<u64> {
        http_body::Body::size_hint(self).exact()
    }

    /// Reads the whole body into memory.
    pub async fn to_bytes(self) -> Result<Bytes, BoxError> {
        match self.kind {
            Kind::Empty => Ok(Bytes::new()),
            Kind::Full(bytes) => Ok(bytes),
            Kind::Stream(body) => Ok(body.collect().await?.to_bytes()),
        }
    }
}

impl Default for Body {
    fn default() -> Self {
        Body::empty()
    }
}

impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Kind::Empty => f.write_str("Body::Empty"),
            Kind::Full(b) => f.debug_tuple("Body::Full").field(&b.len()).finish(),
            Kind::Stream(_) => f.write_str("Body::Stream"),
        }
    }
}

impl http_body::Body for Body {
    type Data = Bytes;
    type Error = BoxError;

    #[inline]
    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, BoxError>>> {
        let this = self.get_mut();
        match &mut this.kind {
            Kind::Empty => Poll::Ready(None),
            Kind::Full(bytes) => {
                let bytes = std::mem::take(bytes);
                this.kind = Kind::Empty;
                if bytes.is_empty() {
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Ok(Frame::data(bytes))))
                }
            }
            Kind::Stream(body) => Pin::new(body).poll_frame(cx),
        }
    }

    #[inline]
    fn is_end_stream(&self) -> bool {
        match &self.kind {
            Kind::Empty => true,
            Kind::Full(bytes) => bytes.is_empty(),
            Kind::Stream(body) => body.is_end_stream(),
        }
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        match &self.kind {
            Kind::Empty => SizeHint::with_exact(0),
            Kind::Full(bytes) => SizeHint::with_exact(bytes.len() as u64),
            Kind::Stream(body) => body.size_hint(),
        }
    }
}

impl From<Bytes> for Body {
    #[inline]
    fn from(bytes: Bytes) -> Self {
        Body {
            kind: Kind::Full(bytes),
        }
    }
}

impl From<&'static str> for Body {
    #[inline]
    fn from(s: &'static str) -> Self {
        Bytes::from_static(s.as_bytes()).into()
    }
}

impl From<&'static [u8]> for Body {
    #[inline]
    fn from(s: &'static [u8]) -> Self {
        Bytes::from_static(s).into()
    }
}

impl From<String> for Body {
    #[inline]
    fn from(s: String) -> Self {
        Bytes::from(s).into()
    }
}

impl From<Vec<u8>> for Body {
    #[inline]
    fn from(v: Vec<u8>) -> Self {
        Bytes::from(v).into()
    }
}

impl From<std::borrow::Cow<'static, str>> for Body {
    #[inline]
    fn from(s: std::borrow::Cow<'static, str>) -> Self {
        match s {
            std::borrow::Cow::Borrowed(s) => s.into(),
            std::borrow::Cow::Owned(s) => s.into(),
        }
    }
}

impl From<()> for Body {
    #[inline]
    fn from(_: ()) -> Self {
        Body::empty()
    }
}

/// Adapter from `Stream` to `http_body::Body`.
struct StreamBody<S> {
    stream: Pin<Box<S>>,
}

impl<S, D, E> http_body::Body for StreamBody<S>
where
    S: Stream<Item = Result<D, E>>,
    D: Into<Bytes>,
    E: Into<BoxError>,
{
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, BoxError>>> {
        match self.get_mut().stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(data))) => Poll::Ready(Some(Ok(Frame::data(data.into())))),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e.into()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
