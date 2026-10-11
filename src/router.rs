//! Routers (`express.Router()`), to be mounted on the application.

use std::sync::Arc;

use http::Method;

use crate::handler::{Chained, Handler, Middleware};
use crate::tree::parse_pattern;

/// A registered route.
pub(crate) struct Route {
    pub(crate) handler: Arc<dyn Handler>,
    pub(crate) names: Box<[Box<str>]>,
    /// The full pattern, mount prefix included (`/api/users/:id`).
    pub(crate) pattern: Box<str>,
}

/// The handlers of a single path, by HTTP method.
#[derive(Default)]
pub(crate) struct MethodMap {
    get: Option<Route>,
    post: Option<Route>,
    put: Option<Route>,
    patch: Option<Route>,
    delete: Option<Route>,
    head: Option<Route>,
    options: Option<Route>,
    other: Vec<(Method, Route)>,
    any: Option<Route>,
}

impl MethodMap {
    /// Registers a route; returns `Err` if it already exists.
    pub(crate) fn insert(&mut self, method: Option<Method>, route: Route) -> Result<(), ()> {
        let slot = match method {
            None => &mut self.any,
            Some(Method::GET) => &mut self.get,
            Some(Method::POST) => &mut self.post,
            Some(Method::PUT) => &mut self.put,
            Some(Method::PATCH) => &mut self.patch,
            Some(Method::DELETE) => &mut self.delete,
            Some(Method::HEAD) => &mut self.head,
            Some(Method::OPTIONS) => &mut self.options,
            Some(other) => {
                if self.other.iter().any(|(m, _)| *m == other) {
                    return Err(());
                }
                self.other.push((other, route));
                return Ok(());
            }
        };
        if slot.is_some() {
            return Err(());
        }
        *slot = Some(route);
        Ok(())
    }

    /// The handler for this method (`HEAD` falls back to `GET`).
    #[inline]
    pub(crate) fn find(&self, method: &Method) -> Option<&Route> {
        let route = match *method {
            Method::GET => self.get.as_ref(),
            Method::POST => self.post.as_ref(),
            Method::PUT => self.put.as_ref(),
            Method::PATCH => self.patch.as_ref(),
            Method::DELETE => self.delete.as_ref(),
            Method::HEAD => self.head.as_ref().or(self.get.as_ref()),
            Method::OPTIONS => self.options.as_ref(),
            _ => self.other.iter().find(|(m, _)| m == method).map(|(_, r)| r),
        };
        route.or(self.any.as_ref())
    }

    /// The value of the `Allow` header.
    pub(crate) fn allow(&self) -> String {
        if self.any.is_some() {
            return "GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS".into();
        }
        let mut methods: Vec<&str> = Vec::new();
        if self.get.is_some() {
            methods.push("GET");
        }
        if self.head.is_some() || self.get.is_some() {
            methods.push("HEAD");
        }
        for (m, name) in [
            (&self.post, "POST"),
            (&self.put, "PUT"),
            (&self.patch, "PATCH"),
            (&self.delete, "DELETE"),
        ] {
            if m.is_some() {
                methods.push(name);
            }
        }
        methods.push("OPTIONS");
        for (m, _) in &self.other {
            methods.push(m.as_str());
        }
        methods.join(", ")
    }
}

/// Joins a mount prefix and a route path.
pub(crate) fn join_paths(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_end_matches('/');
    let mut out = String::with_capacity(prefix.len() + path.len() + 1);
    if !prefix.is_empty() && !prefix.starts_with('/') {
        out.push('/');
    }
    out.push_str(prefix);
    if path != "/" || out.is_empty() {
        out.push_str(path);
    }
    out
}

