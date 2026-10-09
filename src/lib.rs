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
//! - Le socle HTTP est [hyper](https://hyper.rs) sur [tokio](https://tokio.rs),
//!   compilé en code natif et exécuté sur tous les cœurs.
//! - Le routeur est un arbre de segments : pas d'expression régulière, pas
//!   d'allocation pour les routes sans paramètre.
//! - Une seule allocation par handler pour son `Future` ; les paniques sont
//!   rattrapées sans coût supplémentaire.
//! - L'application est figée au démarrage (`&'static`) : aucun compteur de
//!   références atomique n'est touché par requête.
//! - Le corps n'est lu que si le handler le demande.
//! - En option, [`App::thread_per_core`] donne à chaque cœur sa propre boucle
//!   d'événements et son propre socket (`SO_REUSEPORT`, Linux).

#![warn(missing_docs)]

mod app;
mod body;
mod error;
mod handler;
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
