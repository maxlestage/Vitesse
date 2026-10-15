//! Lifecycle hooks (Fastify's `onRequest`, `preHandler`, `onSend`),
//! `param` callbacks and plugins.

use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use crate::app::App;
use crate::error::Error;
use crate::handler::{BoxFuture, Chained, Handler, Middleware, Next, panic_response};
use crate::request::Request;
use crate::response::{IntoResponse, Response};

/// A hook that runs on the request: `Err` answers with the error.
pub(crate) type RequestHook = Box<dyn Fn(&mut Request) -> Result<(), Error> + Send + Sync>;
/// A hook that runs on the response.
pub(crate) type ResponseHook = Box<dyn Fn(&mut Response) + Send + Sync>;

/// Runs the request hooks; the response that stops the request, if any.
#[inline]
fn run_request_hooks(hooks: &[RequestHook], req: &mut Request) -> Option<Response> {
    for hook in hooks {
        match catch_unwind(AssertUnwindSafe(|| hook(req))) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Some(error.into_response()),
            Err(_) => return Some(panic_response()),
        }
    }
    None
}

/// `app.on_request` and `app.on_response`: a single middleware at the head
/// of the chain (after `app.on_error`). Without response hooks it adds no
/// allocation: the request goes on with the future of the next middleware.
pub(crate) struct Hooks {
    pub(crate) on_request: Box<[RequestHook]>,
    pub(crate) on_response: Box<[ResponseHook]>,
}

impl Middleware for Hooks {
    fn handle(&'static self, mut req: Request, next: Next) -> BoxFuture<Response> {
        if let Some(res) = run_request_hooks(&self.on_request, &mut req) {
            return Box::pin(std::future::ready(self.respond(res)));
        }
        if self.on_response.is_empty() {
            return next.run(req);
        }
        let future = next.run(req);
        Box::pin(async move { self.respond(future.await) })
    }
}

impl Hooks {
    /// Runs the response hooks.
    fn respond(&self, mut res: Response) -> Response {
        for hook in &*self.on_response {
            if catch_unwind(AssertUnwindSafe(|| hook(&mut res))).is_err() {
                return panic_response();
            }
        }
        res
    }
}

/// `app.pre_handler`: the hooks, then the route's handler (its middlewares
/// included).
pub(crate) struct PreHandler {
    pub(crate) hooks: Arc<[RequestHook]>,
    pub(crate) handler: Arc<dyn Handler>,
}

impl Handler for PreHandler {
    fn call(&'static self, mut req: Request) -> BoxFuture<Response> {
        if let Some(res) = run_request_hooks(&self.hooks, &mut req) {
            return Box::pin(std::future::ready(res));
        }
        self.handler.call(req)
    }
}

/// The handler preceded by the `param` callbacks of the parameters it has,
/// in the order of the parameters, or `None` if none applies.
pub(crate) fn with_params(
    params: &[(Box<str>, Arc<dyn Middleware>)],
    names: &[Box<str>],
    handler: &Arc<dyn Handler>,
) -> Option<Arc<dyn Handler>> {
    let middlewares: Vec<Arc<dyn Middleware>> = names
        .iter()
        .flat_map(|name| {
            params
                .iter()
                .filter(move |(param, _)| param == name)
                .map(|(_, mw)| mw.clone())
        })
        .collect();
    if middlewares.is_empty() {
        return None;
    }
    Some(Arc::new(Chained::new(middlewares, handler.clone())))
}

/// The middleware of `bind_param`.
pub(crate) struct BindParam<F> {
    name: Box<str>,
    loader: F,
}

impl<F> BindParam<F> {
    pub(crate) fn new(name: &str, loader: F) -> Self {
        BindParam {
            name: name.into(),
            loader,
        }
    }
}

impl<T, F, Fut> Middleware for BindParam<F>
where
    T: Clone + Send + Sync + 'static,
    F: Fn(&Request, &str) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, Error>> + Send + 'static,
{
    fn handle(&'static self, mut req: Request, next: Next) -> BoxFuture<Response> {
        let Some(value) = req.param(&self.name) else {
            return next.run(req);
        };
        let loading = match catch_unwind(AssertUnwindSafe(|| (self.loader)(&req, value))) {
            Ok(loading) => loading,
            Err(_) => return Box::pin(std::future::ready(panic_response())),
        };
        Box::pin(async move {
            match loading.await {
                Ok(value) => {
                    req.set(value);
                    next.run(req).await
                }
                Err(error) => error.into_response(),
            }
        })
    }
}

/// A reusable piece of application (Fastify's plugins): routes,
/// middlewares, state, hooks… added in one call with
/// [`App::register`].
///
/// Any `FnOnce(&mut App)` is a plugin; a struct can carry its options:
///
/// ```
/// use vitesse::prelude::*;
/// use vitesse::Plugin;
///
/// /// Adds `GET <path>` answering "ok".
/// struct Health {
///     path: &'static str,
/// }
///
/// impl Plugin for Health {
///     fn register(self, app: &mut App) {
///         app.get(self.path, |_| async { "ok" });
///     }
/// }
///
/// fn api(app: &mut App) {
///     app.get("/api/ping", |_| async { "pong" });
/// }
///
/// let mut app = App::new();
/// app.register(Health { path: "/healthz" }).register(api);
/// ```
///
/// For routes encapsulated under a prefix, with middlewares of their own,
/// see [`App::group`].
pub trait Plugin {
    /// Adds the plugin to the application.
    fn register(self, app: &mut App);
}

impl<F: FnOnce(&mut App)> Plugin for F {
    fn register(self, app: &mut App) {
        self(app)
    }
}
