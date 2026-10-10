//! Écran de chargement, curseur, navigation et pied de page.

use std::cell::Cell;
use std::rc::Rc;

use gloo_events::EventListener;
use gloo_timers::callback::Interval;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement};
use yew::prelude::*;

use super::ui::{Icon, Logo, Magnetic, Scramble};
use crate::data::GITHUB;
use crate::dom;

// ----- Écran de chargement --------------------------------------------------

#[derive(Properties, PartialEq)]
pub struct LoaderProps {
    pub done: bool,
}

/// Un éclair qui se dessine pendant qu'un compteur monte à 100, puis le
/// rideau se lève.
#[component]
pub fn Loader(props: &LoaderProps) -> Html {
    let percent = use_state(|| 0u32);
    {
        let percent = percent.clone();
        use_effect_with((), move |_| {
            let mut p = 0u32;
            let interval = Interval::new(16, move || {
                if p < 100 {
                    // Accélère puis ralentit, comme un vrai chargement.
                    p = (p + 1 + (100 - p) / 18).min(100);
                    percent.set(p);
                }
            });
            move || drop(interval)
        });
    }
    html! {
        <div class={classes!("loader", props.done.then_some("loader--done"))} aria-hidden="true">
            <div class="loader__inner">
                <svg class="loader__bolt" viewBox="0 0 32 32">
                    <path d="M18.5 2 6 18h8.5l-2 12L26 13h-8.7z" />
                </svg>
                <div class="loader__word">{ "Vitesse" }</div>
                <div class="loader__bar"><span style={format!("transform:scaleX({})", f64::from(*percent) / 100.0)}></span></div>
                <div class="loader__percent">{ format!("{:03}", *percent) }</div>
            </div>
            <div class="loader__curtain"></div>
        </div>
    }
}

// ----- Curseur ---------------------------------------------------------------

/// Un point qui suit la souris, et un anneau qui le rattrape avec un temps de
/// retard ; l'anneau grossit sur les éléments cliquables.
#[component]
pub fn Cursor() -> Html {
    let dot = use_node_ref();
    let ring = use_node_ref();
    let label = use_state(String::new);
    {
        let (dot, ring, label) = (dot.clone(), ring.clone(), label.clone());
        use_effect_with((), move |_| {
            let mut keep: Vec<EventListener> = Vec::new();
            let mut anim = None;
            if dom::fine_pointer() && !dom::reduced_motion() {
                if let Some(body) = dom::document().body() {
                    let _ = body.class_list().add_1("has-cursor");
                }
                let target = Rc::new(Cell::new((-100.0f64, -100.0f64)));
                let win = dom::window();
                {
                    let target = target.clone();
                    let dot = dot.clone();
                    keep.push(EventListener::new(&win, "mousemove", move |e| {
                        let Some(e) = e.dyn_ref::<MouseEvent>() else {
                            return;
                        };
                        let (x, y) = (f64::from(e.client_x()), f64::from(e.client_y()));
                        target.set((x, y));
                        if let Some(dot) = dot.cast::<HtmlElement>() {
                            let _ = dot
                                .style()
                                .set_property("transform", &format!("translate3d({x}px,{y}px,0)"));
                        }
                    }));
                }
                {
                    let ring = ring.clone();
                    let label = label.clone();
                    keep.push(EventListener::new(&win, "mouseover", move |e| {
                        let hovered = e
                            .target()
                            .and_then(|t| t.dyn_into::<Element>().ok())
                            .and_then(|el| el.closest("a, button, [data-cursor]").ok().flatten());
                        if let Some(ring) = ring.cast::<HtmlElement>() {
                            let _ = ring
                                .class_list()
                                .toggle_with_force("cursor__ring--hover", hovered.is_some());
                        }
                        label.set(
                            hovered
                                .and_then(|el| el.get_attribute("data-cursor"))
                                .unwrap_or_default(),
                        );
                    }));
                }
                for (event, down) in [("mousedown", true), ("mouseup", false)] {
                    let ring = ring.clone();
                    keep.push(EventListener::new(&win, event, move |_| {
                        if let Some(ring) = ring.cast::<HtmlElement>() {
                            let _ = ring
                                .class_list()
                                .toggle_with_force("cursor__ring--down", down);
                        }
                    }));
                }
                let mut pos = (-100.0f64, -100.0f64);
                anim = Some(dom::raf_loop(move |_| {
                    let (tx, ty) = target.get();
                    pos.0 += (tx - pos.0) * 0.16;
                    pos.1 += (ty - pos.1) * 0.16;
                    if let Some(ring) = ring.cast::<HtmlElement>() {
                        let _ = ring.style().set_property(
                            "transform",
                            &format!("translate3d({:.1}px,{:.1}px,0)", pos.0, pos.1),
                        );
                    }
                    true
                }));
            }
            move || {
                drop(keep);
                drop(anim);
            }
        });
    }
    html! {
        <div class="cursor" aria-hidden="true">
            <div class="cursor__dot" ref={dot}></div>
            <div class={classes!("cursor__ring", (!label.is_empty()).then_some("cursor__ring--label"))} ref={ring}>
                <span>{ (*label).clone() }</span>
            </div>
        </div>
    }
}

