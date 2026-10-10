//! Petits utilitaires DOM : boucles d'animation, apparition à l'écran,
//! préférences de mouvement.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use web_sys::{
    Element, HtmlElement, IntersectionObserver, IntersectionObserverEntry,
    IntersectionObserverInit, Window,
};

pub fn window() -> Window {
    web_sys::window().expect("pas de fenêtre")
}

pub fn document() -> web_sys::Document {
    window().document().expect("pas de document")
}

/// L'utilisateur a demandé à réduire les animations.
pub fn reduced_motion() -> bool {
    window()
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .is_some_and(|m| m.matches())
}

/// L'appareil a un vrai pointeur (souris), pas seulement un écran tactile.
pub fn fine_pointer() -> bool {
    window()
        .match_media("(hover: hover) and (pointer: fine)")
        .ok()
        .flatten()
        .is_some_and(|m| m.matches())
}

/// Une boucle `requestAnimationFrame` ; elle s'arrête quand on la lâche.
pub struct RafLoop {
    running: Rc<Cell<bool>>,
}

impl Drop for RafLoop {
    fn drop(&mut self) {
        self.running.set(false);
    }
}

/// Appelle `frame(temps_ms)` à chaque image ; `frame` renvoie `false` pour
/// arrêter la boucle.
pub fn raf_loop(mut frame: impl FnMut(f64) -> bool + 'static) -> RafLoop {
    let running = Rc::new(Cell::new(true));
    type Slot = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;
    let slot: Slot = Rc::new(RefCell::new(None));
    let next = slot.clone();
    let alive = running.clone();
    *slot.borrow_mut() = Some(Closure::new(move |t: f64| {
        if !alive.get() || !frame(t) {
            alive.set(false);
            return;
        }
        if let Some(cb) = next.borrow().as_ref() {
            let _ = window().request_animation_frame(cb.as_ref().unchecked_ref());
        }
    }));
    if let Some(cb) = slot.borrow().as_ref() {
        let _ = window().request_animation_frame(cb.as_ref().unchecked_ref());
    }
    RafLoop { running }
}

/// Appelle `on_visible` une seule fois, quand `element` entre à l'écran.
/// Renvoie l'observateur (à garder en vie, ou à déconnecter).
pub fn on_first_visible(
    element: &Element,
    threshold: f64,
    on_visible: impl FnOnce() + 'static,
) -> IntersectionObserver {
    let on_visible = RefCell::new(Some(on_visible));
    let callback = Closure::<dyn FnMut(js_sys::Array, IntersectionObserver)>::new(
        move |entries: js_sys::Array, observer: IntersectionObserver| {
            let visible = entries.iter().any(|e| {
                e.dyn_into::<IntersectionObserverEntry>()
                    .is_ok_and(|e| e.is_intersecting())
            });
            if visible {
                observer.disconnect();
                if let Some(f) = on_visible.borrow_mut().take() {
                    f();
                }
            }
        },
    );
    let options = IntersectionObserverInit::new();
    options.set_threshold(&threshold.into());
    let observer =
        IntersectionObserver::new_with_options(callback.as_ref().unchecked_ref(), &options)
            .expect("IntersectionObserver");
    callback.forget();
    observer.observe(element);
    observer
}

/// Ajoute la classe `in` à chaque élément `[data-reveal]` qui entre à l'écran
/// (les animations elles-mêmes sont en CSS).
pub fn reveal_on_scroll() {
    let callback = Closure::<dyn FnMut(js_sys::Array, IntersectionObserver)>::new(
        |entries: js_sys::Array, observer: IntersectionObserver| {
            for entry in entries.iter() {
                let Ok(entry) = entry.dyn_into::<IntersectionObserverEntry>() else {
                    continue;
                };
                if entry.is_intersecting() {
                    let target = entry.target();
                    let _ = target.class_list().add_1("in");
                    observer.unobserve(&target);
                }
            }
        },
    );
    let options = IntersectionObserverInit::new();
    options.set_threshold(&0.0.into());
    options.set_root_margin("0px 0px -8% 0px");
    let Ok(observer) =
        IntersectionObserver::new_with_options(callback.as_ref().unchecked_ref(), &options)
    else {
        return;
    };
    callback.forget();
    if let Ok(nodes) = document().query_selector_all("[data-reveal]:not(.in)") {
        for i in 0..nodes.length() {
            if let Some(node) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                observer.observe(&node);
            }
        }
    }
}

/// Définit une variable CSS sur un élément.
pub fn set_var(element: &HtmlElement, name: &str, value: &str) {
    let _ = element.style().set_property(name, value);
}

/// Un nombre aléatoire dans `[0, 1[`.
pub fn random() -> f64 {
    js_sys::Math::random()
}
