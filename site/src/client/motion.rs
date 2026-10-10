//! Les animations que le CSS ne sait pas faire seul : apparitions au défilement,
//! barre de progression, navigation qui se cache, parallaxe, défilement horizontal
//! de « Sous le capot », compteurs, mots qui tournent, curseur, boutons
//! magnétiques, cartes inclinées, texte brouillé, menus, boutons « Copier » et
//! plan de la page. Tout se dégrade en une page statique.

use std::cell::Cell;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{
    Element, HtmlElement, IntersectionObserver, IntersectionObserverEntry, IntersectionObserverInit,
};

use super::{
    all, document, every, later, legacy, matches, on, on_passive, page_lang, reduced_motion,
    request_frame, window,
};
use crate::i18n::{Lang, number};

pub fn init() {
    let root: HtmlElement = document().document_element().unwrap().unchecked_into();
    intro(&root);
    reveals();
    // Seulement maintenant, le CSS peut cacher ce qui n'est pas encore apparu : ce
    // qui est déjà à l'écran vient d'être marqué, rien ne clignote.
    let _ = root.class_list().add_1("hydrated");
    scroll_effects();
    counters();
    rotating();
    copy_buttons();
    menus();
    toc();
    languages();
    if matches("(hover: hover) and (pointer: fine)") && !reduced_motion() {
        if let Some(body) = document().body() {
            let _ = body.class_list().add_1("has-cursor");
        }
        cursor();
        magnetic();
        tilt();
        scramble();
    }
}

fn set(el: &HtmlElement, name: &str, value: &str) {
    let _ = el.style().set_property(name, value);
}

fn viewport() -> (f64, f64) {
    let win = window();
    let size = |v: Result<JsValue, JsValue>| v.ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
    (size(win.inner_width()), size(win.inner_height()))
}

fn observe(
    selector: &str,
    options: &IntersectionObserverInit,
    f: impl Fn(&IntersectionObserverEntry, &IntersectionObserver) + 'static,
) {
    let callback = Closure::<dyn Fn(js_sys::Array, IntersectionObserver)>::new(
        move |entries: js_sys::Array, io: IntersectionObserver| {
            for entry in entries.iter() {
                if let Ok(entry) = entry.dyn_into::<IntersectionObserverEntry>() {
                    f(&entry, &io);
                }
            }
        },
    );
    if let Ok(io) =
        IntersectionObserver::new_with_options(callback.as_ref().unchecked_ref(), options)
    {
        for el in all(selector) {
            io.observe(&el);
        }
    }
    callback.forget();
}

/// L'écran de chargement ne joue qu'une fois par visite : aux accueils suivants,
/// il s'efface aussitôt. Un clic le passe aussi.
fn intro(root: &HtmlElement) {
    if all(".loader").is_empty() {
        return;
    }
    let storage = window().session_storage().ok().flatten();
    let seen = storage
        .as_ref()
        .and_then(|s| s.get_item("vitesse-intro").ok().flatten())
        .is_some();
    if seen || reduced_motion() {
        let _ = root.class_list().add_1("intro-seen");
    } else if let Some(storage) = storage {
        let _ = storage.set_item("vitesse-intro", "1");
    }
    for loader in all(".loader") {
        let root = root.clone();
        on(&loader, "click", move |_: web_sys::Event| {
            let _ = root.class_list().add_1("intro-seen");
        });
    }
}

/// Les éléments `[data-reveal]` apparaissent quand ils entrent à l'écran.
fn reveals() {
    let (_, height) = viewport();
    for el in all("[data-reveal]") {
        let rect = el.get_bounding_client_rect();
        if rect.top() < height * 0.92 && rect.bottom() > 0.0 {
            let _ = el.class_list().add_1("in");
        }
    }
    let options = IntersectionObserverInit::new();
    options.set_threshold(&JsValue::from_f64(0.0));
    options.set_root_margin("0px 0px -8% 0px");
    observe("[data-reveal]:not(.in)", &options, |entry, io| {
        if entry.is_intersecting() {
            let target = entry.target();
            let _ = target.class_list().add_1("in");
            io.unobserve(&target);
        }
    });
}

