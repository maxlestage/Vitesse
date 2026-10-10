//! The application (`express()`).

use std::io;
use std::sync::Arc;

use http::header::{self, HeaderValue};
use http::{Method, StatusCode};

use crate::error::Error;
use crate::handler::{BoxFuture, Chain, Handler, Middleware, Next};
use crate::request::{Request, Shared, StateMap};
use crate::response::{IntoResponse, Response};
use crate::router::{MethodMap, Route, Scope, routing_methods};
use crate::server::{self, ListenAddr, Server};
use crate::static_files::{Conditional, with_conditional};
use crate::tree::{Tree, param_names, parse_pattern};

/// Default maximum size of a request body read into memory: 1 MiB.
pub const DEFAULT_BODY_LIMIT: usize = 1024 * 1024;

/// A Vitesse application (`const app = express()`).
///
/// ```no_run
/// use vitesse::prelude::*;
///
/// fn main() -> std::io::Result<()> {
///     let mut app = App::new();
///
///     app.middleware(middleware::logger());
///     app.get("/", |_| async { "Hello World!" });
///     app.get("/users/:id", |req: Request| async move {
///         Json(json!({ "id": req.param("id") }))
///     });
///
///     app.run(3000)
/// }
/// ```
pub struct App {
    tree: Tree<MethodMap>,
    middlewares: Vec<Arc<dyn Middleware>>,
    /// Middlewares of the mounted routers, by prefix.
    scopes: Vec<Scope>,
    fallback: Option<Arc<dyn Handler>>,
    error_handler: Option<Arc<dyn Fn(Error) -> Response + Send + Sync>>,
    state: StateMap,
    body_limit: usize,
    workers: Option<usize>,
    thread_per_core: bool,
}

impl Default for App {
    fn default() -> Self {
        App::new()
    }
}

impl App {
    /// Creates an empty application.
    pub fn new() -> Self {
        App {
            tree: Tree::new(),
            middlewares: Vec::new(),
            scopes: Vec::new(),
            fallback: None,
            error_handler: None,
            state: StateMap::default(),
            body_limit: DEFAULT_BODY_LIMIT,
            workers: None,
            thread_per_core: true,
        }
    }

    routing_methods!();

    #[track_caller]
    fn add_route(&mut self, method: Option<Method>, path: &str, handler: Arc<dyn Handler>) {
        // Un `match` plutôt qu'une closure : `#[track_caller]` désigne ainsi la
        // ligne de l'utilisateur.
        let segments = match parse_pattern(path) {
            Ok(segments) => segments,
            Err(e) => panic!("invalid route: {e}"),
        };
        let route = Route {
            handler,
            names: param_names(&segments),
        };
        let methods = self.tree.entry(&segments, MethodMap::default);
        if methods.insert(method.clone(), route).is_err() {
            let method = method.map_or("ALL".to_owned(), |m| m.to_string());
            panic!("duplicate route: {method} {path} is already defined");
        }
    }

    /// Adds a global middleware (`app.use(fn)`).
    ///
    /// Global middlewares run in the order they were added, before routing,
    /// for **every** request (including 404s).
    pub fn middleware<M: Middleware>(&mut self, middleware: M) -> &mut Self {
        self.middlewares.push(Arc::new(middleware));
        self
    }

    /// Sets the handler called when no route matches (a 404 by default).
    pub fn fallback<H: Handler>(&mut self, handler: H) -> &mut Self {
        self.fallback = Some(Arc::new(handler));
        self
    }

    /// Customizes error responses (Express's `(err, req, res, next)`).
    ///
    /// Called for every response produced from an [`Error`] (including 404s,
    /// parsing errors and panics).
    ///
    /// ```
    /// use vitesse::prelude::*;
    ///
    /// let mut app = App::new();
    /// app.on_error(|err: Error| {
    ///     res::status(err.status()).html(format!("<h1>Oops: {}</h1>", err.message()))
    /// });
    /// ```
    pub fn on_error<F, R>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(Error) -> R + Send + Sync + 'static,
        R: IntoResponse,
    {
        self.error_handler = Some(Arc::new(move |err| handler(err).into_response()));
        self
    }

