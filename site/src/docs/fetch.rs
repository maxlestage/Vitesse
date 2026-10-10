//! Chargement des pages Markdown (`docs/<langue>/<slug>.md`, copiées à côté
//! du site par Trunk), avec un cache pour ne les télécharger qu'une fois.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;

use crate::dom;
use crate::i18n::Lang;

thread_local! {
    static CACHE: RefCell<HashMap<(Lang, &'static str), Rc<str>>> = RefCell::new(HashMap::new());
}

/// La page si elle est déjà en cache.
pub fn cached(lang: Lang, slug: &'static str) -> Option<Rc<str>> {
    CACHE.with(|c| c.borrow().get(&(lang, slug)).cloned())
}

/// Télécharge la page (ou la prend dans le cache). `None` si elle n'existe
/// pas dans cette langue ou si le réseau a échoué.
pub async fn load(lang: Lang, slug: &'static str) -> Option<Rc<str>> {
    if let Some(hit) = cached(lang, slug) {
        return Some(hit);
    }
    let text = fetch_text(&format!("docs/{}/{slug}.md", lang.code())).await?;
    CACHE.with(|c| c.borrow_mut().insert((lang, slug), text.clone()));
    Some(text)
}

async fn fetch_text(url: &str) -> Option<Rc<str>> {
    let response = JsFuture::from(dom::window().fetch_with_str(url))
        .await
        .ok()?
        .dyn_into::<Response>()
        .ok()?;
    if !response.ok() {
        return None;
    }
    let text = JsFuture::from(response.text().ok()?)
        .await
        .ok()?
        .as_string()?;
    Some(Rc::from(text))
}
