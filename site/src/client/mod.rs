//! Le code du navigateur (WebAssembly) : il hydrate les îlots avec active, puis
//! démarre les animations de la page. Sans lui, le site reste lisible : tout le
//! contenu est dans le HTML envoyé par le serveur.

pub mod legacy;
pub mod motion;
pub mod search;
pub mod warp;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use active::State;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{IntersectionObserver, IntersectionObserverEntry, IntersectionObserverInit};

use crate::i18n::Lang;

#[wasm_bindgen(start)]
pub fn start() {
    // Les liens d'un site publié sous un préfixe (GitHub Pages : `/Vitesse`).
    if let Some(base) = document()
        .document_element()
        .and_then(|html| html.get_attribute("data-base"))
    {
        crate::routes::set_base(&base);
    }
    // Une adresse de l'ancien site (`/#/docs/routing`) : on part vers la nouvelle.
    if legacy::redirect_hash() {
        return;
    }
    active::hydrate(&crate::islands::ISLANDS);
    motion::init();
}

pub fn window() -> web_sys::Window {
    web_sys::window().expect("window")
}

pub fn document() -> web_sys::Document {
    window().document().expect("document")
}

pub fn matches(query: &str) -> bool {
    window()
        .match_media(query)
        .ok()
        .flatten()
        .is_some_and(|m| m.matches())
}

/// Le visiteur a demandé à réduire les animations.
pub fn reduced_motion() -> bool {
    matches("(prefers-reduced-motion: reduce)")
}

/// La langue de la page (`<html lang>`).
pub fn page_lang() -> Lang {
    document()
        .document_element()
        .and_then(|h| h.get_attribute("lang"))
        .and_then(|l| Lang::parse(&l))
        .unwrap_or(Lang::En)
}

/// Écoute un événement pendant toute la vie de la page.
pub fn on<E: JsCast + 'static>(
    target: &web_sys::EventTarget,
    event: &str,
    f: impl FnMut(E) + 'static,
) {
    let mut f = f;
    let closure =
        Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| f(e.unchecked_into()));
    let _ = target.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref());
    closure.forget();
}

/// Comme [`on`], avec `{ passive: true }` (défilement et pointeur).
pub fn on_passive<E: JsCast + 'static>(
    target: &web_sys::EventTarget,
    event: &str,
    f: impl FnMut(E) + 'static,
) {
    let mut f = f;
    let closure =
        Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| f(e.unchecked_into()));
    let options = web_sys::AddEventListenerOptions::new();
    options.set_passive(true);
    let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
        event,
        closure.as_ref().unchecked_ref(),
        &options,
    );
    closure.forget();
}

type FrameCallback = Closure<dyn FnMut(f64)>;

/// Appelle `f(temps_ms)` à chaque image tant qu'elle renvoie `true`.
pub fn request_frame(f: impl FnMut(f64) -> bool + 'static) {
    let f = Rc::new(RefCell::new(f));
    let slot: Rc<RefCell<Option<FrameCallback>>> = Rc::new(RefCell::new(None));
    let again = slot.clone();
    *slot.borrow_mut() = Some(Closure::new(move |now: f64| {
        if (f.borrow_mut())(now) {
            if let Some(cb) = again.borrow().as_ref() {
                let _ = window().request_animation_frame(cb.as_ref().unchecked_ref());
            }
        } else {
            again.borrow_mut().take();
        }
    }));
    if let Some(cb) = slot.borrow().as_ref() {
        let _ = window().request_animation_frame(cb.as_ref().unchecked_ref());
    }
}

/// Tous les éléments qui correspondent à `selector`.
pub fn all(selector: &str) -> Vec<web_sys::HtmlElement> {
    let Ok(list) = document().query_selector_all(selector) else {
        return Vec::new();
    };
    (0..list.length())
        .filter_map(|i| list.item(i)?.dyn_into().ok())
        .collect()
}

/// Appelle `f` une fois, dans `ms` millisecondes.
pub fn later(ms: i32, f: impl FnOnce() + 'static) {
    let f = Closure::once_into_js(f);
    let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(f.unchecked_ref(), ms);
}

/// Appelle `f` toutes les `ms` millisecondes, pendant toute la vie de la page.
pub fn every(ms: i32, f: impl FnMut() + 'static) {
    let closure = Closure::<dyn FnMut()>::new(f);
    let _ = window().set_interval_with_callback_and_timeout_and_arguments_0(
        closure.as_ref().unchecked_ref(),
        ms,
    );
    closure.forget();
}

/// Appelle `f` une seule fois, quand `element` entre à l'écran.
pub fn when_visible(element: &web_sys::Element, threshold: f64, f: impl FnOnce() + 'static) {
    let f = RefCell::new(Some(f));
    let callback = Closure::<dyn FnMut(js_sys::Array, IntersectionObserver)>::new(
        move |entries: js_sys::Array, observer: IntersectionObserver| {
            let visible = entries.iter().any(|e| {
                e.dyn_into::<IntersectionObserverEntry>()
                    .is_ok_and(|e| e.is_intersecting())
            });
            if visible {
                observer.disconnect();
                if let Some(f) = f.borrow_mut().take() {
                    f();
                }
            }
        },
    );
    let options = IntersectionObserverInit::new();
    options.set_threshold(&threshold.into());
    if let Ok(observer) =
        IntersectionObserver::new_with_options(callback.as_ref().unchecked_ref(), &options)
    {
        observer.observe(element);
    }
    callback.forget();
}

thread_local! {
    /// Le numéro de la dernière animation des compteurs du benchmark : une nouvelle
    /// remplace la précédente.
    static COUNTING: Cell<u32> = const { Cell::new(0) };
    /// Idem pour l'exemple qui s'écrit tout seul.
    static TYPING: Cell<u32> = const { Cell::new(0) };
}

fn next(slot: &'static std::thread::LocalKey<Cell<u32>>) -> u32 {
    slot.with(|c| {
        c.set(c.get().wrapping_add(1));
        c.get()
    })
}

/// Fait défiler `progress` de 0 à 1 en `duration` millisecondes, en ralentissant.
pub fn count_up(progress: State<f64>, duration: f64) {
    let run = next(&COUNTING);
    if reduced_motion() {
        progress.set(1.0);
        return;
    }
    progress.set(0.0);
    let mut start = None;
    request_frame(move |now| {
        if COUNTING.with(Cell::get) != run {
            return false;
        }
        let t0 = *start.get_or_insert(now);
        let p = ((now - t0) / duration).min(1.0);
        // Décélération exponentielle.
        progress.set(if p >= 1.0 {
            1.0
        } else {
            1.0 - 2f64.powf(-10.0 * p)
        });
        p < 1.0
    });
}

/// Tape `total` caractères dans `typed`, trois toutes les 16 ms.
pub fn type_code(typed: State<usize>, total: usize) {
    let run = next(&TYPING);
    if reduced_motion() {
        typed.set(total);
        return;
    }
    typed.set(0);
    let mut start = None;
    request_frame(move |now| {
        if TYPING.with(Cell::get) != run {
            return false;
        }
        let t0 = *start.get_or_insert(now);
        let n = (((now - t0) / 16.0) as usize * 3).min(total);
        if n != typed.get() {
            typed.set(n);
        }
        n < total
    });
}

/// L'heure locale dans `time`, chaque seconde.
pub fn clock(lang: Lang, time: State<String>) {
    let tick = move || {
        let now = js_sys::Date::new_0()
            .to_locale_time_string(lang.locale())
            .as_string()
            .unwrap_or_default();
        time.set(now);
    };
    tick();
    every(1000, tick);
}