// ----- Navigation ------------------------------------------------------------

const LINKS: [(&str, &str); 4] = [
    ("#performances", "Performances"),
    ("#fonctionnalites", "Fonctionnalités"),
    ("#express", "D'Express à Vitesse"),
    ("#demarrer", "Démarrer"),
];

#[component]
pub fn Nav() -> Html {
    let open = use_state(|| false);
    let toggle = {
        let open = open.clone();
        Callback::from(move |_: MouseEvent| open.set(!*open))
    };
    let close = {
        let open = open.clone();
        Callback::from(move |_: MouseEvent| open.set(false))
    };
    html! {
        <>
            <div class="progress" id="progress" aria-hidden="true"></div>
            <header class={classes!("nav", open.then_some("nav--open"))} id="nav">
                <a class="nav__brand" href="#top" aria-label="Vitesse, retour en haut">
                    <Logo />
                    <span>{ "Vitesse" }</span>
                </a>
                <nav class="nav__links" aria-label="Navigation principale">
                    { for LINKS.iter().map(|(href, text)| html! {
                        <a href={*href} class="nav__link"><Scramble text={*text} /></a>
                    }) }
                </nav>
                <Magnetic href={GITHUB} external=true class={classes!("btn", "btn--ghost", "nav__cta")} strength={0.25}>
                    <Icon name="github" />
                    <span>{ "GitHub" }</span>
                </Magnetic>
                <button class="nav__burger" onclick={toggle} aria-label="Menu" aria-expanded={(*open).to_string()}>
                    <span></span><span></span>
                </button>
            </header>
            <div class={classes!("menu", open.then_some("menu--open"))} aria-hidden={(!*open).to_string()}>
                <nav>
                    { for LINKS.iter().enumerate().map(|(i, (href, text))| html! {
                        <a href={*href} style={format!("--i:{i}")} onclick={close.clone()}>
                            <span class="menu__num">{ format!("0{}", i + 1) }</span>{ *text }
                        </a>
                    }) }
                    <a href={GITHUB} target="_blank" rel="noopener" style="--i:4" onclick={close.clone()}>
                        <span class="menu__num">{ "05" }</span>{ "GitHub" }
                    </a>
                </nav>
            </div>
        </>
    }
}

// ----- Pied de page ----------------------------------------------------------

