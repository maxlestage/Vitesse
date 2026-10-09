//! Handlers, middlewares et la chaîne `next`.

use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::request::Request;
use crate::response::{IntoResponse, Response};

/// Un `Future` alloué sur le tas, envoyable entre threads.
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Ce qui répond à une requête.
///
/// Toute fonction ou closure `async` qui prend une [`Request`] et renvoie un
/// [`IntoResponse`] est un `Handler` :
///
/// ```
/// use vitesse::prelude::*;
///
/// async fn hello(_req: Request) -> &'static str {
///     "Salut !"
/// }
///
/// let mut app = App::new();
/// app.get("/", hello);
/// app.get("/users/:id", |req: Request| async move {
///     format!("user {}", req.param("id").unwrap_or("?"))
/// });
/// ```
///
/// Les handlers vivent aussi longtemps que l'application, d'où le
/// `&'static self` : un handler peut emprunter ses propres champs depuis son
/// `Future` sans `Arc` ni clone.
pub trait Handler: Send + Sync + 'static {
    /// Traite la requête.
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
    /// Convertit la sortie d'un handler en [`Response`] et transforme une
    /// panique en `500` (sans allocation supplémentaire).
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

/// Un middleware : il reçoit la requête et la suite de la chaîne ([`Next`]).
///
/// Il peut modifier la requête, court-circuiter la chaîne en répondant
/// lui-même, ou modifier la réponse produite par la suite :
///
/// ```
/// use vitesse::prelude::*;
///
/// let mut app = App::new();
/// app.middleware(|req: Request, next: Next| async move {
///     if req.header("x-api-key") != Some("secret") {
///         return Error::unauthorized("clé manquante").into_response();
///     }
///     let res = next.run(req).await;
///     res.header("x-checked", "1")
/// });
/// ```
pub trait Middleware: Send + Sync + 'static {
    /// Traite la requête, en appelant `next.run(req)` pour continuer.
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

/// Une suite de middlewares terminée par un handler.
pub(crate) struct Chain {
    pub(crate) middlewares: Box<[Arc<dyn Middleware>]>,
    pub(crate) endpoint: Arc<dyn Handler>,
}

/// La suite de la chaîne de middlewares (`next()` en Express).
pub struct Next {
    chain: &'static Chain,
    index: usize,
}

impl Next {
    #[inline]
    pub(crate) fn new(chain: &'static Chain) -> Self {
        Next { chain, index: 0 }
    }

    /// Passe la requête au middleware suivant, ou au handler final.
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

/// Un handler précédé de middlewares, créé par [`HandlerExt::with`].
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

    /// Ajoute un middleware, exécuté après ceux déjà présents.
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

/// Ajoute des middlewares à une seule route.
///
/// ```
/// use vitesse::prelude::*;
///
/// async fn auth(req: Request, next: Next) -> Response {
///     match req.header("authorization") {
///         Some(_) => next.run(req).await,
///         None => Error::unauthorized("connectez-vous").into_response(),
///     }
/// }
///
/// async fn admin(_req: Request) -> &'static str {
///     "zone admin"
/// }
///
/// let mut app = App::new();
/// app.get("/admin", admin.with(auth));
/// ```
pub trait HandlerExt: Handler + Sized {
    /// Exécute `middleware` avant ce handler.
    fn with<M: Middleware>(self, middleware: M) -> Chained {
        Chained::new(vec![Arc::new(middleware)], Arc::new(self))
    }
}

impl<H: Handler> HandlerExt for H {}