/// Barre de progression, navigation qui se cache, parallaxe, et le défilement
/// horizontal de « Sous le capot » (sur grand écran).
fn scroll_effects() {
    let progress = all("#progress");
    let nav = all("#nav");
    let still = reduced_motion();
    let parallax: Vec<(HtmlElement, f64)> = if still {
        Vec::new()
    } else {
        all("[data-speed]")
            .into_iter()
            .map(|el| {
                let speed = el
                    .get_attribute("data-speed")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.0);
                (el, speed)
            })
            .collect()
    };
    let journey = all("#journey").into_iter().next();
    let track = all("#journey-track").into_iter().next();
    // La largeur à faire défiler horizontalement (0 sur mobile).
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
                set(journey, "height", &format!("{}px", h + extra));
            } else {
                overflow.set(0.0);
                let _ = journey.style().remove_property("height");
                let _ = track.style().remove_property("transform");
            }
        }
    };
    measure();
    let last = Rc::new(Cell::new(-1.0f64));
    let update = move || {
        let y = window().scroll_y().unwrap_or(0.0);
        let (_, height) = viewport();
        let root = document().document_element().unwrap();
        let max = (f64::from(root.scroll_height()) - height).max(1.0);
        for bar in &progress {
            set(
                bar,
                "transform",
                &format!("scaleX({:.4})", (y / max).clamp(0.0, 1.0)),
            );
        }
        let previous = last.get();
        for nav in &nav {
            let list = nav.class_list();
            let _ = list.toggle_with_force("nav--scrolled", y > 30.0);
            if y > previous + 2.0 && y > 400.0 {
                let _ = list.add_1("nav--hidden");
            } else if y < previous - 2.0 {
                let _ = list.remove_1("nav--hidden");
            }
        }
        last.set(y);
        for (el, speed) in &parallax {
            set(
                el,
                "transform",
                &format!("translate3d(0,{:.1}px,0)", y * speed),
            );
        }
        let extra = overflow.get();
        if extra > 0.0
            && let (Some(journey), Some(track)) = (&journey, &track)
        {
            let rect = journey.get_bounding_client_rect();
            let p = (-rect.top() / (rect.height() - height).max(1.0)).clamp(0.0, 1.0);
            set(
                track,
                "transform",
                &format!("translate3d({:.1}px,0,0)", -p * extra),
            );
            set(journey, "--p", &format!("{p:.4}"));
        }
    };
    update();
    let update = Rc::new(update);
    let ticking = Rc::new(Cell::new(false));
    let schedule = move || {
        if ticking.replace(true) {
            return;
        }
        let (update, ticking) = (update.clone(), ticking.clone());
        request_frame(move |_| {
            ticking.set(false);
            update();
            false
        });
    };
    let on_scroll = schedule.clone();
    on_passive(&window(), "scroll", move |_: web_sys::Event| on_scroll());
    on_passive(&window(), "resize", move |_: web_sys::Event| {
        measure();
        schedule();
    });
}

/// Les nombres défilent jusqu'à leur valeur quand ils apparaissent (`data-count`,
/// `data-decimals`, `data-prefix`, `data-suffix`), au format de la langue.
fn counters() {
    let lang = page_lang();
    let options = IntersectionObserverInit::new();
    options.set_threshold(&JsValue::from_f64(0.4));
    observe("[data-count]", &options, move |entry, io| {
        if !entry.is_intersecting() {
            return;
        }
        let el: HtmlElement = entry.target().unchecked_into();
        io.unobserve(&el);
        if reduced_motion() {
            return;
        }
        let data = el.dataset();
        let target: f64 = data
            .get("count")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let decimals: usize = data
            .get("decimals")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let prefix = data.get("prefix").unwrap_or_default();
        let suffix = data.get("suffix").unwrap_or_default();
        let render = move |v: f64| format!("{prefix}{}{suffix}", number(lang, v, decimals));
        let mut start = None;
        request_frame(move |now| {
            let t0 = *start.get_or_insert(now);
            let p = ((now - t0) / 1800.0).min(1.0);
            let eased = if p >= 1.0 {
                1.0
            } else {
                1.0 - 2f64.powf(-10.0 * p)
            };
            el.set_text_content(Some(&render(target * eased)));
            p < 1.0
        });
    });
}

