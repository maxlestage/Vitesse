//! Handlers, middlewares and the `next` chain.

use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::request::Request;
use crate::response::{IntoResponse, Response};

/// A heap-allocated `Future` that can be sent between threads.
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Something that responds to a request.
///
/// Any `async` function or closure that takes a [`Request`] and returns an
/// [`IntoResponse`] is a `Handler`:
///
/// ```
/// use vitesse::prelude::*;
///
/// async fn hello(_req: Request) -> &'static str {
///     "Hello!"
/// }
///
/// let mut app = App::new();
/// app.get("/", hello);
/// app.get("/users/:id", |req: Request| async move {
///     format!("user {}", req.param("id").unwrap_or("?"))
/// });
/// ```
///
/// Handlers live as long as the application, hence the `&'static self`: a
/// handler can borrow its own fields from its `Future` without an `Arc` or a
/// clone.
pub trait Handler: Send + Sync + 'static {
    /// Handles the request.
    fn call(&'static self, req: Request) -> BoxFuture<Response>;
}

impl<F, Fut> Handler for F
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future + Send + 'static,
    Fut::Output: IntoResponse,
{
    #[inline]
    fn call(&'static self, req: Request) -> BoxFuture<Response> {
        match catch_unwind(AssertUnwindSafe(|| self(req))) {
            Ok(fut) => Box::pin(HandlerFuture { fut }),
            Err(_) => Box::pin(std::future::ready(panic_response())),
        }
    }
}

pin_project_lite::pin_project! {
    /// Converts the output of a handler into a [`Response`] and turns a
    /// panic into a `500` (with no extra allocation).
    struct HandlerFuture<Fut> {
        #[pin]
        fut: Fut,
    }
}

impl<Fut> Future for HandlerFuture<Fut>
where
    Fut: Future,
    Fut::Output: IntoResponse,
{
    type Output = Response;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Response> {
        let fut = self.project().fut;
        let polled = catch_unwind(AssertUnwindSafe(|| match fut.poll(cx) {
            Poll::Ready(out) => Poll::Ready(out.into_response()),
            Poll::Pending => Poll::Pending,
        }));
        polled.unwrap_or_else(|_| Poll::Ready(panic_response()))
    }
}

pub(crate) fn panic_response() -> Response {
    crate::Error::internal("Internal Server Error").into_response()
}

/// A middleware: it receives the request and the rest of the chain
/// ([`Next`]).
///
/// It can modify the request, short-circuit the chain by responding itself,
/// or modify the response produced by the rest of the chain:
///
/// ```
/// use vitesse::prelude::*;
///
/// let mut app = App::new();
/// app.middleware(|req: Request, next: Next| async move {
///     if req.header("x-api-key") != Some("secret") {
///         return Error::unauthorized("missing key").into_response();
///     }
///     let res = next.run(req).await;
///     res.header("x-checked", "1")
/// });
/// ```
pub trait Middleware: Send + Sync + 'static {
    /// Handles the request, calling `next.run(req)` to continue.
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response>;
}

impl<F, Fut> Middleware for F
where
    F: Fn(Request, Next) -> Fut + Send + Sync + 'static,
    Fut: Future + Send + 'static,
    Fut::Output: IntoResponse,
{
    #[inline]
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response> {
        let fut = self(req, next);
        Box::pin(async move { fut.await.into_response() })
    }
}

/// A sequence of middlewares ending with a handler.
pub(crate) struct Chain {
    pub(crate) middlewares: Box<[Arc<dyn Middleware>]>,
    pub(crate) endpoint: Arc<dyn Handler>,
}

/// The rest of the middleware chain (`next()` in Express).
pub struct Next {
    chain: &'static Chain,
    index: usize,
}

impl Next {
    #[inline]
    pub(crate) fn new(chain: &'static Chain) -> Self {
        Next { chain, index: 0 }
    }

    /// Passes the request to the next middleware, or to the final handler.
    #[inline]
    pub fn run(mut self, req: Request) -> BoxFuture<Response> {
        let chain = self.chain;
        match chain.middlewares.get(self.index) {
            Some(mw) => {
                self.index += 1;
                mw.handle(req, self)
            }
            None => chain.endpoint.call(req),
        }
    }
}

/// A handler preceded by middlewares, created by [`HandlerExt::with`].
pub struct Chained {
    chain: Chain,
}

impl Chained {
    pub(crate) fn new(middlewares: Vec<Arc<dyn Middleware>>, endpoint: Arc<dyn Handler>) -> Self {
        Chained {
            chain: Chain {
                middlewares: middlewares.into(),
                endpoint,
            },
        }
    }

    /// Adds a middleware, run after the ones already present.
    pub fn with<M: Middleware>(self, middleware: M) -> Chained {
        let mut middlewares = self.chain.middlewares.into_vec();
        middlewares.push(Arc::new(middleware));
        Chained::new(middlewares, self.chain.endpoint)
    }
}

impl Handler for Chained {
    #[inline]
    fn call(&'static self, req: Request) -> BoxFuture<Response> {
        Next::new(&self.chain).run(req)
    }
}

/// Adds middlewares to a single route.
///
/// ```
/// use vitesse::prelude::*;
///
/// async fn auth(req: Request, next: Next) -> Response {
///     match req.header("authorization") {
///         Some(_) => next.run(req).await,
///         None => Error::unauthorized("please log in").into_response(),
///     }
/// }
///
/// async fn admin(_req: Request) -> &'static str {
///     "admin area"
/// }
///
/// let mut app = App::new();
/// app.get("/admin", admin.with(auth));
/// ```
pub trait HandlerExt: Handler + Sized {
    /// Runs `middleware` before this handler.
    fn with<M: Middleware>(self, middleware: M) -> Chained {
        Chained::new(vec![Arc::new(middleware)], Arc::new(self))
    }
}

impl<H: Handler> HandlerExt for H {}