#[component]
fn Clock() -> Html {
    let now = use_state(|| {
        js_sys::Date::new_0()
            .to_locale_time_string("fr-FR")
            .as_string()
            .unwrap_or_default()
    });
    {
        let now = now.clone();
        use_effect_with((), move |_| {
            let interval = Interval::new(1000, move || {
                now.set(
                    js_sys::Date::new_0()
                        .to_locale_time_string("fr-FR")
                        .as_string()
                        .unwrap_or_default(),
                );
            });
            move || drop(interval)
        });
    }
    html! { <span class="clock"><span class="clock__dot"></span>{ format!("Heure locale {}", *now) }</span> }
}

#[component]
pub fn Footer() -> Html {
    let to_top = Callback::from(|e: MouseEvent| {
        e.prevent_default();
        dom::window().scroll_to_with_x_and_y(0.0, 0.0);
    });
    let columns: [(&str, [(&str, String); 4]); 3] = [
        (
            "Projet",
            [
                ("Code source", GITHUB.to_string()),
                ("Benchmark", format!("{GITHUB}/tree/master/bench")),
                ("Exemples", format!("{GITHUB}/tree/master/examples")),
                ("Ce site (Yew)", format!("{GITHUB}/tree/master/site")),
            ],
        ),
        (
            "Documentation",
            [
                ("Guide", format!("{GITHUB}#guide")),
                (
                    "D'Express à Vitesse",
                    format!("{GITHUB}#dexpress-à-vitesse"),
                ),
                (
                    "Pourquoi c'est rapide",
                    format!("{GITHUB}#pourquoi-cest-rapide"),
                ),
                ("Limites actuelles", format!("{GITHUB}#limites-actuelles")),
            ],
        ),
        (
            "Communauté",
            [
                ("Signaler un bug", format!("{GITHUB}/issues/new")),
                ("Issues", format!("{GITHUB}/issues")),
                ("Pull requests", format!("{GITHUB}/pulls")),
                ("Historique", format!("{GITHUB}/commits/master")),
            ],
        ),
    ];
    html! {
        <footer class="footer">
            <div class="footer__glow" aria-hidden="true"></div>
            <div class="footer__top">
                <div class="footer__pitch" data-reveal="">
                    <p class="eyebrow">{ "Le framework web Rust à la Express" }</p>
                    <p class="footer__lead">{ "Écrivez du code comme avec Express. Servez-le à la vitesse du métal." }</p>
                    <Magnetic href="#demarrer" class={classes!("btn", "btn--primary")}>
                        <span>{ "Commencer maintenant" }</span>
                        <Icon name="arrow" />
                    </Magnetic>
                </div>
                <div class="footer__cols">
                    { for columns.iter().enumerate().map(|(c, (title, links))| html! {
                        <div class="footer__col" data-reveal="" style={format!("--d:{}ms", c * 90)}>
                            <h4>{ *title }</h4>
                            <ul>
                                { for links.iter().map(|(text, href)| html! {
                                    <li><a href={href.clone()} target="_blank" rel="noopener" class="footer__link">
                                        <span>{ *text }</span><Icon name="arrow" />
                                    </a></li>
                                }) }
                            </ul>
                        </div>
                    }) }
                </div>
            </div>
            <div class="footer__word" aria-hidden="true" data-reveal="">
                { for "VITESSE".chars().enumerate().map(|(i, c)| html! {
                    <span style={format!("--i:{i}")}>{ c }</span>
                }) }
            </div>
            <div class="footer__bottom">
                <span>{ "© 2026 Vitesse. Fait en Rust avec Yew et WebAssembly." }</span>
                <Clock />
                <a href="#top" class="totop" onclick={to_top} data-cursor="Haut" aria-label="Retour en haut">
                    <svg viewBox="0 0 100 100" class="totop__text" aria-hidden="true">
                        <defs><path id="totop-circle" d="M50 50 m-38 0 a38 38 0 1 1 76 0 a38 38 0 1 1 -76 0" /></defs>
                        <text><textPath href="#totop-circle">{ "RETOUR EN HAUT • RETOUR EN HAUT • " }</textPath></text>
                    </svg>
                    <Icon name="up" />
                </a>
            </div>
        </footer>
    }
}