/// Les mots du héros défilent un à un.
fn rotating() {
    if reduced_motion() {
        return;
    }
    for track in all(".rotating__track") {
        let words: Vec<Element> = (0..track.children().length())
            .filter_map(|i| track.children().item(i))
            .collect();
        if words.len() < 2 {
            continue;
        }
        let mut index = 0;
        every(2300, move || {
            index = (index + 1) % words.len();
            set(&track, "--idx", &index.to_string());
            for (i, word) in words.iter().enumerate() {
                let _ = word.class_list().toggle_with_force("is-active", i == index);
            }
        });
    }
}

/// Les boutons « Copier » : la commande du héros (`data-copy`) ou le bloc de code
/// de la documentation qui suit le bouton.
fn copy_buttons() {
    on(&document(), "click", |e: web_sys::MouseEvent| {
        let Some(target) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
            return;
        };
        let Some(button) = target.closest(".copy, .doc-code__copy").ok().flatten() else {
            return;
        };
        let doc_code = button.class_list().contains("doc-code__copy");
        let text = if doc_code {
            button
                .parent_element()
                .and_then(|bar| bar.next_element_sibling())
                .and_then(|pre| pre.text_content())
        } else {
            button.get_attribute("data-copy")
        };
        let Some(text) = text else { return };
        let promise = window().navigator().clipboard().write_text(&text);
        wasm_bindgen_futures::spawn_local(async move {
            if wasm_bindgen_futures::JsFuture::from(promise).await.is_err() {
                return;
            }
            let class = if doc_code { "is-done" } else { "copy--done" };
            let before = button.text_content();
            if let Some(done) = button.get_attribute("data-copied") {
                button.set_text_content(Some(&done));
            }
            let _ = button.class_list().add_1(class);
            later(1600, move || {
                button.set_text_content(before.as_deref());
                let _ = button.class_list().remove_1(class);
            });
        });
    });
}

/// Le menu du téléphone et le sommaire de la documentation.
fn menus() {
    let (nav, menu) = (
        all("#nav").into_iter().next(),
        all("#menu").into_iter().next(),
    );
    let side = all("#doc-side").into_iter().next();
    let set_menu = {
        let (nav, menu) = (nav.clone(), menu.clone());
        move |open: bool| {
            if let (Some(nav), Some(menu)) = (&nav, &menu) {
                let _ = nav.class_list().toggle_with_force("nav--open", open);
                let _ = menu.class_list().toggle_with_force("menu--open", open);
                let _ = menu.set_attribute("aria-hidden", if open { "false" } else { "true" });
                let _ = menu.toggle_attribute_with_force("inert", !open);
            }
            for burger in all(".nav__burger") {
                let _ = burger.set_attribute("aria-expanded", if open { "true" } else { "false" });
            }
        }
    };
    let set_side = {
        let side = side.clone();
        move |open: bool| {
            if let Some(side) = &side {
                let _ = side.class_list().toggle_with_force("is-open", open);
            }
            for button in all(".docs__menu") {
                let _ = button.set_attribute("aria-expanded", if open { "true" } else { "false" });
            }
        }
    };
    for burger in all(".nav__burger") {
        let (set_menu, menu) = (set_menu.clone(), menu.clone());
        on(&burger, "click", move |_: web_sys::Event| {
            let open = menu
                .as_ref()
                .is_some_and(|m| m.class_list().contains("menu--open"));
            set_menu(!open);
        });
    }
    for link in all(".menu a") {
        let set_menu = set_menu.clone();
        on(&link, "click", move |_: web_sys::Event| set_menu(false));
    }
    for button in all(".docs__menu") {
        let (set_side, side) = (set_side.clone(), side.clone());
        on(&button, "click", move |_: web_sys::Event| {
            let open = side
                .as_ref()
                .is_some_and(|s| s.class_list().contains("is-open"));
            set_side(!open);
        });
    }
    for el in all(".docs__scrim, .doc-side__nav a, .doc-side .doc-hit") {
        let set_side = set_side.clone();
        on(&el, "click", move |_: web_sys::Event| set_side(false));
    }
    on(&window(), "keydown", move |e: web_sys::KeyboardEvent| {
        if e.key() == "Escape" {
            set_menu(false);
            set_side(false);
        }
    });
}

