//! Request ids (tower-http's `SetRequestId` and `PropagateRequestId`), to
//! follow a request through logs and services.

use std::cell::Cell;
use std::fmt;
use std::hash::{BuildHasher, Hasher};

use http::header::{HeaderName, HeaderValue};

use crate::handler::{BoxFuture, Middleware, Next};
use crate::request::Request;
use crate::response::Response;

/// Gives every request an id: the one received in `X-Request-Id` (from a
/// proxy or another service), or a new random one (a UUID v4). The id is
/// put back in the request's header, in its extensions (`req.get::<RequestId>()`)
/// and in the response's header.
///
/// ```
/// use vitesse::prelude::*;
/// use vitesse::middleware::RequestId;
///
/// let mut app = App::new();
/// app.middleware(middleware::request_id());
/// app.get("/", |req: Request| async move {
///     let id = req.get::<RequestId>().unwrap();
///     format!("request {id}")
/// });
/// ```
pub fn request_id() -> SetRequestId {
    SetRequestId {
        header: HeaderName::from_static("x-request-id"),
        trust_incoming: true,
    }
}

/// The middleware returned by [`request_id`].
#[derive(Debug, Clone)]
pub struct SetRequestId {
    header: HeaderName,
    trust_incoming: bool,
}

impl SetRequestId {
    /// The header that carries the id (`X-Request-Id` by default).
    ///
    /// # Panics
    /// If the name is not a valid header name.
    pub fn header(mut self, name: &str) -> Self {
        self.header = HeaderName::try_from(name).expect("invalid header name");
        self
    }

    /// Keeps the id received from the client (`true` by default). Turn it
    /// off when clients can reach the application directly and their ids
    /// should not end up in your logs.
    pub fn trust_incoming(mut self, trust: bool) -> Self {
        self.trust_incoming = trust;
        self
    }
}

/// The id of a request, set by [`request_id`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RequestId(pub String);

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Middleware for SetRequestId {
    fn handle(&'static self, mut req: Request, next: Next) -> BoxFuture<Response> {
        let incoming = req
            .header(&self.header)
            .filter(|_| self.trust_incoming)
            .filter(|id| {
                !id.is_empty() && id.len() <= 200 && id.bytes().all(|b| b.is_ascii_graphic())
            })
            .map(str::to_owned);
        let id = match incoming {
            Some(id) => id,
            None => {
                let id = uuid_v4();
                if let Ok(value) = HeaderValue::from_str(&id) {
                    req.headers_mut().insert(self.header.clone(), value);
                }
                id
            }
        };
        let value = HeaderValue::from_str(&id).ok();
        req.set(RequestId(id));
        Box::pin(async move {
            let mut res = next.run(req).await;
            if let Some(value) = value {
                if !res.headers().contains_key(&self.header) {
                    res.headers_mut().insert(self.header.clone(), value);
                }
            }
            res
        })
    }
}

/// A random UUID v4, from a per-thread generator seeded by the operating
/// system (through `RandomState`).
fn uuid_v4() -> String {
    thread_local! {
        static STATE: Cell<u64> = Cell::new({
            let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
            hasher.write_u64(0x5eed);
            hasher.finish()
        });
    }
    // splitmix64
    let next = || {
        STATE.with(|state| {
            let s = state.get().wrapping_add(0x9e37_79b9_7f4a_7c15);
            state.set(s);
            let mut z = s;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        })
    };
    let (hi, lo) = (next(), next());
    let hi = (hi & 0xffff_ffff_ffff_0fff) | 0x0000_0000_0000_4000;
    let lo = (lo & 0x3fff_ffff_ffff_ffff) | 0x8000_0000_0000_0000;
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        hi >> 32,
        (hi >> 16) & 0xffff,
        hi & 0xffff,
        lo >> 48,
        lo & 0xffff_ffff_ffff
    )
}

#[cfg(test)]
mod tests {
    use super::uuid_v4;

    #[test]
    fn uuids() {
        let a = uuid_v4();
        let b = uuid_v4();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
        assert!(matches!(&a[19..20], "8" | "9" | "a" | "b"), "{a}");
        assert!(a.bytes().all(|c| c == b'-' || c.is_ascii_hexdigit()));
    }
}
