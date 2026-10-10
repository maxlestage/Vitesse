//! Le côté navigateur de l'îlot de recherche.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use active::{Event, State};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use super::{on, window};
use crate::i18n::Lang;
use crate::routes::link;
use crate::search::{Entry, Hit, search};

type Waiting = (State<String>, State<Vec<Hit>>, State<bool>);

thread_local! {
    /// L'index de la langue de la page, téléchargé au premier usage.
    static INDEX: RefCell<Option<Rc<Vec<Entry>>>> = const { RefCell::new(None) };
    static LOADING: Cell<bool> = const { Cell::new(false) };
    /// Les champs qui attendent l'index.
    static WAITING: RefCell<Vec<Waiting>> = const { RefCell::new(Vec::new()) };
}

/// Met à jour les résultats de `query`. Le premier appel télécharge
/// `/search/{lang}.txt` (toutes les pages et sections de la langue) ; ensuite tout
/// se passe dans le navigateur, sans réseau.
pub fn refresh(lang: Lang, query: State<String>, hits: State<Vec<Hit>>, loaded: State<bool>) {
    if let Some(index) = INDEX.with(|i| i.borrow().clone()) {
        hits.set(search(&index, &query.get()));
        loaded.set(true);
        return;
    }
    WAITING.with(|w| w.borrow_mut().push((query, hits, loaded)));
    if LOADING.with(|l| l.replace(true)) {
        return;
    }
    let url = link(&format!("/search/{}.txt", lang.code()));
    spawn_local(async move {
        let waiting = |f: &dyn Fn(Waiting)| {
            for entry in WAITING.with(|w| std::mem::take(&mut *w.borrow_mut())) {
                f(entry);
            }
        };
        match fetch_text(&url).await {
            Some(text) => {
                let index = Rc::new(text.lines().filter_map(Entry::parse).collect::<Vec<_>>());
                INDEX.with(|i| *i.borrow_mut() = Some(index.clone()));
                waiting(&|(query, hits, loaded)| {
                    hits.set(search(&index, &query.get()));
                    loaded.set(true);
                });
            }
            // Hors ligne, sans index en cache : la prochaine touche réessaiera.
            None => {
                waiting(&|_| {});
                LOADING.with(|l| l.set(false));
            }
        }
    });
}

/// « / » place le curseur dans le champ, depuis n'importe où dans la page.
pub fn shortcut(input: &web_sys::Element) {
    let input = input.clone();
    on(&window(), "keydown", move |e: web_sys::KeyboardEvent| {
        let typing = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .and_then(|t| t.closest("input, textarea").ok().flatten())
            .is_some();
        if e.key() == "/" && !typing && !e.ctrl_key() && !e.meta_key() {
            e.prevent_default();
            if let Some(input) = input.dyn_ref::<web_sys::HtmlElement>() {
                let _ = input.focus();
            }
        }
    });
}

pub fn navigate(href: &str) {
    let _ = window().location().set_href(href);
}

/// Quitte le champ (Échap).
pub fn blur(e: &Event) {
    if let Some(input) = e
        .current_target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = input.blur();
    }
}

async fn fetch_text(url: &str) -> Option<String> {
    let response = JsFuture::from(window().fetch_with_str(url)).await.ok()?;
    let response = response.dyn_into::<web_sys::Response>().ok()?;
    if !response.ok() {
        return None;
    }
    JsFuture::from(response.text().ok()?)
        .await
        .ok()?
        .as_string()
}