/// Generates the routing methods shared by [`App`](crate::App) and [`Router`].
macro_rules! routing_methods {
    () => {
        /// Adds a `GET` route (`app.get(path, handler)`).
        #[track_caller]
        pub fn get<H: $crate::Handler>(&mut self, path: &str, handler: H) -> &mut Self {
            self.add_route(
                Some(::http::Method::GET),
                path,
                ::std::sync::Arc::new(handler),
            );
            self
        }

        /// Adds a `POST` route.
        #[track_caller]
        pub fn post<H: $crate::Handler>(&mut self, path: &str, handler: H) -> &mut Self {
            self.add_route(
                Some(::http::Method::POST),
                path,
                ::std::sync::Arc::new(handler),
            );
            self
        }

        /// Adds a `PUT` route.
        #[track_caller]
        pub fn put<H: $crate::Handler>(&mut self, path: &str, handler: H) -> &mut Self {
            self.add_route(
                Some(::http::Method::PUT),
                path,
                ::std::sync::Arc::new(handler),
            );
            self
        }

        /// Adds a `PATCH` route.
        #[track_caller]
        pub fn patch<H: $crate::Handler>(&mut self, path: &str, handler: H) -> &mut Self {
            self.add_route(
                Some(::http::Method::PATCH),
                path,
                ::std::sync::Arc::new(handler),
            );
            self
        }

        /// Adds a `DELETE` route.
        #[track_caller]
        pub fn delete<H: $crate::Handler>(&mut self, path: &str, handler: H) -> &mut Self {
            self.add_route(
                Some(::http::Method::DELETE),
                path,
                ::std::sync::Arc::new(handler),
            );
            self
        }

        /// Adds a `HEAD` route (by default, `HEAD` already uses the `GET`
        /// route).
        #[track_caller]
        pub fn head<H: $crate::Handler>(&mut self, path: &str, handler: H) -> &mut Self {
            self.add_route(
                Some(::http::Method::HEAD),
                path,
                ::std::sync::Arc::new(handler),
            );
            self
        }

        /// Adds an `OPTIONS` route (by default, `OPTIONS` answers `204` with
        /// `Allow`).
        #[track_caller]
        pub fn options<H: $crate::Handler>(&mut self, path: &str, handler: H) -> &mut Self {
            self.add_route(
                Some(::http::Method::OPTIONS),
                path,
                ::std::sync::Arc::new(handler),
            );
            self
        }

        /// Adds a route for every method (`app.all(path, handler)`).
        #[track_caller]
        pub fn all<H: $crate::Handler>(&mut self, path: &str, handler: H) -> &mut Self {
            self.add_route(None, path, ::std::sync::Arc::new(handler));
            self
        }

        /// Adds a WebSocket route (`express-ws`'s `app.ws(path, handler)`):
        /// `handler` receives the request and the open [`WebSocket`](crate::ws::WebSocket).
        ///
        /// A request that is not a WebSocket handshake gets
        /// `426 Upgrade Required`. For subprotocols or a size limit, use
        /// [`ws::Upgrade`](crate::ws::Upgrade) in a `GET` route.
        ///
        /// ```no_run
        /// # use vitesse::prelude::*;
        /// let mut app = App::new();
        /// app.ws("/chat/:room", |req, mut socket| async move {
        ///     let room = req.param("room").unwrap_or("lobby").to_owned();
        ///     let _ = socket.send(format!("welcome to {room}")).await;
        ///     while let Some(Ok(message)) = socket.recv().await {
        ///         if socket.send(message).await.is_err() {
        ///             break;
        ///         }
        ///     }
        /// });
        /// ```
        #[cfg(feature = "ws")]
        #[track_caller]
        pub fn ws<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
        where
            F: Fn($crate::Request, $crate::ws::WebSocket) -> Fut + Send + Sync + 'static,
            Fut: ::std::future::Future<Output = ()> + Send + 'static,
        {
            let handler = ::std::sync::Arc::new(handler);
            self.get(path, move |req: $crate::Request| {
                let handler = handler.clone();
                async move {
                    match $crate::ws::Upgrade::new(&req) {
                        Ok(upgrade) => {
                            upgrade.on_upgrade(req, move |req, socket| handler(req, socket))
                        }
                        Err(error) => $crate::IntoResponse::into_response(error),
                    }
                }
            })
        }

        /// Adds a route for an arbitrary method.
        #[track_caller]
        pub fn route<H: $crate::Handler>(
            &mut self,
            method: ::http::Method,
            path: &str,
            handler: H,
        ) -> &mut Self {
            self.add_route(Some(method), path, ::std::sync::Arc::new(handler));
            self
        }

        /// Mounts a [`Router`](crate::Router) under a prefix
        /// (`app.use('/api', router)`).
        ///
        /// Its middlewares run for its own routes, and, as in Express, for
        /// every other request under the prefix that no route answers (the
        /// `404`, `405` and automatic `OPTIONS` responses): a CORS or logging
        /// middleware on the router covers its whole prefix. So does its
        /// [`fallback`](crate::Router::fallback), if it has one.
        #[track_caller]
        pub fn mount(&mut self, prefix: &str, router: $crate::Router) -> &mut Self {
            let (routes, scopes, names) = router.into_parts();
            for (method, path, handler) in routes {
                self.add_route(method, &$crate::router::join_paths(prefix, &path), handler);
            }
            for scope in scopes {
                self.scopes.push(scope.under(prefix));
            }
            for (name, path) in names {
                self.add_name(&name, $crate::router::join_paths(prefix, &path));
            }
            self.last_route = None;
            self
        }

        /// Names the route added just before, to build its URL with
        /// [`Request::path_for`](crate::Request::path_for) and
        /// [`Request::url_for`](crate::Request::url_for) (actix-web's named
        /// resources). Names are global to the application, mounted routers
        /// included.
        ///
        /// ```
        /// use vitesse::prelude::*;
        ///
        /// let mut app = App::new();
        /// app.get("/users/:id", |req: Request| async move {
        ///     req.param("id").unwrap_or("").to_owned()
        /// })
        /// .name("user");
        /// app.post("/users", |req: Request| async move {
        ///     let link = req.path_for("user", &[&42])?; // "/users/42"
        ///     Ok::<_, Error>(Redirect::see_other(link))
        /// });
        /// ```
        ///
        /// # Panics
        /// If no route was added before, or if the name is already used.
        #[track_caller]
        pub fn name(&mut self, name: &str) -> &mut Self {
            let Some(pattern) = self.last_route.take() else {
                panic!("`name(\"{name}\")` must follow the route it names");
            };
            self.add_name(name, pattern);
            self
        }

        /// Serves the files of a directory under a prefix
        /// (`app.use('/static', express.static('public'))`).
        ///
        /// For more options, mount a [`ServeDir`](crate::ServeDir) with
        /// [`serve_dir`](Self::serve_dir).
        #[track_caller]
        pub fn static_dir(
            &mut self,
            prefix: &str,
            dir: impl Into<::std::path::PathBuf>,
        ) -> &mut Self {
            self.serve_dir(prefix, $crate::ServeDir::new(dir))
        }

        /// Serves the files of a configured [`ServeDir`](crate::ServeDir).
        #[track_caller]
        pub fn serve_dir(&mut self, prefix: &str, dir: $crate::ServeDir) -> &mut Self {
            let path = $crate::router::join_paths(prefix, "/*");
            let handler: ::std::sync::Arc<dyn $crate::Handler> = ::std::sync::Arc::new(dir);
            self.add_route(Some(::http::Method::GET), &path, handler.clone());
            self.add_route(Some(::http::Method::HEAD), &path, handler);
            self
        }
    };
}