    /// Registers global state, accessible with [`Request::state`].
    ///
    /// ```
    /// use vitesse::prelude::*;
    /// use std::sync::atomic::{AtomicU64, Ordering};
    ///
    /// let mut app = App::new();
    /// app.state(AtomicU64::new(0));
    /// app.get("/visits", |req: Request| async move {
    ///     let n = req.state::<AtomicU64>().fetch_add(1, Ordering::Relaxed);
    ///     format!("visit #{}", n + 1)
    /// });
    /// ```
    pub fn state<T: Send + Sync + 'static>(&mut self, value: T) -> &mut Self {
        self.state.insert(value);
        self
    }

    /// Maximum size of a request body read into memory (1 MiB by default).
    pub fn body_limit(&mut self, bytes: usize) -> &mut Self {
        self.body_limit = bytes;
        self
    }

    /// Number of server threads for [`App::run`] (one per core by default).
    pub fn workers(&mut self, workers: usize) -> &mut Self {
        self.workers = Some(workers.max(1));
        self
    }

    /// "Thread per core" mode for [`App::run`]: enabled by default on Linux
    /// (ignored elsewhere).
    ///
    /// Each thread has its own event loop and its own socket
    /// (`SO_REUSEPORT`): the kernel spreads connections across threads and a
    /// request never changes thread, with no synchronization at all between
    /// cores. This is the fastest mode.
    ///
    /// The trade-off is that load is not rebalanced between threads: disable
    /// it (`false`) to use tokio's multi-threaded runtime if handlers perform
    /// long blocking computations, or if connections are few and very
    /// uneven.
    pub fn thread_per_core(&mut self, enabled: bool) -> &mut Self {
        self.thread_per_core = enabled;
        self
    }

    /// Freezes the application. It then lives until the end of the program,
    /// so requests never have to touch a reference count.
    pub(crate) fn build(self) -> &'static AppService {
        let fallback = self.fallback.unwrap_or_else(|| Arc::new(not_found));
        let core = Arc::new(Core {
            tree: self.tree,
            fallback,
        });
        let dispatcher: Arc<dyn Handler> = Arc::new(Dispatcher {
            scopes: build_scopes(self.scopes, &core),
            core,
        });
        let mut middlewares = self.middlewares;
        if let Some(handler) = self.error_handler {
            middlewares.insert(0, Arc::new(ErrorHandler(handler)));
        }
        Box::leak(Box::new(AppService {
            chain: Chain {
                middlewares: middlewares.into(),
                endpoint: dispatcher,
            },
            shared: Shared {
                state: self.state,
                body_limit: self.body_limit,
            },
        }))
    }

    /// Opens the port without starting the server: handy to find out which
    /// port was picked when listening on port `0`.
    pub async fn bind(self, addr: impl ListenAddr) -> io::Result<Server> {
        let listener = server::bind(&addr.socket_addrs()?, false)?;
        Server::new(self.build(), tokio::net::TcpListener::from_std(listener)?)
    }

    /// Starts the server on the current tokio runtime (`app.listen(3000)`).
    ///
    /// Like [`App::run`], it stops gracefully on `Ctrl+C` or `SIGTERM`:
    /// in-flight requests get up to 10 s to finish. [`App::workers`] and
    /// [`App::thread_per_core`] do not apply here: the runtime is yours.
    ///
    /// ```no_run
    /// # use vitesse::prelude::*;
    /// #[tokio::main]
    /// async fn main() -> std::io::Result<()> {
    ///     let mut app = App::new();
    ///     app.get("/", |_| async { "Hello World!" });
    ///     app.listen(3000).await
    /// }
    /// ```
    pub async fn listen(self, addr: impl ListenAddr) -> io::Result<()> {
        self.bind(addr).await?.run().await
    }

    /// Starts the server without needing `#[tokio::main]`: spawns the
    /// threads (one per core, see [`App::workers`] and
    /// [`App::thread_per_core`]) and shuts down gracefully on `Ctrl+C` /
    /// `SIGTERM`. This is the fastest mode.
    pub fn run(self, addr: impl ListenAddr) -> io::Result<()> {
        let addrs = addr.socket_addrs()?;
        let workers = self
            .workers
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()));
        let thread_per_core = self.thread_per_core;
        server::run(self.build(), &addrs, workers, thread_per_core)
    }
}

/// The frozen application, ready to serve.
pub(crate) struct AppService {
    chain: Chain,
    pub(crate) shared: Shared,
}

impl AppService {
    #[inline]
    pub(crate) fn handle(&'static self, req: Request) -> BoxFuture<Response> {
        if req.is_conditional() {
            // Rare : `Range`, `If-None-Match`… restent lisibles par
            // `res::file` pendant tout le traitement.
            let conditional = Conditional::from_request(&req);
            return with_conditional(conditional, Next::new(&self.chain).run(req));
        }
        Next::new(&self.chain).run(req)
    }
}

