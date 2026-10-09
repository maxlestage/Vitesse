//! # Vitesse ⚡
//!
//! Un framework web minimaliste **à la Express.js**, en Rust natif, pensé
//! pour la vitesse.
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
//!         let user: serde_json::Value = req.json().await?;
//!         Ok::<_, Error>((201, Json(user)))
//!     });
//!
//!     app.run(3000)
//! }
//! ```
//!
//! ## Correspondance avec Express
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
//! | `res.status(201).json(obj)` | `res::status(201).json(obj)` ou `(201, Json(obj))` |
//! | `res.send('texte')` | renvoyer `"texte"` |
//! | `res.redirect('/login')` | `Redirect::to("/login")` |
//! | `app.listen(3000)` | `app.run(3000)` ou `app.listen(3000).await` |
//!
//! ## Pourquoi c'est rapide
//!
//! - **Un moteur HTTP/1.1 maison** sur [tokio](https://tokio.rs) : têtes
//!   analysées par `httparse` (SIMD) sans copie, en-têtes et URI construits
//!   seulement si on les demande, réponses sérialisées directement dans le
//!   tampon d'écriture, et un seul appel système pour tout un lot de
//!   requêtes « pipelinées ».
//! - **Un thread par cœur** ([`App::run`] sous Linux) : chaque cœur a sa
//!   boucle d'événements et son socket `SO_REUSEPORT`, sans synchronisation.
//! - **Un routeur sans regex** : arbre de segments, aucune allocation pour
//!   les routes sans paramètre.
//! - **Zéro compteur atomique partagé par requête** : l'application est
//!   figée au démarrage (`&'static`).
//! - **Peu d'allocations** : la requête traverse middlewares et handlers en
//!   ne déplaçant qu'un pointeur, les tables d'en-têtes des réponses sont
//!   recyclées, et le corps n'est lu que si le handler le demande.

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
mod util;

pub mod middleware;
pub mod test;

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

/// Tout ce qu'il faut pour écrire une application : `use vitesse::prelude::*;`
pub mod prelude {
    pub use crate::middleware;
    pub use crate::{
        App, Body, Cookie, Error, HandlerExt, Html, IntoResponse, Json, Method, Next, Redirect,
        Request, Response, Router, ServeDir, StatusCode, json, res,
    };
}
