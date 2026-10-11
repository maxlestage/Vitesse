//! Protecting the server from overload: a cap on the requests in flight
//! (tower's `ConcurrencyLimit` and `LoadShed`) and a rate limit per client
//! (tower-governor, actix-governor, express-rate-limit).

use std::collections::HashMap;
use std::fmt;
use std::hash::{BuildHasher, Hash};
use std::net::IpAddr;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use http::StatusCode;
use http::header::{self, HeaderName, HeaderValue};
use tokio::sync::Semaphore;

use crate::error::Error;
use crate::handler::{BoxFuture, Middleware, Next};
use crate::request::Request;
use crate::response::{IntoResponse, Response};

/// Limits the number of requests handled at the same time: the others wait
/// for their turn, or, with [`ConcurrencyLimit::max_wait`], get `503 Service
/// Unavailable` (with `Retry-After`) when the wait is too long, which keeps
/// the server responsive under a load peak.
///
/// ```
/// use std::time::Duration;
/// use vitesse::prelude::*;
///
/// async fn report(_req: Request) -> &'static str {
///     "an expensive report"
/// }
///
/// let mut app = App::new();
/// // At most 4 reports at once; the others wait up to 2 s, then get a 503.
/// app.get(
///     "/report",
///     report.with(middleware::concurrency_limit(4).max_wait(Duration::from_secs(2))),
/// );
/// ```
pub fn concurrency_limit(max: usize) -> ConcurrencyLimit {
    ConcurrencyLimit {
        semaphore: Semaphore::new(max.clamp(1, Semaphore::MAX_PERMITS)),
        max_wait: None,
    }
}

/// The middleware returned by [`concurrency_limit`].
#[derive(Debug)]
pub struct ConcurrencyLimit {
    semaphore: Semaphore,
    max_wait: Option<Duration>,
}

impl ConcurrencyLimit {
    /// How long a request may wait for its turn before getting a `503`
    /// (`Duration::ZERO`: right away, as soon as the limit is reached). By
    /// default, requests wait as long as needed.
    pub fn max_wait(mut self, max_wait: Duration) -> Self {
        self.max_wait = Some(max_wait);
        self
    }
}

impl Middleware for ConcurrencyLimit {
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response> {
        Box::pin(async move {
            let permit = match self.max_wait {
                None => self.semaphore.acquire().await.ok(),
                Some(wait) if wait.is_zero() => self.semaphore.try_acquire().ok(),
                Some(wait) => tokio::time::timeout(wait, self.semaphore.acquire())
                    .await
                    .ok()
                    .and_then(Result::ok),
            };
            let Some(_permit) = permit else {
                return Error::new(StatusCode::SERVICE_UNAVAILABLE, "the server is busy")
                    .into_response()
                    .header(header::RETRY_AFTER, HeaderValue::from_static("1"));
            };
            next.run(req).await
        })
    }
}

/// Limits each client to `max` requests per `period` (by default, a client
/// is an IP address, see [`App::trust_proxy`](crate::App::trust_proxy)
/// behind a proxy; requests without a known address, on a Unix socket
/// without `trust_proxy`, share one counter). Beyond that: `429 Too Many
/// Requests` with `Retry-After`.
/// Every response carries `X-RateLimit-Limit` and `X-RateLimit-Remaining`.
///
/// Requests are spread evenly over the period (the GCRA algorithm, as in
/// governor): with `rate_limit(60, Duration::from_secs(60))`, a client can
/// send a burst of 60 requests, then one per second.
///
/// ```
/// use std::time::Duration;
/// use vitesse::prelude::*;
///
/// let mut app = App::new();
/// app.middleware(middleware::rate_limit(100, Duration::from_secs(60)));
///
/// // Per API key rather than per IP on a router:
/// let mut api = Router::new();
/// api.middleware(
///     middleware::rate_limit(10, Duration::from_secs(1))
///         .key(|req: &Request| req.header("x-api-key").map(str::to_owned)),
/// );
/// ```
///
/// The counters live in the memory of the process: with several instances
/// of the application, each one counts on its own.
pub fn rate_limit(max: u32, period: Duration) -> RateLimit {
    let max = max.max(1);
    let period = period.min(crate::util::MAX_DELAY);
    RateLimit {
        max,
        period,
        interval: period / max,
        key: KeyFn::Ip,
        hasher: std::collections::hash_map::RandomState::new(),
        shards: std::array::from_fn(|_| {
            Mutex::new(Shard {
                clients: HashMap::new(),
                cleanup_at: 1024,
            })
        }),
    }
}

const SHARDS: usize = 16;

/// The middleware returned by [`rate_limit`].
pub struct RateLimit {
    max: u32,
    period: Duration,
    /// Time between two requests at the sustained rate.
    interval: Duration,
    key: KeyFn,
    hasher: std::collections::hash_map::RandomState,
    shards: [Mutex<Shard>; SHARDS],
}

type KeyFunction = Box<dyn Fn(&Request) -> Option<String> + Send + Sync>;

enum KeyFn {
    Ip,
    Custom(KeyFunction),
}

