//! Les effets liés au défilement : barre de progression, navigation qui se
//! cache, parallaxe, et défilement horizontal de « Sous le capot ».

use std::cell::Cell;
use std::rc::Rc;

use gloo_events::EventListener;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

use crate::dom::{self, RafLoop};

fn by_id(id: &str) -> Option<HtmlElement> {
    dom::document()
        .get_element_by_id(id)
        .and_then(|e| e.dyn_into().ok())
}

fn viewport() -> (f64, f64) {
    let win = dom::window();
    (
        win.inner_width()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        win.inner_height()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
    )
}

/// Démarre les effets ; ils vivent tant que la valeur renvoyée est gardée.
pub fn start() -> (RafLoop, EventListener) {
    let progress = by_id("progress");
    let nav = by_id("nav");
    let journey = by_id("journey");
    let track = by_id("journey-track");
    let mut parallax: Vec<(HtmlElement, f64)> = Vec::new();
    if let Ok(nodes) = dom::document().query_selector_all("[data-speed]") {
        for i in 0..nodes.length() {
            let Some(el) = nodes.item(i).and_then(|n| n.dyn_into::<HtmlElement>().ok()) else {
                continue;
            };
            let speed = el
                .get_attribute("data-speed")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            parallax.push((el, speed));
        }
    }
    let still = dom::reduced_motion();

    // Largeur à faire défiler horizontalement (0 sur mobile).
    let overflow = Rc::new(Cell::new(0.0f64));
    let measure = {
        let (journey, track, overflow) = (journey.clone(), track.clone(), overflow.clone());
        move || {
            let (Some(journey), Some(track)) = (&journey, &track) else {
                return;
            };
            let (w, h) = viewport();
            if w >= 900.0 && !still {
                let extra = (f64::from(track.scroll_width()) - w).max(0.0);
                overflow.set(extra);
                dom::set_var(journey, "height", &format!("{}px", h + extra));
            } else {
                overflow.set(0.0);
                let _ = journey.style().remove_property("height");
                let _ = track.style().remove_property("transform");
            }
        }
    };
    measure();
    let resize = EventListener::new(&dom::window(), "resize", move |_| measure());

    let mut last_y = -1.0f64;
    let anim = dom::raf_loop(move |_| {
        let win = dom::window();
        let y = win.scroll_y().unwrap_or(0.0);
        let (_, h) = viewport();
        if (y - last_y).abs() > 0.5 {
            if let Some(progress) = &progress {
                let max = dom::document()
                    .document_element()
                    .map_or(1.0, |e| f64::from(e.scroll_height()) - h)
                    .max(1.0);
                dom::set_var(
                    progress,
                    "transform",
                    &format!("scaleX({:.4})", (y / max).clamp(0.0, 1.0)),
                );
            }
            if let Some(nav) = &nav {
                let list = nav.class_list();
                let _ = list.toggle_with_force("nav--scrolled", y > 30.0);
                if y > last_y + 2.0 && y > 400.0 {
                    let _ = list.add_1("nav--hidden");
                } else if y < last_y - 2.0 {
                    let _ = list.remove_1("nav--hidden");
                }
            }
            if !still {
                for (el, speed) in &parallax {
                    dom::set_var(
                        el,
                        "transform",
                        &format!("translate3d(0,{:.1}px,0)", y * speed),
                    );
                }
            }
            last_y = y;
        }
        let extra = overflow.get();
        if extra > 0.0
            && let (Some(journey), Some(track)) = (&journey, &track)
        {
            let rect = journey.get_bounding_client_rect();
            let p = (-rect.top() / (rect.height() - h).max(1.0)).clamp(0.0, 1.0);
            dom::set_var(
                track,
                "transform",
                &format!("translate3d({:.1}px,0,0)", -p * extra),
            );
            dom::set_var(journey, "--p", &format!("{p:.4}"));
        }
        true
    });
    (anim, resize)
}