/// « Sur cette page » : la section en cours de lecture est en surbrillance.
fn toc() {
    let links = all(".doc-toc a");
    let headings = all(".doc-body h2[id], .doc-body h3[id]");
    if links.is_empty() || headings.is_empty() {
        return;
    }
    let current = Rc::new(Cell::new(usize::MAX));
    let update = move || {
        // La dernière section dont le titre est passé sous la navigation.
        let active = headings
            .iter()
            .take_while(|h| h.get_bounding_client_rect().top() < 150.0)
            .count()
            .saturating_sub(1);
        let href = format!("#{}", headings[active].id());
        let index = links
            .iter()
            .position(|l| l.get_attribute("href").as_deref() == Some(href.as_str()))
            .unwrap_or(0);
        if current.replace(index) != index {
            for (i, link) in links.iter().enumerate() {
                let _ = link.class_list().toggle_with_force("is-active", i == index);
            }
        }
    };
    update();
    on_passive(&window(), "scroll", move |_: web_sys::Event| update());
}

/// Le choix d'une langue est mémorisé.
fn languages() {
    for link in all("a[data-lang]") {
        let Some(lang) = link
            .get_attribute("data-lang")
            .and_then(|c| Lang::parse(&c))
        else {
            continue;
        };
        on(&link, "click", move |_: web_sys::Event| {
            legacy::remember(lang)
        });
    }
}

/// Un point suit le pointeur, un anneau le rattrape ; l'anneau grossit sur les
/// liens et affiche l'étiquette `data-cursor` de ce qu'il survole.
fn cursor() {
    let (Some(dot), Some(ring)) = (
        all(".cursor__dot").into_iter().next(),
        all(".cursor__ring").into_iter().next(),
    ) else {
        return;
    };
    // Hors de l'écran tant que le pointeur n'a pas bougé.
    for el in [&dot, &ring] {
        set(el, "transform", "translate3d(-100px,-100px,0)");
    }
    let target = Rc::new(Cell::new((-100.0f64, -100.0f64)));
    {
        let (target, dot, ring) = (target.clone(), dot.clone(), ring.clone());
        on_passive(&window(), "pointermove", move |e: web_sys::PointerEvent| {
            let (x, y) = (f64::from(e.client_x()), f64::from(e.client_y()));
            target.set((x, y));
            set(&dot, "transform", &format!("translate3d({x}px,{y}px,0)"));
            let hovered = e
                .target()
                .and_then(|t| t.dyn_into::<Element>().ok())
                .and_then(|t| t.closest("a, button, input, [data-cursor]").ok().flatten());
            let label = hovered
                .as_ref()
                .and_then(|el| el.get_attribute("data-cursor"))
                .unwrap_or_default();
            let list = ring.class_list();
            let _ = list.toggle_with_force("cursor__ring--hover", hovered.is_some());
            let _ = list.toggle_with_force("cursor__ring--label", !label.is_empty());
            if let Some(span) = ring.query_selector("span").ok().flatten()
                && span.text_content().unwrap_or_default() != label
            {
                span.set_text_content(Some(&label));
            }
        });
    }
    for (event, down) in [("pointerdown", true), ("pointerup", false)] {
        let ring = ring.clone();
        on(&window(), event, move |_: web_sys::Event| {
            let _ = ring
                .class_list()
                .toggle_with_force("cursor__ring--down", down);
        });
    }
    let mut pos = (-100.0f64, -100.0f64);
    request_frame(move |_| {
        let (tx, ty) = target.get();
        if (tx - pos.0).abs() > 0.1 || (ty - pos.1).abs() > 0.1 {
            pos.0 += (tx - pos.0) * 0.16;
            pos.1 += (ty - pos.1) * 0.16;
            set(
                &ring,
                "transform",
                &format!("translate3d({:.1}px,{:.1}px,0)", pos.0, pos.1),
            );
        }
        true
    });
}