#[derive(PartialEq, Eq, Hash)]
enum Key {
    Ip(IpAddr),
    /// The requests whose client address is unknown (a Unix socket without
    /// `trust_proxy`): they share one counter rather than escape the limit.
    UnknownIp,
    Custom(Box<str>),
}

struct Shard {
    /// For each client, the "theoretical arrival time" of its next request.
    clients: HashMap<Key, Instant>,
    /// Size from which expired entries are removed.
    cleanup_at: usize,
}

impl fmt::Debug for RateLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RateLimit")
            .field("max", &self.max)
            .field("period", &self.period)
            .finish_non_exhaustive()
    }
}

impl RateLimit {
    /// Identifies clients otherwise than by IP (an API key, a user id…);
    /// requests for which `key` returns `None` are not limited.
    pub fn key<F>(mut self, key: F) -> Self
    where
        F: Fn(&Request) -> Option<String> + Send + Sync + 'static,
    {
        self.key = KeyFn::Custom(Box::new(key));
        self
    }

    /// Records a request; `Ok(remaining)` if it is allowed, `Err(retry
    /// after)` otherwise.
    fn check(&self, key: Key, now: Instant) -> Result<u32, Duration> {
        let shard = &self.shards[self.hasher.hash_one(&key) as usize % SHARDS];
        let mut shard = shard.lock().unwrap_or_else(PoisonError::into_inner);
        let tat = shard
            .clients
            .get(&key)
            .copied()
            .filter(|tat| *tat > now)
            .unwrap_or(now);
        let next = tat + self.interval;
        let ahead = next - now;
        if ahead > self.period {
            return Err(ahead - self.period);
        }
        if shard.clients.len() >= shard.cleanup_at {
            // Les entrées échues équivalent à un client jamais vu.
            shard.clients.retain(|_, tat| *tat > now);
            shard.cleanup_at = (shard.clients.len() * 2).max(1024);
        }
        shard.clients.insert(key, next);
        let remaining = (self.period - ahead).as_nanos() / self.interval.as_nanos().max(1);
        Ok(remaining.min(u128::from(self.max)) as u32)
    }
}

impl Middleware for RateLimit {
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response> {
        let key = match &self.key {
            KeyFn::Ip => Some(req.ip().map_or(Key::UnknownIp, Key::Ip)),
            KeyFn::Custom(f) => f(&req).map(|k| Key::Custom(k.into())),
        };
        let Some(key) = key else {
            return next.run(req);
        };
        let limit = HeaderValue::from(self.max);
        match self.check(key, Instant::now()) {
            Ok(remaining) => Box::pin(async move {
                let mut res = next.run(req).await;
                let headers = res.headers_mut();
                headers.insert(HeaderName::from_static("x-ratelimit-limit"), limit);
                headers.insert(
                    HeaderName::from_static("x-ratelimit-remaining"),
                    HeaderValue::from(remaining),
                );
                res
            }),
            Err(retry_after) => {
                let seconds = retry_after.as_secs() + u64::from(retry_after.subsec_nanos() > 0);
                let res = Error::new(StatusCode::TOO_MANY_REQUESTS, "too many requests")
                    .into_response()
                    .header(header::RETRY_AFTER, HeaderValue::from(seconds.max(1)))
                    .header(HeaderName::from_static("x-ratelimit-limit"), limit)
                    .header(
                        HeaderName::from_static("x-ratelimit-remaining"),
                        HeaderValue::from_static("0"),
                    );
                Box::pin(std::future::ready(res))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gcra() {
        let limit = rate_limit(3, Duration::from_secs(3));
        let ip = |n| Key::Ip(IpAddr::from([10, 0, 0, n]));
        let t0 = Instant::now();
        assert_eq!(limit.check(ip(1), t0), Ok(2));
        assert_eq!(limit.check(ip(1), t0), Ok(1));
        assert_eq!(limit.check(ip(1), t0), Ok(0));
        assert_eq!(limit.check(ip(1), t0), Err(Duration::from_secs(1)));
        // Un autre client a son propre compteur.
        assert_eq!(limit.check(ip(2), t0), Ok(2));
        // Une seconde plus tard, une requête de plus est permise.
        let t1 = t0 + Duration::from_secs(1);
        assert_eq!(limit.check(ip(1), t1), Ok(0));
        assert!(limit.check(ip(1), t1).is_err());
        // Après une longue pause, tout est oublié.
        let t2 = t0 + Duration::from_secs(60);
        assert_eq!(limit.check(ip(1), t2), Ok(2));
    }

    #[test]
    fn expired_clients_are_forgotten() {
        let limit = rate_limit(1, Duration::from_millis(1));
        let t0 = Instant::now();
        for i in 0..20_000u32 {
            let _ = limit.check(Key::Custom(i.to_string().into()), t0);
        }
        let later = t0 + Duration::from_secs(1);
        for i in 0..20_000u32 {
            let _ = limit.check(Key::Custom(format!("b{i}").into()), later);
        }
        let total: usize = limit
            .shards
            .iter()
            .map(|s| s.lock().unwrap().clients.len())
            .sum();
        assert!(total < 30_000, "{total}");
    }
}
