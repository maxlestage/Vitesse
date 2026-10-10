//! # Vitesse ⚡
//!
//! A minimalist web framework **in the style of Express.js**, written in
//! native Rust and built for speed. Vitesse runs on its own HTTP/1.1 engine,
//! on top of [tokio](https://tokio.rs).
//!
//! ## Quick start
//!
//! ```no_run
//! use vitesse::prelude::*;
//!
//! fn main() -> std::io::Result<()> {
//!     let mut app = App::new();
//!
//!     app.middleware(middleware::logger());
//!
//!     app.get("/", |_| async { "Hello World!" });
//!
//!     app.get("/users/:id", |req: Request| async move {
//!         let id: u32 = req.param_as("id")?;
//!         Ok::<_, Error>(Json(json!({ "id": id, "name": "Ada" })))
//!     });
//!
//!     app.post("/users", |req: Request| async move {
//!         let user: vitesse::serde_json::Value = req.json().await?;
//!         Ok::<_, Error>((201, Json(user)))
//!     });
//!
//!     app.run(3000)
//! }
//! ```
//!
//! ## From Express to Vitesse
//!
//! | Express | Vitesse |
//! |---|---|
//! | `const app = express()` | `let mut app = App::new();` |
//! | `app.get('/u/:id', (req, res) => …)` | `app.get("/u/:id", \|req\| async move { … })` |
//! | `app.use(fn)` | `app.middleware(fn)` |
//! | `app.use('/api', router)` | `app.mount("/api", router)` |
//! | `express.Router()` | `Router::new()` |
//! | `express.static('public')` | `app.static_dir("/", "public")` |
//! | `next()` | `next.run(req).await` |
//! | `req.params.id` | `req.param("id")` |
//! | `req.query.q` | `req.query("q")` |
//! | `req.body` (`express.json()`) | `req.json::<T>().await?` |
//! | `res.status(201).json(obj)` | `res::status(201).json(obj)` or `(201, Json(obj))` |
//! | `res.send('text')` | return `"text"` |
//! | `res.redirect('/login')` | `Redirect::to("/login")` |
//! | `app.listen(3000)` | `app.run(3000)` or `app.listen(3000).await` |
//!
//! ## Why it's fast
//!
//! - **Its own HTTP/1.1 engine** on [tokio](https://tokio.rs): request heads
//!   are parsed by `httparse` (SIMD) without copying, headers and URI are
//!   only built when asked for, responses are serialized straight into the
//!   write buffer, and a whole batch of pipelined requests is answered with
//!   a single system call.
//! - **One thread per core** ([`App::run`] on Linux): each core has its own
//!   event loop and its own `SO_REUSEPORT` socket, with no synchronization.
//! - **A regex-free router**: a tree of path segments, with no allocation
//!   for routes without parameters.
//! - **Zero shared atomic counters per request**: the application is frozen
//!   at startup (`&'static`).
//! - **Few allocations**: a request travels through middlewares and
//!   handlers by moving a single pointer, response header maps are recycled,
//!   and the body is only read if the handler asks for it.
//!
//! ## Guide
//!
//! This page is the API reference. The full guide is available on the
//! [Vitesse website](https://maxlestage.github.io/Vitesse/#/docs) and in the
//! [`docs/` folder of the repository](https://github.com/maxlestage/Vitesse/tree/master/docs/en),
//! in English, French and Spanish.

#![doc(
    html_logo_url = "https://maxlestage.github.io/Vitesse/assets/favicon.svg",
    html_favicon_url = "https://maxlestage.github.io/Vitesse/assets/favicon.svg"
)]
#![warn(missing_docs)]

mod app;
mod body;
mod error;
mod handler;
mod http1;
mod request;
mod response;
mod router;
mod server;
mod static_files;
mod tree;
mod upgrade;
mod util;

#[cfg(feature = "http3")]
pub mod http3;
pub mod middleware;
pub mod test;
#[cfg(feature = "ws")]
pub mod ws;

pub use app::{App, DEFAULT_BODY_LIMIT};
pub use body::{Body, BoxError};
pub use error::{Error, Result};
pub use handler::{BoxFuture, Chained, Handler, HandlerExt, Middleware, Next};
pub use request::Request;
pub use response::{
    Cookie, Html, IntoResponse, IntoStatus, Json, Redirect, Response, SameSite, res,
};
pub use router::Router;
pub use server::{ListenAddr, Server};
pub use static_files::ServeDir;

pub use bytes::Bytes;
pub use http::{self, HeaderMap, Method, StatusCode, header};
pub use serde_json::{self, json};
pub use tokio;

/// Everything you need to write an application: `use vitesse::prelude::*;`
pub mod prelude {
    pub use crate::middleware;
    #[cfg(feature = "ws")]
    pub use crate::ws;
    pub use crate::{
        App, Body, Cookie, Error, HandlerExt, Html, IntoResponse, Json, Method, Next, Redirect,
        Request, Response, Router, ServeDir, StatusCode, json, res,
    };
}
