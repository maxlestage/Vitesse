//! The application (`express()`).

use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use http::header::{self, HeaderValue};
use http::{Method, StatusCode};

use bytes::Bytes;

use crate::error::Error;
use crate::handler::{BoxFuture, Chain, Handler, Middleware, Next};
use crate::hooks::{Hooks, Plugin, PreHandler, RequestHook, ResponseHook};
use crate::proxy::TrustProxy;
use crate::request::{Request, Shared, StateMap};
use crate::response::{IntoResponse, Response};
use crate::router::{MethodMap, Route, Scope, routing_methods};
use crate::server::{self, ListenAddr, Server, ServerConfig};
use crate::static_files::{Conditional, with_conditional};
use crate::tree::{Tree, UrlTemplate, covers, param_names, parse_pattern, variants};

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
    /// Named routes: name → pattern.
    names: HashMap<String, String>,
    /// The pattern of the last route added, for [`App::name`].
    last_route: Option<String>,
    /// Every route, for the OpenAPI document.
    api_routes: Vec<crate::openapi::ApiRoute>,
    /// The route that [`App::doc`] describes, in `api_routes`.
    last_doc: Option<usize>,
    /// Where the OpenAPI document is served ([`App::openapi`]).
    openapi: Option<(String, crate::openapi::OpenApi)>,
    /// `param` callbacks: parameter name → middleware.
    params: Vec<(Box<str>, Arc<dyn Middleware>)>,
    on_request: Vec<RequestHook>,
    pre_handler: Vec<RequestHook>,
    on_response: Vec<ResponseHook>,
    middlewares: Vec<Arc<dyn Middleware>>,
    /// Middlewares of the mounted routers, by prefix.
    scopes: Vec<Scope>,
    fallback: Option<Arc<dyn Handler>>,
    error_handler: Option<Arc<dyn Fn(Error) -> Response + Send + Sync>>,
    state: StateMap,
    body_limit: usize,
    trust_proxy: TrustProxy,
    workers: Option<usize>,
    thread_per_core: bool,
    config: ServerConfig,
    #[cfg(feature = "session")]
    cookie_key: Option<crate::session::Key>,
    #[cfg(feature = "templates")]
    templates: Option<minijinja::Environment<'static>>,
    #[cfg(feature = "http3")]
    http3: Option<crate::http3::Http3>,
    #[cfg(feature = "tls")]
    tls: Option<crate::tls::Tls>,
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
            names: HashMap::new(),
            last_route: None,
            api_routes: Vec::new(),
            last_doc: None,
            openapi: None,
            params: Vec::new(),
            on_request: Vec::new(),
            pre_handler: Vec::new(),
            on_response: Vec::new(),
            fallback: None,
            error_handler: None,
            state: StateMap::default(),
            body_limit: DEFAULT_BODY_LIMIT,
            trust_proxy: TrustProxy::None,
            workers: None,
            thread_per_core: true,
            config: ServerConfig::default(),
            #[cfg(feature = "session")]
            cookie_key: None,
            #[cfg(feature = "templates")]
            templates: None,
            #[cfg(feature = "http3")]
            http3: None,
            #[cfg(feature = "tls")]
            tls: None,
        }
    }

    routing_methods!();

    #[track_caller]
    fn add_name(&mut self, name: &str, pattern: String) {
        if self.names.insert(name.to_owned(), pattern).is_some() {
            panic!("duplicate route name: '{name}' is already used");
        }
    }

    #[track_caller]
    fn add_route(&mut self, method: Option<Method>, path: &str, handler: Arc<dyn Handler>) {
        self.insert_route(method, path, handler, None, true);
    }

    #[track_caller]
    fn add_mounted_route(
        &mut self,
        method: Option<Method>,
        path: &str,
        handler: Arc<dyn Handler>,
        doc: Option<crate::openapi::Operation>,
    ) {
        self.insert_route(method, path, handler, doc, false);
    }

    fn set_doc(&mut self, operation: crate::openapi::Operation) -> bool {
        match self.last_doc.and_then(|i| self.api_routes.get_mut(i)) {
            Some(route) => {
                route.doc = Some(operation);
                true
            }
            None => false,
        }
    }

    #[track_caller]
    fn insert_route(
        &mut self,
        method: Option<Method>,
        path: &str,
        handler: Arc<dyn Handler>,
        doc: Option<crate::openapi::Operation>,
        own: bool,
    ) {
        // Un `match` plutôt qu'une closure : `#[track_caller]` désigne ainsi la
        // ligne de l'utilisateur.
        let segments = match parse_pattern(path) {
            Ok(segments) => segments,
            Err(e) => panic!("invalid route: {e}"),
        };
        self.last_route = Some(path.to_owned());
        self.last_doc = Some(self.api_routes.len());
        self.api_routes.push(crate::openapi::ApiRoute {
            method: method.clone(),
            pattern: path.to_owned(),
            doc,
        });
        // `/archive/:year?` est enregistrée deux fois : `/archive` et `/archive/:year`.
        for variant in variants(&segments) {
            let route = Route {
                handler: handler.clone(),
                names: param_names(&variant),
                pattern: path.into(),
                own,
            };
            let methods = self.tree.entry(&variant, MethodMap::default);
            if methods.insert(method.clone(), route).is_err() {
                let method = method.map_or("ALL".to_owned(), |m| m.to_string());
                panic!("duplicate route: {method} {path} is already defined");
            }
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

    /// Adds a plugin: a function (or a [`Plugin`] struct) that configures
    /// the application, Fastify's `register`. See [`Plugin`].
    ///
    /// ```
    /// use vitesse::prelude::*;
    ///
    /// fn health(app: &mut App) {
    ///     app.get("/healthz", |_| async { "ok" });
    /// }
    ///
    /// let mut app = App::new();
    /// app.register(health);
    /// ```
    pub fn register<P: Plugin>(&mut self, plugin: P) -> &mut Self {
        plugin.register(self);
        self
    }

    /// A hook run on every request, before the middlewares and the routing
    /// (Fastify's `onRequest`): it can read or change the request (headers,
    /// [`Request::set`]…), or stop it by returning an error, which becomes
    /// the response (through [`App::on_error`]). Hooks run in the order they
    /// were added. Lighter than a middleware: a plain function, no future.
    ///
    /// ```
    /// use vitesse::prelude::*;
    ///
    /// let mut app = App::new();
    /// app.on_request(|req: &mut Request| {
    ///     if req.header("x-api-key") != Some("secret") {
    ///         return Err(Error::unauthorized("missing API key"));
    ///     }
    ///     Ok(())
    /// });
    /// ```
    pub fn on_request<F>(&mut self, hook: F) -> &mut Self
    where
        F: Fn(&mut Request) -> Result<(), Error> + Send + Sync + 'static,
    {
        self.on_request.push(Box::new(hook));
        self
    }

    /// A hook run after routing, just before the route's middlewares and
    /// handler (Fastify's `preHandler`): [`Request::route`] and the
    /// parameters are known. Returning an error stops the request. Routes
    /// of mounted routers included; not run for `404`, `405` or the
    /// automatic `OPTIONS`.
    ///
    /// ```
    /// use vitesse::prelude::*;
    ///
    /// let mut app = App::new();
    /// app.pre_handler(|req: &mut Request| {
    ///     if req.route() == Some("/admin/:page") && req.header("x-admin").is_none() {
    ///         return Err(Error::forbidden("admins only"));
    ///     }
    ///     Ok(())
    /// });
    /// app.get("/admin/:page", |_| async { "admin" });
    /// ```
    pub fn pre_handler<F>(&mut self, hook: F) -> &mut Self
    where
        F: Fn(&mut Request) -> Result<(), Error> + Send + Sync + 'static,
    {
        self.pre_handler.push(Box::new(hook));
        self
    }

    /// A hook run on every response before it is sent (Fastify's
    /// `onSend`): add headers, change the status… It sees the responses of
    /// the middlewares, of the `404`s and of the errors (before
    /// [`App::on_error`] rewrites them: [`Response::error`] is still there).
    ///
    /// ```
    /// use vitesse::prelude::*;
    ///
    /// let mut app = App::new();
    /// app.on_response(|res: &mut Response| {
    ///     res.set_header("x-powered-by", "Vitesse");
    /// });
    /// ```
    pub fn on_response<F>(&mut self, hook: F) -> &mut Self
    where
        F: Fn(&mut Response) + Send + Sync + 'static,
    {
        self.on_response.push(Box::new(hook));
        self
    }

    /// Serves the OpenAPI 3.1 document of the application at `path`
    /// (`/openapi.json`), generated from its routes when it starts, and a
    /// Swagger UI page if `spec` asks for one. Describe the routes with
    /// [`doc`](App::doc). See [`openapi`](crate::openapi).
    ///
    /// ```
    /// use vitesse::prelude::*;
    /// use vitesse::openapi::{OpenApi, Operation};
    ///
    /// let mut app = App::new();
    /// app.get("/pets", |_| async { Json(vec!["Rex"]) })
    ///     .doc(Operation::new().summary("List the pets"));
    /// app.openapi("/openapi.json", OpenApi::new("Pets", "1.0.0").swagger_ui("/docs"));
    /// ```
    pub fn openapi(&mut self, path: &str, spec: crate::openapi::OpenApi) -> &mut Self {
        self.openapi = Some((path.to_owned(), spec));
        self
    }

    /// The OpenAPI document of the routes added so far, without serving
    /// it: to write it to a file, or check it in a test.
    pub fn openapi_document(&self, spec: &crate::openapi::OpenApi) -> serde_json::Value {
        crate::openapi::document(spec, &self.api_routes)
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

    /// The secret that signs cookies (`cookieParser(secret)` in Express),
    /// for [`Request::sign_cookie`], [`Request::signed_cookie`] and the
    /// session cookie. Needs the `session` feature. Keep it out of the code
    /// (an environment variable): whoever knows it can forge cookies.
    ///
    /// # Panics
    /// If the secret is shorter than 32 bytes.
    #[cfg(feature = "session")]
    #[track_caller]
    pub fn cookie_secret(&mut self, secret: impl AsRef<[u8]>) -> &mut Self {
        self.cookie_key = Some(crate::session::Key::new(secret.as_ref()));
        self
    }

    /// The directory of the HTML templates (`app.set('views', dir)` in
    /// Express), rendered with [`Request::render`]. Needs the `templates`
    /// feature: see [`templates`](crate::templates).
    #[cfg(feature = "templates")]
    pub fn views(&mut self, dir: impl Into<std::path::PathBuf>) -> &mut Self {
        self.templates = Some(crate::templates::from_dir(dir.into()));
        self
    }

    /// A minijinja environment of your own (filters, functions, globals,
    /// templates embedded in the binary…), rendered with
    /// [`Request::render`]. Needs the `templates` feature.
    ///
    /// ```
    /// use vitesse::prelude::*;
    /// use vitesse::templates::minijinja::Environment;
    ///
    /// let mut env = Environment::new();
    /// env.add_template("hello.html", "<h1>Hello {{ name|upper }}</h1>").unwrap();
    /// let mut app = App::new();
    /// app.templates(env);
    /// app.get("/hello/:name", |req: Request| async move {
    ///     req.render("hello.html", json!({ "name": req.param("name") }))
    /// });
    /// ```
    #[cfg(feature = "templates")]
    pub fn templates(&mut self, env: minijinja::Environment<'static>) -> &mut Self {
        self.templates = Some(env);
        self
    }

    /// Whether routes are case-sensitive (`true` by default, unlike Express):
    /// with `false`, `/Users` matches the route `/users`. Parameters keep
    /// the case of the request.
    pub fn case_sensitive(&mut self, sensitive: bool) -> &mut Self {
        self.tree.set_case_insensitive(!sensitive);
        self
    }

    /// Maximum size of a request body read into memory (1 MiB by default).
    pub fn body_limit(&mut self, bytes: usize) -> &mut Self {
        self.body_limit = bytes;
        self
    }

    /// Declares the reverse proxies in front of the application
    /// (`app.set('trust proxy', …)` in Express), so that [`Request::ip`],
    /// [`Request::protocol`], [`Request::secure`] and [`Request::hostname`]
    /// read `X-Forwarded-For`, `X-Forwarded-Proto` and `X-Forwarded-Host`.
    ///
    /// Only trust what your own proxies set: a client can send these headers
    /// too. See [`TrustProxy`] for the accepted values.
    ///
    /// ```
    /// use vitesse::prelude::*;
    ///
    /// let mut app = App::new();
    /// app.trust_proxy("loopback, 10.0.0.0/8"); // or `true`, or a number of hops
    /// app.get("/ip", |req: Request| async move {
    ///     format!("{:?} over {}", req.ip(), req.protocol())
    /// });
    /// ```
    pub fn trust_proxy(&mut self, trust: impl Into<TrustProxy>) -> &mut Self {
        self.trust_proxy = trust.into();
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

    /// How long an idle keep-alive connection stays open (60 s by default,
    /// actix-web's `keep_alive`). `Duration::ZERO` disables keep-alive: the
    /// connection is closed after each response (`Connection: close`).
    pub fn keep_alive(&mut self, timeout: Duration) -> &mut Self {
        self.config.keep_alive = timeout.min(crate::util::MAX_DELAY);
        self
    }

    /// Time given to a client to send a complete request head (30 s by
    /// default, actix-web's `client_request_timeout`): beyond it, `408
    /// Request Timeout` and the connection is closed. Protects against
    /// clients that open connections and send nothing (slowloris).
    pub fn header_timeout(&mut self, timeout: Duration) -> &mut Self {
        self.config.header_timeout =
            timeout.clamp(Duration::from_millis(20), crate::util::MAX_DELAY);
        self
    }

    /// Time given to in-flight requests to finish during a graceful
    /// shutdown (10 s by default, actix-web's `shutdown_timeout`).
    pub fn shutdown_timeout(&mut self, timeout: Duration) -> &mut Self {
        self.config.shutdown_timeout = timeout.min(crate::util::MAX_DELAY);
        self
    }

    /// Maximum number of open connections, per server thread (actix-web's
    /// `max_connections`): beyond it, new connections wait in the system's
    /// queue until one closes. Unlimited by default.
    pub fn max_connections(&mut self, max: usize) -> &mut Self {
        self.config.max_connections = max.max(1);
        self
    }

    /// Also serves the application over HTTP/3 (QUIC, UDP), next to
    /// HTTP/1.1, with [`App::run`], [`App::listen`] and [`App::bind`]:
    /// see [`http3`](crate::http3).
    #[cfg(feature = "http3")]
    pub fn http3(&mut self, config: crate::http3::Http3) -> &mut Self {
        self.http3 = Some(config);
        self
    }

    /// Serves HTTPS: every TCP connection speaks TLS (1.2 or 1.3), with
    /// HTTP/2 for the clients that offer it when the `http2` feature is on.
    /// See [`tls`](crate::tls).
    ///
    /// ```no_run
    /// # use vitesse::prelude::*;
    /// # fn main() -> std::io::Result<()> {
    /// use vitesse::tls::Tls;
    ///
    /// let mut app = App::new();
    /// app.get("/", |_| async { "Hello over HTTPS!" });
    /// app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?);
    /// app.run(443)
    /// # }
    /// ```
    #[cfg(feature = "tls")]
    pub fn tls(&mut self, config: crate::tls::Tls) -> &mut Self {
        self.tls = Some(config);
        self
    }

    /// Freezes the application. It then lives until the end of the program,
    /// so requests never have to touch a reference count.
    pub(crate) fn build(mut self) -> &'static AppService {
        if let Some((path, spec)) = self.openapi.take() {
            let document = Bytes::from(
                serde_json::to_vec_pretty(&crate::openapi::document(&spec, &self.api_routes))
                    .expect("a JSON value always serializes"),
            );
            if let Some(ui) = &spec.swagger_ui {
                let page = Bytes::from(crate::openapi::swagger_page(spec.title(), &path));
                self.get(ui, move |_| {
                    let page = page.clone();
                    async move { crate::response::Html(page) }
                });
            }
            self.get(&path, move |_| {
                let document = document.clone();
                async move {
                    Response::new()
                        .content_type("application/json")
                        .send(document)
                }
            });
        }
        let pre_handler: Option<Arc<[RequestHook]>> =
            (!self.pre_handler.is_empty()).then(|| std::mem::take(&mut self.pre_handler).into());
        if !self.params.is_empty() || pre_handler.is_some() {
            for methods in self.tree.values_mut() {
                for route in methods.routes_mut() {
                    if route.own {
                        if let Some(wrapped) =
                            crate::hooks::with_params(&self.params, &route.names, &route.handler)
                        {
                            route.handler = wrapped;
                        }
                    }
                    if let Some(hooks) = &pre_handler {
                        route.handler = Arc::new(PreHandler {
                            hooks: hooks.clone(),
                            handler: route.handler.clone(),
                        });
                    }
                }
            }
        }
        let fallback = self.fallback.unwrap_or_else(|| Arc::new(not_found));
        let tree: &'static Tree<MethodMap> = Box::leak(Box::new(self.tree));
        let core = Arc::new(Core { tree, fallback });
        let chains = build_scopes(self.scopes, &core);
        let mut scopes = Tree::new();
        scopes.set_case_insensitive(tree.is_case_insensitive());
        for (i, (prefix, _)) in chains.iter().enumerate() {
            // `/docs/:lang(en|fr)` couvre `/docs/:lang(en|fr)/*`, avec ses contraintes.
            let Ok(segments) = parse_pattern(prefix) else {
                continue;
            };
            for mut variant in variants(&segments) {
                variant.push(crate::tree::Segment::Wildcard("*"));
                scopes.entry(&variant, || i);
            }
        }
        let dispatcher: Arc<dyn Handler> = Arc::new(Dispatcher {
            scopes,
            chains: chains.into_iter().map(|(_, chain)| chain).collect(),
            core,
        });
        let mut middlewares = self.middlewares;
        if !self.on_request.is_empty() || !self.on_response.is_empty() {
            middlewares.insert(
                0,
                Arc::new(Hooks {
                    on_request: self.on_request.into(),
                    on_response: self.on_response.into(),
                }),
            );
        }
        if let Some(handler) = self.error_handler {
            middlewares.insert(0, Arc::new(ErrorHandler(handler)));
        }
        Box::leak(Box::new(AppService {
            chain: Chain {
                middlewares: middlewares.into(),
                endpoint: dispatcher,
            },
            config: self.config,
            shared: Shared {
                #[cfg(feature = "session")]
                cookie_key: self.cookie_key,
                #[cfg(feature = "templates")]
                templates: self.templates,
                routes: Some(tree),
                names: self
                    .names
                    .into_iter()
                    .map(|(name, pattern)| {
                        let template = UrlTemplate::new(&pattern)
                            .expect("route patterns are checked when they are added");
                        (name.into_boxed_str(), template)
                    })
                    .collect(),
                state: self.state,
                body_limit: self.body_limit,
                trust_proxy: self.trust_proxy,
            },
            #[cfg(feature = "http3")]
            http3: self.http3,
            #[cfg(feature = "http3")]
            alt_svc: std::sync::OnceLock::new(),
            #[cfg(feature = "tls")]
            tls: self.tls.map(|tls| tls.acceptor()),
        }))
    }

    /// Turns the application into a tower `Service`, to serve it with hyper,
    /// nest it in an axum router or test it with tower's `ServiceExt`. See
    /// [`tower`](crate::tower).
    #[cfg(feature = "tower")]
    pub fn into_service(self) -> crate::tower::AppService {
        crate::tower::AppService::new(self.build())
    }

    /// Opens the port without starting the server: handy to find out which
    /// port was picked when listening on port `0`.
    pub async fn bind(self, addr: impl ListenAddr) -> io::Result<Server> {
        let addrs = addr.socket_addrs()?;
        let app = self.build();
        // On port 0, HTTP/3 takes the UDP port of the TCP port the system picked, which may
        // be taken or reserved for UDP (Windows reserves ranges per protocol): pick again.
        let mut attempts = if addrs.iter().all(|a| a.port() == 0) {
            20
        } else {
            1
        };
        loop {
            let listener = server::bind(&addrs, false)?;
            match Server::new(app, tokio::net::TcpListener::from_std(listener)?) {
                Err(e)
                    if attempts > 1
                        && matches!(
                            e.kind(),
                            io::ErrorKind::AddrInUse | io::ErrorKind::PermissionDenied
                        ) =>
                {
                    attempts -= 1;
                }
                result => return result,
            }
        }
    }

    /// Opens a Unix domain socket instead of a TCP port (actix-web's
    /// `bind_uds`), without starting the server. A socket file left over by
    /// a previous run is replaced; it is removed again when the server
    /// stops. Not available on Windows.
    ///
    /// ```no_run
    /// # use vitesse::prelude::*;
    /// #[tokio::main]
    /// async fn main() -> std::io::Result<()> {
    ///     let mut app = App::new();
    ///     app.get("/", |_| async { "Hello over a Unix socket!" });
    ///     app.trust_proxy(true); // the proxy reports the client's address
    ///     app.listen_unix("/run/my-app/app.sock").await
    /// }
    /// ```
    #[cfg(unix)]
    pub async fn bind_unix(
        self,
        path: impl AsRef<std::path::Path>,
    ) -> io::Result<crate::server::UnixServer> {
        let path = path.as_ref();
        let listener = server::bind_unix(path)?;
        Ok(crate::server::UnixServer::new(
            self.build(),
            listener,
            path.to_path_buf(),
        ))
    }

    /// Serves the application on a Unix domain socket, in the current tokio
    /// runtime, until `Ctrl+C` or `SIGTERM` (see [`App::bind_unix`]).
    #[cfg(unix)]
    pub async fn listen_unix(self, path: impl AsRef<std::path::Path>) -> io::Result<()> {
        self.bind_unix(path).await?.run().await
    }

    /// Like [`App::listen_unix`], without needing `#[tokio::main]`: starts
    /// tokio's multi-threaded runtime ([`App::workers`] threads).
    #[cfg(unix)]
    pub fn run_unix(self, path: impl AsRef<std::path::Path>) -> io::Result<()> {
        let workers = self
            .workers
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()));
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(workers)
            .enable_all()
            .build()?
            .block_on(self.listen_unix(path))
    }

    /// Starts the server on the current tokio runtime (`app.listen(3000)`).
    ///
    /// Like [`App::run`], it stops gracefully on `Ctrl+C` or `SIGTERM`:
    /// in-flight requests get up to [`App::shutdown_timeout`] (10 s by
    /// default) to finish. [`App::workers`] and
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
    pub(crate) config: ServerConfig,
    pub(crate) shared: Shared,
    #[cfg(feature = "http3")]
    pub(crate) http3: Option<crate::http3::Http3>,
    /// The `alt-svc: …\r\n` line added to HTTP/1.1 responses once the
    /// HTTP/3 port is open.
    #[cfg(feature = "http3")]
    pub(crate) alt_svc: std::sync::OnceLock<Box<[u8]>>,
    /// Set by [`App::tls`]: TCP connections start with a TLS handshake.
    #[cfg(feature = "tls")]
    pub(crate) tls: Option<tokio_rustls::TlsAcceptor>,
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
    /// The mount prefixes of the routers (`prefix/*`), pointing into `chains`.
    scopes: Tree<usize>,
    /// For each prefix, the chain of the middlewares that cover it.
    chains: Box<[Chain]>,
}

