//! L'application (`express()`).

use std::io;
use std::sync::Arc;

use http::header::{self, HeaderValue};
use http::{Method, StatusCode};

use crate::error::Error;
use crate::handler::{BoxFuture, Chain, Handler, Middleware, Next};
use crate::request::{Request, Shared, StateMap};
use crate::response::{IntoResponse, Response};
use crate::router::{MethodMap, Route, routing_methods};
use crate::server::{self, ListenAddr, Server};
use crate::tree::{Tree, param_names, parse_pattern};

/// Taille maximale par défaut d'un corps de requête lu en mémoire : 1 Mio.
pub const DEFAULT_BODY_LIMIT: usize = 1024 * 1024;

/// Une application Vitesse (`const app = express()`).
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
    /// Une application vide.
    pub fn new() -> Self {
        App {
            tree: Tree::new(),
            middlewares: Vec::new(),
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
        let segments = parse_pattern(path).unwrap_or_else(|e| panic!("route invalide : {e}"));
        let route = Route {
            handler,
            names: param_names(&segments),
        };
        let methods = self.tree.entry(&segments, MethodMap::default);
        if methods.insert(method.clone(), route).is_err() {
            let method = method.map_or("ALL".to_owned(), |m| m.to_string());
            panic!("route en double : {method} {path} est déjà définie");
        }
    }

    /// Ajoute un middleware global (`app.use(fn)`).
    ///
    /// Les middlewares globaux s'exécutent dans l'ordre d'ajout, avant le
    /// routage, pour **toutes** les requêtes (y compris les 404).
    pub fn middleware<M: Middleware>(&mut self, middleware: M) -> &mut Self {
        self.middlewares.push(Arc::new(middleware));
        self
    }

    /// Le handler appelé quand aucune route ne correspond (404 par défaut).
    pub fn fallback<H: Handler>(&mut self, handler: H) -> &mut Self {
        self.fallback = Some(Arc::new(handler));
        self
    }

    /// Personnalise les réponses d'erreur (le `(err, req, res, next)` d'Express).
    ///
    /// Appelé pour chaque réponse issue d'une [`Error`] (y compris les 404,
    /// les erreurs de parsing et les paniques).
    ///
    /// ```
    /// use vitesse::prelude::*;
    ///
    /// let mut app = App::new();
    /// app.on_error(|err: Error| {
    ///     res::status(err.status()).html(format!("<h1>Oups : {}</h1>", err.message()))
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

    /// Enregistre un état global, accessible avec [`Request::state`].
    ///
    /// ```
    /// use vitesse::prelude::*;
    /// use std::sync::atomic::{AtomicU64, Ordering};
    ///
    /// let mut app = App::new();
    /// app.state(AtomicU64::new(0));
    /// app.get("/visits", |req: Request| async move {
    ///     let n = req.state::<AtomicU64>().fetch_add(1, Ordering::Relaxed);
    ///     format!("visite n°{}", n + 1)
    /// });
    /// ```
    pub fn state<T: Send + Sync + 'static>(&mut self, value: T) -> &mut Self {
        self.state.insert(value);
        self
    }

    /// Taille maximale d'un corps de requête lu en mémoire (1 Mio par défaut).
    pub fn body_limit(&mut self, bytes: usize) -> &mut Self {
        self.body_limit = bytes;
        self
    }

    /// Nombre de threads du serveur pour [`App::run`] (un par cœur par défaut).
    pub fn workers(&mut self, workers: usize) -> &mut Self {
        self.workers = Some(workers.max(1));
        self
    }

    /// Mode « un thread par cœur » pour [`App::run`] : activé par défaut
    /// sous Linux (ignoré ailleurs).
    ///
    /// Chaque thread a sa propre boucle d'événements et son propre socket
    /// (`SO_REUSEPORT`) : le noyau répartit les connexions et une requête ne
    /// change jamais de thread, sans aucune synchronisation entre cœurs. C'est
    /// le mode le plus rapide.
    ///
    /// En contrepartie, la charge n'est pas rééquilibrée entre threads :
    /// désactivez-le (`false`) pour utiliser le runtime multi-thread de tokio
    /// si des handlers font de longs calculs bloquants ou si les connexions
    /// sont peu nombreuses et très inégales.
    pub fn thread_per_core(&mut self, enabled: bool) -> &mut Self {
        self.thread_per_core = enabled;
        self
    }

    /// Fige l'application. Elle vit ensuite jusqu'à la fin du programme : les
    /// requêtes n'ont ainsi jamais à manipuler de compteur de références.
    pub(crate) fn build(self) -> &'static AppService {
        let fallback = self.fallback.unwrap_or_else(|| Arc::new(not_found));
        let dispatcher: Arc<dyn Handler> = Arc::new(Dispatcher {
            tree: self.tree,
            fallback,
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

    /// Ouvre le port sans démarrer le serveur : pratique pour connaître le
    /// port choisi quand on écoute sur le port `0`.
    pub async fn bind(self, addr: impl ListenAddr) -> io::Result<Server> {
        let listener = server::bind(&addr.socket_addrs()?, false)?;
        Server::new(self.build(), tokio::net::TcpListener::from_std(listener)?)
    }

    /// Démarre le serveur sur le runtime tokio courant (`app.listen(3000)`).
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

    /// Démarre le serveur sans avoir besoin de `#[tokio::main]` : crée les
    /// threads (un par cœur, voir [`App::workers`] et
    /// [`App::thread_per_core`]) et s'arrête proprement sur `Ctrl+C` /
    /// `SIGTERM`. C'est le mode le plus rapide.
    pub fn run(self, addr: impl ListenAddr) -> io::Result<()> {
        let addrs = addr.socket_addrs()?;
        let workers = self
            .workers
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()));
        let thread_per_core = self.thread_per_core;
        server::run(self.build(), &addrs, workers, thread_per_core)
    }
}

/// L'application figée, prête à servir.
pub(crate) struct AppService {
    chain: Chain,
    pub(crate) shared: Shared,
}

impl AppService {
    #[inline]
    pub(crate) fn handle(&'static self, req: Request) -> BoxFuture<Response> {
        Next::new(&self.chain).run(req)
    }
}

/// Le routage : trouve la route et appelle son handler.
struct Dispatcher {
    tree: Tree<MethodMap>,
    fallback: Arc<dyn Handler>,
}

impl Handler for Dispatcher {
    #[inline]
    fn call(&'static self, mut req: Request) -> BoxFuture<Response> {
        let Some((methods, captures)) = self.tree.find(req.path()) else {
            return self.fallback.call(req);
        };
        match methods.find(req.method()) {
            Some(route) => {
                if !captures.is_empty() {
                    req.set_params(&route.names, captures);
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

/// Applique `app.on_error` aux réponses issues d'une erreur.
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
