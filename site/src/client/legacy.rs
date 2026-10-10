//! Le choix de la langue et les adresses de l'ancien site.
//!
//! L'ancien site n'avait qu'une page : la documentation y vivait derrière l'ancre
//! (`/#/docs/routing/jokers`). Ces liens mènent maintenant à leur page
//! (`/fr/docs/routing/#jokers`).

use super::{document, page_lang, window};
use crate::i18n::Lang;
use crate::routes::{base, home_href, legacy_hash};

const KEY: &str = "vitesse-lang";

/// La langue choisie la dernière fois dans le sélecteur.
pub fn saved_lang() -> Option<Lang> {
    window()
        .local_storage()
        .ok()
        .flatten()
        .and_then(|s| s.get_item(KEY).ok().flatten())
        .and_then(|code| Lang::parse(&code))
}

/// La première langue du navigateur que le site parle.
pub fn browser_lang() -> Option<Lang> {
    let navigator = window().navigator();
    navigator
        .languages()
        .iter()
        .filter_map(|l| l.as_string())
        .chain(navigator.language())
        .find_map(|l| Lang::from_code(&l))
}

/// Mémorise la langue choisie : dans le navigateur (pour la page `/` d'un
/// hébergement statique) et dans un cookie (pour le serveur).
pub fn remember(lang: Lang) {
    if let Some(storage) = window().local_storage().ok().flatten() {
        let _ = storage.set_item(KEY, lang.code());
    }
    if let Ok(html) = wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlDocument>(document()) {
        let _ = html.set_cookie(&format!(
            "{KEY}={}; Path={}/; Max-Age=31536000; SameSite=Lax",
            lang.code(),
            base()
        ));
    }
}

/// Sur une page dont l'ancre vient de l'ancien site, part vers sa nouvelle adresse.
/// Renvoie `true` si la page s'en va.
pub fn redirect_hash() -> bool {
    // La page `/` d'un hébergement statique choisit d'abord la langue.
    if document()
        .query_selector("[data-island=root]")
        .ok()
        .flatten()
        .is_some()
    {
        return false;
    }
    let location = window().location();
    let hash = location.hash().unwrap_or_default();
    match legacy_hash(page_lang(), &hash) {
        Some(target) => location.replace(&target).is_ok(),
        None => false,
    }
}

/// La page `/` d'un hébergement statique : l'accueil de la langue choisie la
/// dernière fois, sinon de celle du navigateur, sinon l'anglais. Les anciennes
/// ancres suivent.
pub fn redirect_root() {
    let lang = saved_lang().or_else(browser_lang).unwrap_or(Lang::En);
    let location = window().location();
    let hash = location.hash().unwrap_or_default();
    let target =
        legacy_hash(lang, &hash).unwrap_or_else(|| format!("{}{hash}", home_href(lang, "")));
    let _ = location.replace(&target);
}