pub(crate) use routing_methods;

/// A group of routes with its own middlewares (`express.Router()`).
///
/// ```
/// use vitesse::prelude::*;
///
/// let mut api = Router::new();
/// api.middleware(|req: Request, next: Next| async move {
///     next.run(req).await.header("x-api-version", "1")
/// });
/// api.get("/users", |_| async { Json(vec!["alice", "bob"]) });
/// api.get("/users/:id", |req: Request| async move {
///     format!("user {}", req.param("id").unwrap())
/// });
///
/// let mut app = App::new();
/// app.mount("/api/v1", api); // GET /api/v1/users, GET /api/v1/users/:id
/// ```
#[derive(Default)]
pub struct Router {
    routes: Vec<(Option<Method>, String, Arc<dyn Handler>)>,
    middlewares: Vec<Arc<dyn Middleware>>,
    fallback: Option<Arc<dyn Handler>>,
    /// Middlewares of the routers mounted inside this one, by prefix.
    pub(crate) scopes: Vec<Scope>,
    /// Named routes: (name, pattern).
    names: Vec<(String, String)>,
    /// The pattern of the last route added, for [`Router::name`].
    last_route: Option<String>,
}

/// The middlewares and the fallback of a mounted router, with its prefix:
/// they also apply to the requests under that prefix that no route answers.
pub(crate) struct Scope {
    pub(crate) prefix: String,
    pub(crate) middlewares: Vec<Arc<dyn Middleware>>,
    pub(crate) fallback: Option<Arc<dyn Handler>>,
}

