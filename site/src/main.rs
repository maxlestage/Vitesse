//! Le site de présentation de Vitesse, en Rust avec Yew (WebAssembly).

mod components;
mod data;
mod dom;
mod highlight;

use gloo_timers::callback::Timeout;
use yew::prelude::*;

use components::layout::{Cursor, Footer, Loader, Nav};
use components::sections::{Bench, Compare, Cta, Features, Hero, Journey, Marquee, Start, Stats};

#[component]
fn App() -> Html {
    let loaded = use_state(|| false);
    {
        let loaded = loaded.clone();
        use_effect_with((), move |_| {
            let effects = components::effects::start();
            dom::reveal_on_scroll();
            let delay = if dom::reduced_motion() { 0 } else { 1700 };
            let timeout = Timeout::new(delay, move || loaded.set(true));
            move || {
                drop(effects);
                drop(timeout);
            }
        });
    }
    use_effect_with(*loaded, |loaded| {
        if *loaded && let Some(body) = dom::document().body() {
            let _ = body.class_list().add_1("is-loaded");
        }
    });
    html! {
        <>
            <Loader done={*loaded} />
            <Cursor />
            <Nav />
            <main>
                <Hero />
                <Marquee />
                <Stats />
                <Bench />
                <Features />
                <Journey />
                <Compare />
                <Start />
                <Cta />
            </main>
            <Footer />
            <div class="noise" aria-hidden="true"></div>
        </>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