/// Les liens `data-magnetic` se laissent attirer par le pointeur.
fn magnetic() {
    for el in all("[data-magnetic]") {
        let strength: f64 = el
            .get_attribute("data-strength")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.35);
        let target = el.clone();
        on(&el, "pointermove", move |e: web_sys::PointerEvent| {
            let r = target.get_bounding_client_rect();
            let x = f64::from(e.client_x()) - r.left() - r.width() / 2.0;
            let y = f64::from(e.client_y()) - r.top() - r.height() / 2.0;
            set(&target, "--mx", &format!("{:.1}px", x * strength));
            set(&target, "--my", &format!("{:.1}px", y * strength));
        });
        let target = el.clone();
        on(&el, "pointerleave", move |_: web_sys::Event| {
            set(&target, "--mx", "0px");
            set(&target, "--my", "0px");
        });
    }
}

/// Les cartes `data-tilt` s'inclinent vers le pointeur, avec une lumière qui le suit.
fn tilt() {
    for el in all("[data-tilt]") {
        let target = el.clone();
        on(&el, "pointermove", move |e: web_sys::PointerEvent| {
            let r = target.get_bounding_client_rect();
            let x = (f64::from(e.client_x()) - r.left()) / r.width().max(1.0);
            let y = (f64::from(e.client_y()) - r.top()) / r.height().max(1.0);
            set(&target, "--px", &format!("{:.1}%", x * 100.0));
            set(&target, "--py", &format!("{:.1}%", y * 100.0));
            set(&target, "--ry", &format!("{:.2}deg", (x - 0.5) * 10.0));
            set(&target, "--rx", &format!("{:.2}deg", (0.5 - y) * 10.0));
        });
        let target = el.clone();
        on(&el, "pointerleave", move |_: web_sys::Event| {
            set(&target, "--rx", "0deg");
            set(&target, "--ry", "0deg");
        });
    }
}

const GLYPHS: &[char] = &[
    '#', '%', '&', '*', '+', '=', '/', '<', '>', '?', '0', '1', '⚡', '_',
];

/// Les lettres des liens de la navigation se mélangent puis se remettent en place
/// au survol.
fn scramble() {
    for el in all(".scramble") {
        let Some(text) = el.get_attribute("data-text") else {
            continue;
        };
        let chars: Rc<Vec<char>> = Rc::new(text.chars().collect());
        let running = Rc::new(Cell::new(false));
        let target = el.clone();
        on(&el, "mouseenter", move |_: web_sys::Event| {
            if running.replace(true) {
                return;
            }
            let (chars, running, target) = (chars.clone(), running.clone(), target.clone());
            let total = chars.len() * 2 + 4;
            let mut frame = 0usize;
            let mut last = 0.0f64;
            request_frame(move |now| {
                if now - last < 28.0 {
                    return true;
                }
                last = now;
                frame += 1;
                let revealed = frame.saturating_sub(4) / 2;
                let shown: String = chars
                    .iter()
                    .enumerate()
                    .map(|(i, &c)| {
                        if i < revealed || c == ' ' || frame >= total {
                            c
                        } else {
                            GLYPHS[(js_sys::Math::random() * GLYPHS.len() as f64) as usize
                                % GLYPHS.len()]
                        }
                    })
                    .collect();
                target.set_text_content(Some(&shown));
                let more = frame < total;
                if !more {
                    running.set(false);
                }
                more
            });
        });
    }
}