impl Scope {
    /// The same scope, mounted under `prefix`.
    pub(crate) fn under(mut self, prefix: &str) -> Scope {
        self.prefix = join_paths(prefix, &self.prefix);
        self
    }
}

impl Router {
    /// Creates an empty router.
    pub fn new() -> Self {
        Router::default()
    }

    routing_methods!();

    /// Adds a middleware to every route of this router.
    pub fn middleware<M: Middleware>(&mut self, middleware: M) -> &mut Self {
        self.middlewares.push(Arc::new(middleware));
        self
    }

    /// Sets the handler called when no route of this router matches a path
    /// under its prefix (the application's [`fallback`](crate::App::fallback)
    /// otherwise), after the router's middlewares. A path that exists with
    /// another method still gets `405`.
    ///
    /// ```
    /// use vitesse::prelude::*;
    ///
    /// let mut api = Router::new();
    /// api.get("/users", |_| async { Json(vec!["ada"]) });
    /// api.fallback(|req: Request| async move {
    ///     (404, Json(json!({ "error": format!("no API route for {}", req.path()) })))
    /// });
    ///
    /// let mut app = App::new();
    /// app.mount("/api", api); // /api/nope: JSON 404; /nope: the app's 404
    /// ```
    pub fn fallback<H: Handler>(&mut self, handler: H) -> &mut Self {
        self.fallback = Some(Arc::new(handler));
        self
    }

    #[track_caller]
    fn add_name(&mut self, name: &str, pattern: String) {
        if self.names.iter().any(|(n, _)| n == name) {
            panic!("duplicate route name: '{name}' is already used");
        }
        self.names.push((name.to_owned(), pattern));
    }

    #[track_caller]
    fn add_route(&mut self, method: Option<Method>, path: &str, handler: Arc<dyn Handler>) {
        if let Err(e) = parse_pattern(path) {
            panic!("invalid route: {e}");
        }
        self.last_route = Some(path.to_owned());
        self.routes.push((method, path.to_owned(), handler));
    }

    /// The routes, wrapped in the router's middlewares, and the scopes
    /// (this router's own one first, at `/`).
    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Vec<(Option<Method>, String, Arc<dyn Handler>)>,
        Vec<Scope>,
        Vec<(String, String)>,
    ) {
        let Router {
            routes,
            middlewares,
            fallback,
            scopes: inner,
            names,
            last_route: _,
        } = self;
        if middlewares.is_empty() && fallback.is_none() {
            return (routes, inner, names);
        }
        let routes = if middlewares.is_empty() {
            routes
        } else {
            routes
                .into_iter()
                .map(|(method, path, handler)| {
                    let chained: Arc<dyn Handler> =
                        Arc::new(Chained::new(middlewares.clone(), handler));
                    (method, path, chained)
                })
                .collect()
        };
        let mut scopes = vec![Scope {
            prefix: "/".to_owned(),
            middlewares,
            fallback,
        }];
        scopes.extend(inner);
        (routes, scopes, names)
    }
}

#[cfg(test)]
mod tests {
    use super::join_paths;

    #[test]
    fn join() {
        assert_eq!(join_paths("/api", "/users"), "/api/users");
        assert_eq!(join_paths("/api/", "/users"), "/api/users");
        assert_eq!(join_paths("api", "/users"), "/api/users");
        assert_eq!(join_paths("/api", "/"), "/api");
        assert_eq!(join_paths("/", "/"), "/");
        assert_eq!(join_paths("", "/x"), "/x");
        assert_eq!(join_paths("/", "/*"), "/*");
    }
}
