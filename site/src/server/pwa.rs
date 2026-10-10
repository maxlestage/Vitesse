//! La PWA : le site s'installe sur l'écran d'accueil d'un téléphone, s'ouvre en
//! plein écran comme une application et reste lisible hors ligne.
//!
//! - un manifeste par langue (`/{lang}/manifest.webmanifest`, et
//!   `/manifest.webmanifest` en anglais) : l'application installée depuis une page
//!   française s'ouvre en français ; tous ont le même `id`, c'est la même
//!   application ;
//! - un service worker (`/sw.js`), généré par `active::pwa` : les pages passent
//!   d'abord par le réseau, le reste vient du cache ; les accueils, les sommaires de
//!   la documentation, la feuille de style, le code du navigateur, les icônes et les
//!   polices sont en cache dès la première visite ; `/api/` ne l'est jamais.

use active::pwa::{Manifest, ServiceWorker};

use super::Site;
use crate::i18n::Lang;
use crate::routes::{base, docs_href, home_href, link};

/// La couleur du fond du site (`--bg`).
pub const THEME: &str = "#07070b";

/// L'adresse du manifeste d'une langue.
pub fn manifest_href(lang: Lang) -> String {
    link(&format!("/{}/manifest.webmanifest", lang.code()))
}

pub fn manifest(lang: Lang) -> Manifest {
    let b = base();
    Manifest::new("Vitesse")
        .short_name("Vitesse")
        .description(lang.texts().meta_description)
        .id(format!("{b}/"))
        .start_url(home_href(lang, ""))
        .scope(format!("{b}/"))
        .theme_color(THEME)
        .background_color(THEME)
        .lang(lang.code())
        .categories(["developer", "education"])
        .icon(format!("{b}/assets/icon-192.png"), "192x192", "image/png")
        .icon(format!("{b}/assets/icon-512.png"), "512x512", "image/png")
        .maskable_icon(
            format!("{b}/assets/icon-maskable-512.png"),
            "512x512",
            "image/png",
        )
        .icon(format!("{b}/assets/favicon.svg"), "any", "image/svg+xml")
}

/// Ce qui est mis en cache à l'installation.
pub fn precache(site: &Site) -> Vec<String> {
    let mut urls: Vec<String> = Lang::ALL
        .iter()
        .flat_map(|&l| [home_href(l, ""), docs_href(l)])
        .collect();
    for file in [
        "/assets/main.css",
        "/pkg/vitesse_site.js",
        "/pkg/vitesse_site_bg.wasm",
    ] {
        if site.assets.has(file) {
            urls.push(site.assets.url(file));
        }
    }
    for icon in [
        "/assets/favicon.svg",
        "/assets/icon-192.png",
        "/assets/icon-512.png",
        "/assets/icon-maskable-512.png",
    ] {
        if site.assets.has(icon) {
            urls.push(link(icon));
        }
    }
    // Les polices, appelées par la feuille de style sans empreinte.
    urls.extend(
        site.assets
            .under("/assets/fonts/")
            .filter(|f| f.ends_with(".woff2"))
            .map(link),
    );
    urls
}

pub fn worker(site: &Site) -> ServiceWorker {
    ServiceWorker::new("vitesse-site", site.version.clone())
        .precache(precache(site))
        .offline_page(link("/offline/"))
        .network_only(link("/api/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_opens_the_page_language() {
        let json = manifest(Lang::Fr).to_json();
        assert!(json.starts_with(r#"{"name":"Vitesse","short_name":"Vitesse""#));
        assert!(json.contains(r#""start_url":"/fr/""#));
        assert!(json.contains(r#""scope":"/""#));
        assert!(json.contains(r#""id":"/""#));
        assert!(json.contains(r##""theme_color":"#07070b""##));
        assert!(json.contains(r#""lang":"fr""#));
        assert!(json.contains(r#""purpose":"maskable""#));
        assert_eq!(manifest_href(Lang::Es), "/es/manifest.webmanifest");
    }
}