impl Handler for Dispatcher {
    #[inline]
    fn call(&'static self, mut req: Request) -> BoxFuture<Response> {
        if let Some(route) = req
            .find_route(self.core.tree)
            .and_then(|methods| methods.find(req.method()))
        {
            req.set_route(route);
            return route.handler.call(req);
        }
        // Aucune route ne répond : les middlewares des routeurs montés sur ce
        // préfixe passent d'abord, puis le 404, le 405 ou l'OPTIONS automatique.
        let mut captures = Vec::new();
        match self.scopes.find(req.path(), &mut captures) {
            Some(&i) => Next::new(&self.chains[i]).run(req),
            None => self.core.call(req),
        }
    }
}

/// Pour chaque préfixe, la chaîne de tous les middlewares qui le couvrent :
/// les préfixes les plus courts (routeurs extérieurs) d'abord, puis dans
/// l'ordre de montage.
/// Le fallback est celui du routeur le plus profond qui couvre le préfixe.
fn build_scopes(scopes: Vec<Scope>, core: &Arc<Core>) -> Vec<(Box<str>, Chain)> {
    let depth = |p: &str| p.split('/').filter(|s| !s.is_empty()).count();
    let mut ordered: Vec<(usize, Scope)> = scopes.into_iter().enumerate().collect();
    ordered.sort_by_key(|(i, scope)| (depth(&scope.prefix), *i));
    let mut prefixes: Vec<&str> = ordered.iter().map(|(_, s)| s.prefix.as_str()).collect();
    // Le préfixe le plus précis d'abord : le plus de segments.
    prefixes.sort_by_key(|p| std::cmp::Reverse((depth(p), p.len())));
    prefixes.dedup();
    prefixes
        .into_iter()
        .map(|prefix| {
            let covering = || {
                ordered
                    .iter()
                    .map(|(_, scope)| scope)
                    .filter(|scope| covers(&scope.prefix, prefix))
            };
            let middlewares: Vec<Arc<dyn Middleware>> = covering()
                .flat_map(|scope| scope.middlewares.iter().cloned())
                .collect();
            let endpoint: Arc<dyn Handler> =
                match covering().rev().find_map(|scope| scope.fallback.clone()) {
                    Some(fallback) => Arc::new(ScopedFallback {
                        core: core.clone(),
                        fallback,
                    }),
                    None => core.clone(),
                };
            let chain = Chain {
                middlewares: middlewares.into(),
                endpoint,
            };
            (prefix.into(), chain)
        })
        .collect()
}

/// Routing without the routers' middlewares: the route, or else the `405` /
/// automatic `OPTIONS`, or else the fallback (`404`). A router middleware may
/// have rewritten the path, hence the new lookup.
struct Core {
    tree: &'static Tree<MethodMap>,
    fallback: Arc<dyn Handler>,
}

impl Handler for Core {
    fn call(&'static self, req: Request) -> BoxFuture<Response> {
        self.route(req, &*self.fallback)
    }
}

/// Routing under a router that has its own fallback.
struct ScopedFallback {
    core: Arc<Core>,
    fallback: Arc<dyn Handler>,
}

impl Handler for ScopedFallback {
    fn call(&'static self, req: Request) -> BoxFuture<Response> {
        let core: &'static Core = &self.core;
        core.route(req, &*self.fallback)
    }
}

impl Core {
    fn route(
        &'static self,
        mut req: Request,
        fallback: &'static dyn Handler,
    ) -> BoxFuture<Response> {
        let Some(methods) = req.find_route(self.tree) else {
            return fallback.call(req);
        };
        match methods.find(req.method()) {
            Some(route) => {
                req.set_route(route);
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
