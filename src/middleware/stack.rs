//! Middleware stacks and named middleware groups (Laravel's `web` and
//! `api` groups).

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, OnceLock};

use crate::error::Error;
use crate::handler::{BoxFuture, Chain, Handler, Middleware, Next, RawNext};
use crate::request::Request;
use crate::response::{IntoResponse, Response};

/// Several middlewares used as one, in order: build it once, use it on the
/// application, on routers, groups and single routes, or name it with
/// [`App::middleware_group`](crate::App::middleware_group).
///
/// ```
/// use std::time::Duration;
/// use vitesse::prelude::*;
/// use vitesse::middleware::Stack;
///
/// let api = Stack::new()
///     .with(middleware::request_id())
///     .with(middleware::rate_limit(60, Duration::from_secs(60)));
///
/// let mut app = App::new();
/// app.group("/api", |r| {
///     r.middleware(api.clone());
///     r.get("/users", |_| async { Json(vec!["ada"]) });
/// });
/// app.post("/webhook", (|_| async { "ok" }).with(api));
/// ```
#[derive(Default)]
pub struct Stack {
    middlewares: Vec<Arc<dyn Middleware>>,
    /// The chain of the middlewares, ending by going back to the outer
    /// chain, built on the first request.
    chain: OnceLock<Chain>,
}

impl Clone for Stack {
    fn clone(&self) -> Self {
        Stack {
            middlewares: self.middlewares.clone(),
            chain: OnceLock::new(),
        }
    }
}

impl fmt::Debug for Stack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Stack")
            .field("middlewares", &self.middlewares.len())
            .finish()
    }
}

impl Stack {
    /// An empty stack.
    pub fn new() -> Self {
        Stack::default()
    }

    /// Adds a middleware, run after those already in the stack.
    pub fn with<M: Middleware>(mut self, middleware: M) -> Self {
        self.middlewares.push(Arc::new(middleware));
        self
    }

    /// Adds every middleware of another stack, after those already there.
    pub fn extend(mut self, other: &Stack) -> Self {
        self.middlewares.extend(other.middlewares.iter().cloned());
        self
    }

    /// The number of middlewares.
    pub fn len(&self) -> usize {
        self.middlewares.len()
    }

    /// The stack has no middleware.
    pub fn is_empty(&self) -> bool {
        self.middlewares.is_empty()
    }
}

/// Where the request goes once through the stack: back to the chain the
/// stack was called from.
#[derive(Clone, Default)]
struct Continuations(Vec<RawNext>);

struct Resume;

impl Handler for Resume {
    fn call(&'static self, mut req: Request) -> BoxFuture<Response> {
        let next = req
            .extensions_mut()
            .get_mut::<Continuations>()
            .and_then(|c| c.0.pop());
        match next {
            Some(next) => next.into_next().run(req),
            None => Box::pin(std::future::ready(
                Error::internal("middleware stack lost its continuation").into_response(),
            )),
        }
    }
}

impl Middleware for Stack {
    fn handle(&'static self, mut req: Request, next: Next) -> BoxFuture<Response> {
        if self.middlewares.is_empty() {
            return next.run(req);
        }
        let chain = self.chain.get_or_init(|| Chain {
            middlewares: self.middlewares.clone().into(),
            endpoint: Arc::new(Resume),
        });
        let extensions = req.extensions_mut();
        match extensions.get_mut::<Continuations>() {
            Some(continuations) => continuations.0.push(next.into_raw()),
            None => {
                extensions.insert(Continuations(vec![next.into_raw()]));
            }
        }
        Next::new(chain).run(req)
    }
}

/// The named groups of the application, kept in its state.
pub(crate) struct Groups(pub(crate) HashMap<String, Stack>);

/// The middleware group named `name`, defined with
/// [`App::middleware_group`](crate::App::middleware_group). The name is
/// looked up on the first request (then remembered); an unknown name
/// answers `500` and is logged.
///
/// ```
/// use vitesse::prelude::*;
/// use vitesse::middleware::Stack;
///
/// // In a module that has no access to the application:
/// fn admin_routes(r: &mut Router) {
///     r.middleware(middleware::group("web"));
///     r.get("/dashboard", |_| async { "dashboard" });
/// }
///
/// let mut app = App::new();
/// app.middleware_group("web", Stack::new().with(middleware::helmet()));
/// app.group("/admin", admin_routes);
/// ```
pub fn group(name: &str) -> Group {
    Group {
        name: name.into(),
        stack: OnceLock::new(),
    }
}

/// The middleware returned by [`group`].
pub struct Group {
    name: Box<str>,
    stack: OnceLock<Option<&'static Stack>>,
}

impl fmt::Debug for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Group").field("name", &self.name).finish()
    }
}

impl Middleware for Group {
    fn handle(&'static self, req: Request, next: Next) -> BoxFuture<Response> {
        let stack = self.stack.get_or_init(|| {
            req.try_state::<Groups>()
                .and_then(|groups| groups.0.get(&*self.name))
        });
        match stack {
            Some(stack) => stack.handle(req, next),
            None => {
                eprintln!(
                    "[vitesse] unknown middleware group '{}': define it with app.middleware_group",
                    self.name
                );
                Box::pin(std::future::ready(
                    Error::internal("Internal Server Error").into_response(),
                ))
            }
        }
    }
}
