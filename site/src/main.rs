//! Le site de présentation de Vitesse, en Rust avec Yew (WebAssembly), en
//! français, anglais et espagnol, avec sa documentation.

mod components;
mod data;
mod docs;
mod dom;
mod highlight;
mod i18n;
mod route;

use gloo_events::EventListener;
use gloo_timers::callback::Timeout;
use yew::prelude::*;

use components::layout::{Cursor, Footer, Loader, Nav};
use components::sections::{Bench, Compare, Cta, Features, Hero, Journey, Marquee, Start, Stats};
use docs::{DocsHome, DocsPage};
use i18n::{Lang, LangCtx};
use route::Route;

#[component]
fn Home() -> Html {
    let t = i18n::use_lang().lang.texts();
    use_effect_with(t.meta_title, |title| dom::document().set_title(title));
    html! {
        <>
            <Hero />
            <Marquee />
            <Stats />
            <Bench />
            <Features />
            <Journey />
            <Compare />
            <Start />
            <Cta />
        </>
    }
}

/// Fait défiler jusqu'à la section de l'ancre (`#performances`…), une fois
/// la page d'accueil affichée.
fn scroll_to_section() {
    let hash = dom::window().location().hash().unwrap_or_default();
    let id = hash.trim_start_matches('#');
    if id.is_empty() || id.starts_with('/') {
        return;
    }
    if let Some(el) = dom::document().get_element_by_id(id) {
        el.scroll_into_view();
    }
}

#[component]
fn App() -> Html {
    let lang = use_state(Lang::detect);
    let route = use_state(route::current);
    let loaded = use_state(|| false);

    {
        let route = route.clone();
        use_effect_with((), move |_| {
            let listener = EventListener::new(&dom::window(), "hashchange", move |_| {
                route.set(route::current());
            });
            move || drop(listener)
        });
    }
    {
        let loaded = loaded.clone();
        let docs = route.is_docs();
        use_effect_with((), move |_| {
            let delay = match (dom::reduced_motion(), docs) {
                (true, _) => 0,
                (false, true) => 700,
                (false, false) => 1700,
            };
            let timeout = Timeout::new(delay, move || loaded.set(true));
            move || drop(timeout)
        });
    }
    use_effect_with(*loaded, |loaded| {
        if *loaded && let Some(body) = dom::document().body() {
            let _ = body.class_list().add_1("is-loaded");
        }
    });
    use_effect_with(*lang, |lang| {
        if let Some(root) = dom::document().document_element() {
            let _ = root.set_attribute("lang", lang.code());
        }
    });
    // Les effets de défilement et les apparitions repartent à chaque
    // changement d'écran (accueil ou documentation).
    use_effect_with(route.is_docs(), |_| {
        let effects = components::effects::start();
        let reveal = dom::reveal_on_scroll();
        move || {
            drop(effects);
            drop(reveal);
        }
    });
    use_effect_with((*route).clone(), |route| {
        if *route == Route::Home {
            scroll_to_section();
        }
    });

    let context = LangCtx {
        lang: *lang,
        set: {
            let lang = lang.clone();
            Callback::from(move |l: Lang| {
                l.save();
                lang.set(l);
            })
        },
    };
    html! {
        <ContextProvider<LangCtx> {context}>
            <Loader done={*loaded} />
            <Cursor />
            <Nav docs={route.is_docs()} />
            <main class={classes!(route.is_docs().then_some("main--docs"))}>
                { match &*route {
                    Route::Home => html! { <Home /> },
                    Route::DocsHome => html! { <DocsHome /> },
                    Route::Doc { slug, anchor } => html! { <DocsPage slug={*slug} anchor={anchor.clone()} /> },
                } }
            </main>
            <Footer />
            <div class="noise" aria-hidden="true"></div>
        </ContextProvider<LangCtx>>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