/// Routing: finds the route and calls its handler.
struct Dispatcher {
    core: Arc<Core>,
    /// Chains of the mounted routers' middlewares, longest prefix first.
    scopes: Box<[(Box<str>, Chain)]>,
}

impl Handler for Dispatcher {
    #[inline]
    fn call(&'static self, mut req: Request) -> BoxFuture<Response> {
        if let Some(route) = req
            .find_route(&self.core.tree)
            .and_then(|methods| methods.find(req.method()))
        {
            if !route.names.is_empty() {
                req.set_params(&route.names);
            }
            return route.handler.call(req);
        }
        // Aucune route ne répond : les middlewares des routeurs montés sur ce
        // préfixe passent d'abord, puis le 404, le 405 ou l'OPTIONS automatique.
        let path = req.path();
        match self.scopes.iter().find(|(prefix, _)| under(path, prefix)) {
            Some((_, chain)) => Next::new(chain).run(req),
            None => self.core.call(req),
        }
    }
}

/// `path` est sous `prefix`, segment par segment (`/api` couvre `/api` et
/// `/api/x`, pas `/apix`).
fn under(path: &str, prefix: &str) -> bool {
    prefix == "/"
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Pour chaque préfixe, la chaîne de tous les middlewares qui le couvrent :
/// les préfixes les plus courts (routeurs extérieurs) d'abord, puis dans
/// l'ordre de montage.
fn build_scopes(scopes: Vec<Scope>, core: &Arc<Core>) -> Box<[(Box<str>, Chain)]> {
    let depth = |p: &str| p.split('/').filter(|s| !s.is_empty()).count();
    let mut ordered: Vec<(usize, Scope)> = scopes.into_iter().enumerate().collect();
    ordered.sort_by_key(|(i, (prefix, _))| (depth(prefix), *i));
    let mut prefixes: Vec<&str> = ordered.iter().map(|(_, (p, _))| p.as_str()).collect();
    prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
    prefixes.dedup();
    prefixes
        .into_iter()
        .map(|prefix| {
            let middlewares: Vec<Arc<dyn Middleware>> = ordered
                .iter()
                .filter(|(_, (p, _))| under(prefix, p))
                .flat_map(|(_, (_, m))| m.iter().cloned())
                .collect();
            let chain = Chain {
                middlewares: middlewares.into(),
                endpoint: core.clone(),
            };
            (prefix.into(), chain)
        })
        .collect()
}

/// Routing without the routers' middlewares: the route, or else the `405` /
/// automatic `OPTIONS`, or else the fallback (`404`). A router middleware may
/// have rewritten the path, hence the new lookup.
struct Core {
    tree: Tree<MethodMap>,
    fallback: Arc<dyn Handler>,
}

impl Handler for Core {
    fn call(&'static self, mut req: Request) -> BoxFuture<Response> {
        let Some(methods) = req.find_route(&self.tree) else {
            return self.fallback.call(req);
        };
        match methods.find(req.method()) {
            Some(route) => {
                if !route.names.is_empty() {
                    req.set_params(&route.names);
                }
                route.handler.call(req)
            }
            None => {
                let allow = HeaderValue::try_from(methods.allow()).ok();
                let mut res = if *req.method() == Method::OPTIONS {
                    Response::new().status(StatusCode::NO_CONTENT)
                } else {
                    Error::from_status(StatusCode::METHOD_NOT_ALLOWED).into_response()
                };
                if let Some(allow) = allow {
                    res.headers_mut().insert(header::ALLOW, allow);
                }
                Box::pin(std::future::ready(res))
            }
        }
    }
}

async fn not_found(req: Request) -> Error {
    Error::not_found(format!("Cannot {} {}", req.method(), req.path()))
}

/// Applies `app.on_error` to responses produced from an error.
struct ErrorHandler(Arc<dyn Fn(Error) -> Response + Send + Sync>);

impl Middleware for ErrorHandler {
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response> {
        Box::pin(async move {
            let mut res = next.run(req).await;
            let Some(err) = res.take_error() else {
                return res;
            };
            let mut custom = (self.0)(err);
            // On conserve les en-têtes posés par les middlewares (CORS…).
            for (name, value) in res.headers() {
                if name != header::CONTENT_TYPE
                    && name != header::CONTENT_LENGTH
                    && !custom.headers().contains_key(name)
                {
                    custom.headers_mut().append(name.clone(), value.clone());
                }
            }
            custom
        })
    }
}
