//! Le site de présentation de Vitesse, en français, anglais et espagnol, avec sa
//! documentation.
//!
//! - côté serveur (natif) : une application Vitesse qui rend chaque page avec
//!   [active](https://github.com/maxlestage/Active), sert les fichiers statiques, le
//!   manifeste et le service worker, et exporte tout le site en fichiers statiques
//!   pour GitHub Pages ;
//! - côté navigateur (wasm32) : les îlots interactifs, hydratés par active, et les
//!   animations de la page.

pub mod catalog;
pub mod data;
pub mod highlight;
pub mod i18n;
pub mod icons;
pub mod islands;
pub mod labels;
pub mod routes;
pub mod search;

#[cfg(not(target_arch = "wasm32"))]
pub mod server;

#[cfg(target_arch = "wasm32")]
mod client;
