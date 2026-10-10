//! Les sections de la page, de haut en bas.

use std::cell::RefCell;
use std::rc::Rc;

use gloo_timers::callback::Interval;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;
use yew::prelude::*;

use super::ui::{Code, CopyButton, Counter, Icon, Magnetic, Rotating, Split, fr_number};
use super::warp::Warp;
use crate::data::{EXAMPLES, FEATURES, GITHUB, JOURNEY, SCENARIOS, SERVERS};
use crate::dom;

const INSTALL: &str = "cargo add vitesse --git https://github.com/maxlestage/vitesse";

// ----- Héros -----------------------------------------------------------------

#[component]
pub fn Hero() -> Html {
    let words: Vec<AttrValue> = ["natif", "minimaliste", "ultra-rapide", "sûr", "familier"]
        .into_iter()
        .map(AttrValue::from)
        .collect();
    html! {
        <section class="hero" id="top">
            <div class="hero__bg" aria-hidden="true">
                <Warp />
                <div class="blob blob--a" data-speed="0.12"></div>
                <div class="blob blob--b" data-speed="-0.08"></div>
                <div class="blob blob--c" data-speed="0.05"></div>
                <div class="hero__grid"></div>
                <div class="hero__fade"></div>
            </div>
            <div class="hero__content">
                <a class="badge intro" style="--d:0ms" href="#performances">
                    <span class="badge__dot"></span>
                    <span>{ "Nouveau · 2,78 millions de requêtes/s sur 2 cœurs" }</span>
                    <Icon name="arrow" />
                </a>
                <h1 class="hero__title">
                    <span class="hero__line"><Split text="Le confort d'Express." /></span>
                    <span class="hero__line">
                        <Split text="La vitesse de" offset={22} />
                        { " " }
                        <span class="grad-text"><Split text="Rust." offset={36} /></span>
                    </span>
                </h1>
                <p class="hero__sub intro" style="--d:900ms">
                    { "Un framework web " }<Rotating {words} />{ " pour Rust." }
                </p>
                <p class="hero__lead intro" style="--d:1050ms">
                    { "Vitesse reprend l'API que vous connaissez déjà — " }
                    <code>{ "app.get" }</code>{ ", " }<code>{ "req.params" }</code>{ ", " }<code>{ "res.json" }</code>
                    { " — et la propulse avec son propre moteur HTTP/1.1. Plus rapide qu'actix-web, axum et Drogon." }
                </p>
                <div class="hero__ctas intro" style="--d:1200ms">
                    <Magnetic href="#demarrer" class={classes!("btn", "btn--primary", "btn--lg")} cursor={Some(AttrValue::from("Go"))}>
                        <span>{ "Commencer" }</span><Icon name="arrow" />
                    </Magnetic>
                    <Magnetic href="#performances" class={classes!("btn", "btn--ghost", "btn--lg")}>
                        <span>{ "Voir les performances" }</span>
                    </Magnetic>
                </div>
                <div class="install intro" style="--d:1350ms">
                    <span class="install__prompt">{ "$" }</span>
                    <code>{ INSTALL }</code>
                    <CopyButton text={INSTALL} />
                </div>
            </div>
            <a class="scroll-hint intro" style="--d:1600ms" href="#chiffres" aria-label="Défiler vers la suite">
                <span class="scroll-hint__mouse"><span></span></span>
                <span>{ "Défiler" }</span>
            </a>
        </section>
    }
}

// ----- Bandeaux défilants ----------------------------------------------------

#[component]
pub fn Marquee() -> Html {
    let words = [
        "L'API d'Express",
        "2,78 M req/s",
        "Moteur HTTP/1.1 maison",
        "Zéro regex",
        "Un thread par cœur",
        "100 % Rust",
        "Arrêt propre",
        "Tests sans réseau",
    ];
    let code = [
        "app.get(\"/\")",
        "req.param(\"id\")",
        "res::json(..)",
        "next.run(req).await",
        "Router::new()",
        "app.static_dir(..)",
        "middleware::cors()",
        "app.run(3000)",
    ];
    let row = |items: &[&str], class: &'static str| {
        let once = html! {
            <>{ for items.iter().map(|w| html! { <><span class="marquee__item">{ *w }</span><span class="marquee__star">{ "✦" }</span></> }) }</>
        };
        html! {
            <div class={classes!("marquee__row", class)}>
                <div class="marquee__track">
                    <div class="marquee__group">{ once.clone() }</div>
                    <div class="marquee__group" aria-hidden="true">{ once }</div>
                </div>
            </div>
        }
    };
    html! {
        <section class="marquee" aria-label="Points forts">
            { row(&words, "marquee__row--a") }
            { row(&code, "marquee__row--b") }
        </section>
    }
}

// ----- Titre de section ------------------------------------------------------

#[derive(Properties, PartialEq)]
struct HeadProps {
    num: &'static str,
    label: &'static str,
    title: &'static str,
    #[prop_or_default]
    lead: Option<&'static str>,
}

#[component]
fn Head(props: &HeadProps) -> Html {
    html! {
        <div class="head">
            <p class="eyebrow" data-reveal=""><span class="eyebrow__num">{ props.num }</span>{ props.label }</p>
            <h2 class="head__title" data-reveal="" style="--d:80ms">{ props.title }</h2>
            if let Some(lead) = props.lead {
                <p class="head__lead" data-reveal="" style="--d:160ms">{ lead }</p>
            }
        </div>
    }
}

// ----- Chiffres ----------------------------------------------------------------

#[component]
pub fn Stats() -> Html {
    let stats: [(f64, usize, &str, &str, &str, &str); 4] = [
        (
            2.78,
            2,
            "",
            " M",
            "requêtes par seconde",
            "Sur deux cœurs, en pipeline : le test « plaintext » du TechEmpower.",
        ),
        (
            1.62,
            2,
            "×",
            "",
            "plus rapide qu'actix-web",
            "Avec une vraie requête de navigateur, et ×2,2 en pipeline.",
        ),
        (
            6.1,
            1,
            "",
            " µs",
            "de CPU par requête",
            "Contre 7,1 µs pour actix-web et 11,7 µs pour axum.",
        ),
        (
            50.0,
            0,
            "×",
            "",
            "plus rapide qu'Express",
            "De 49 à 56 fois selon le scénario, sur le même matériel.",
        ),
    ];
    html! {
        <section class="section" id="chiffres">
            <Head num="01" label="En chiffres" title="Des chiffres qui parlent d'eux-mêmes." />
            <div class="stats">
                { for stats.iter().enumerate().map(|(i, (value, decimals, prefix, suffix, label, detail))| html! {
                    <article class="stat card" data-reveal="" style={format!("--d:{}ms", i * 110)}>
                        <div class="stat__value">
                            <Counter value={*value} decimals={*decimals} prefix={*prefix} suffix={*suffix} />
                        </div>
                        <h3 class="stat__label">{ *label }</h3>
                        <p class="stat__detail">{ *detail }</p>
                        <span class="stat__line"></span>
                    </article>
                }) }
            </div>
        </section>
    }
}

// ----- Benchmark ---------------------------------------------------------------

#[component]
pub fn Bench() -> Html {
    let selected = use_state(|| 0usize);
    let visible = use_state(|| false);
    let panel = use_node_ref();
    {
        let (visible, panel) = (visible.clone(), panel.clone());
        use_effect_with((), move |_| {
            let observer = panel
                .cast::<HtmlElement>()
                .map(|el| dom::on_first_visible(&el, 0.1, move || visible.set(true)));
            move || {
                if let Some(o) = observer {
                    o.disconnect();
                }
            }
        });
    }
    let scenario = &SCENARIOS[*selected];
    let max = scenario.rps.iter().cloned().fold(0.0, f64::max);
    let vs_actix = scenario.rps[4] / scenario.rps[3];
    let vs_axum = scenario.rps[4] / scenario.rps[2];
    html! {
        <section class="section bench" id="performances">
            <Head num="02" label="Performances"
                  title="Plus rapide qu'actix\u{2011}web. Bien plus rapide que le reste."
                  lead={Some("Mêmes routes, même machine, même session : Express, Drogon (C++), axum, actix-web et Vitesse face au même générateur de charge.")} />
            <div class="bench__tabs" role="tablist" data-reveal="" style={format!("--tab:{}; --tabs:{}", *selected, SCENARIOS.len())}>
                <span class="bench__indicator" aria-hidden="true"></span>
                { for SCENARIOS.iter().enumerate().map(|(i, s)| {
                    let selected = selected.clone();
                    html! {
                        <button role="tab" aria-selected={(i == *selected).to_string()}
                                class={classes!("bench__tab", (i == *selected).then_some("is-active"))}
                                onclick={Callback::from(move |_: MouseEvent| selected.set(i))}>
                            { s.label }
                        </button>
                    }
                }) }
            </div>
            // « in » est repris ici : Yew réécrit l'attribut class quand `visible`
            // change, ce qui effacerait la classe posée par l'observateur.
            <div class={classes!("bench__panel", "card", visible.then_some("is-visible"), visible.then_some("in"))} ref={panel} data-reveal="">
                <div class="bench__meta">
                    <p class="bench__detail">{ scenario.detail }</p>
                    <div class="bench__badges">
                        <span class="pill pill--hot">{ format!("×{} face à actix-web", fr_number(vs_actix, 2)) }</span>
                        <span class="pill">{ format!("×{} face à axum", fr_number(vs_axum, 1)) }</span>
                    </div>
                </div>
                <div class="bench__rows">
                    { for SERVERS.iter().enumerate().map(|(i, name)| {
                        let rps = scenario.rps[i];
                        let width = if *visible { (rps / max * 100.0).max(0.6) } else { 0.0 };
                        let ours = i == SERVERS.len() - 1;
                        html! {
                            <div class={classes!("bar", ours.then_some("bar--ours"))} style={format!("--i:{i}")}>
                                <span class="bar__name">{ *name }</span>
                                <span class="bar__track">
                                    <span class="bar__fill" style={format!("width:{width:.2}%")}></span>
                                </span>
                                <span class="bar__value">
                                    <Counter key={format!("{}-{i}", scenario.id)} value={rps} duration={1400.0} />
                                    <small>{ " req/s" }</small>
                                </span>
                                <span class="bar__cpu">{ format!("{} µs", fr_number(scenario.cpu[i], if scenario.cpu[i] < 10.0 { 1 } else { 0 })) }</span>
                            </div>
                        }
                    }) }
                </div>
                <p class="bench__note">
                    { "VM 4 vCPU : serveur sur 2 cœurs, wrk sur les 2 autres, 128 connexions keep-alive, 10 s par scénario. À droite, le temps CPU consommé par le serveur pour chaque requête. " }
                    <a href={format!("{GITHUB}/tree/master/bench")} target="_blank" rel="noopener">{ "Reproduire le benchmark" }</a>
                </p>
            </div>
        </section>
    }
}

// ----- Fonctionnalités -----------------------------------------------------------

#[component]
pub fn Features() -> Html {
    let onmousemove = Callback::from(|e: MouseEvent| {
        let Some(el) = e
            .current_target()
            .and_then(|t| t.dyn_into::<HtmlElement>().ok())
        else {
            return;
        };
        let rect = el.get_bounding_client_rect();
        let x = (f64::from(e.client_x()) - rect.left()) / rect.width();
        let y = (f64::from(e.client_y()) - rect.top()) / rect.height();
        dom::set_var(&el, "--px", &format!("{:.1}%", x * 100.0));
        dom::set_var(&el, "--py", &format!("{:.1}%", y * 100.0));
        dom::set_var(&el, "--ry", &format!("{:.2}deg", (x - 0.5) * 10.0));
        dom::set_var(&el, "--rx", &format!("{:.2}deg", (0.5 - y) * 10.0));
    });
    let onmouseleave = Callback::from(|e: MouseEvent| {
        let Some(el) = e
            .current_target()
            .and_then(|t| t.dyn_into::<HtmlElement>().ok())
        else {
            return;
        };
        dom::set_var(&el, "--rx", "0deg");
        dom::set_var(&el, "--ry", "0deg");
    });
    html! {
        <section class="section" id="fonctionnalites">
            <Head num="03" label="Fonctionnalités"
                  title="Tout ce qu'Express sait faire. En Rust, sans compromis."
                  lead={Some("Un cœur volontairement petit, comme Express, et tout ce qu'il faut pour une vraie application.")} />
            <div class="features">
                { for FEATURES.iter().enumerate().map(|(i, f)| html! {
                    <div class="feature-wrap" data-reveal="" style={format!("--d:{}ms", (i % 4) * 90)}>
                        <article class="feature card" onmousemove={onmousemove.clone()} onmouseleave={onmouseleave.clone()}>
                            <span class="feature__spot" aria-hidden="true"></span>
                            <span class="feature__icon"><Icon name={f.icon} /></span>
                            <h3>{ f.title }</h3>
                            <p>{ f.text }</p>
                            <code class="feature__tag">{ f.tag }</code>
                        </article>
                    </div>
                }) }
            </div>
        </section>
    }
}

// ----- Sous le capot (défilement horizontal) ------------------------------------

#[component]
pub fn Journey() -> Html {
    html! {
        <section class="journey" id="journey">
            <div class="journey__sticky">
                <div class="journey__head">
                    <p class="eyebrow"><span class="eyebrow__num">{ "04" }</span>{ "Sous le capot" }</p>
                    <h2 class="head__title">{ "Le voyage d'une requête, en six microsecondes." }</h2>
                    <div class="journey__bar" aria-hidden="true"><span></span></div>
                </div>
                <div class="journey__track" id="journey-track">
                    { for JOURNEY.iter().enumerate().map(|(i, step)| html! {
                        <article class="panel">
                            <div class="panel__top">
                                <span class="panel__num">{ format!("0{}", i + 1) }</span>
                                <span class="pill pill--hot">{ step.metric }</span>
                            </div>
                            <div class={classes!("viz", format!("viz--{}", i + 1))} aria-hidden="true">
                                { for (0..12).map(|k| html! { <span style={format!("--k:{k}")}></span> }) }
                            </div>
                            <h3>{ step.title }</h3>
                            <p>{ step.text }</p>
                        </article>
                    }) }
                    <article class="panel panel--end">
                        <p class="panel__big">{ "≈ 6 µs" }</p>
                        <p>{ "de CPU par requête, du premier octet lu au dernier octet écrit." }</p>
                        <Magnetic href="#express" class={classes!("btn", "btn--primary")}>
                            <span>{ "Voir le code" }</span><Icon name="arrow" />
                        </Magnetic>
                    </article>
                </div>
            </div>
        </section>
    }
}

// ----- D'Express à Vitesse --------------------------------------------------------

#[component]
pub fn Compare() -> Html {
    let selected = use_state(|| 0usize);
    let typed = use_state(|| 0usize);
    let started = use_state(|| false);
    let window_ref = use_node_ref();
    let timer: Rc<RefCell<Option<Interval>>> = use_mut_ref(|| None);
    {
        let (started, window_ref) = (started.clone(), window_ref.clone());
        use_effect_with((), move |_| {
            let observer = window_ref
                .cast::<HtmlElement>()
                .map(|el| dom::on_first_visible(&el, 0.35, move || started.set(true)));
            move || {
                if let Some(o) = observer {
                    o.disconnect();
                }
            }
        });
    }
    {
        let (typed, timer) = (typed.clone(), timer.clone());
        use_effect_with((*selected, *started), move |&(selected, started)| {
            let total = EXAMPLES[selected].vitesse.chars().count();
            if !started {
                typed.set(0);
            } else if dom::reduced_motion() {
                typed.set(total);
            } else {
                let mut n = 0usize;
                typed.set(0);
                *timer.borrow_mut() = Some(Interval::new(16, move || {
                    n = (n + 3).min(total);
                    typed.set(n);
                }));
            }
            move || {
                timer.borrow_mut().take();
            }
        });
    }
    let example = &EXAMPLES[*selected];
    let done = *typed >= example.vitesse.chars().count();
    html! {
        <section class="section" id="express">
            <Head num="05" label="D'Express à Vitesse"
                  title="Vous savez déjà écrire du Vitesse."
                  lead={Some("Mêmes idées, mêmes noms, même façon de penser. La différence : un compilateur qui vérifie tout, et des performances natives.")} />
            <div class="compare__tabs" data-reveal="">
                { for EXAMPLES.iter().enumerate().map(|(i, ex)| {
                    let selected = selected.clone();
                    html! {
                        <button class={classes!("chip", (i == *selected).then_some("is-active"))}
                                onclick={Callback::from(move |_: MouseEvent| selected.set(i))}>
                            { ex.label }
                        </button>
                    }
                }) }
            </div>
            <div class="compare" ref={window_ref} data-reveal="">
                <div class="window window--dim">
                    <div class="window__bar"><span></span><span></span><span></span><em>{ "server.js — Express" }</em></div>
                    <Code code={example.express} />
                </div>
                <div class="compare__arrow" aria-hidden="true"><Icon name="arrow" /></div>
                <div class="window window--glow">
                    <div class="window__bar"><span></span><span></span><span></span><em>{ "main.rs — Vitesse" }</em></div>
                    <Code code={example.vitesse} limit={*typed} caret={!done} />
                </div>
            </div>
        </section>
    }
}

// ----- Démarrer -----------------------------------------------------------------------

#[component]
pub fn Start() -> Html {
    let steps: [(&str, &str, &str); 2] = [
        (
            "Ajoutez Vitesse",
            "Cargo.toml",
            "[dependencies.vitesse]\ngit = \"https://github.com/maxlestage/vitesse\"",
        ),
        (
            "Écrivez votre application",
            "src/main.rs",
            "use vitesse::prelude::*;\n\nfn main() -> std::io::Result<()> {\n    let mut app = App::new();\n    app.get(\"/\", |_| async { \"Hello World!\" });\n    app.run(3000)\n}",
        ),
    ];
    html! {
        <section class="section" id="demarrer">
            <Head num="06" label="Démarrer" title="En ligne en trente secondes." />
            <div class="steps">
                <span class="steps__line" data-reveal="" aria-hidden="true"></span>
                { for steps.iter().enumerate().map(|(i, (title, file, code))| html! {
                    <article class="step" data-reveal="" style={format!("--d:{}ms", i * 140)}>
                        <span class="step__num">{ i + 1 }</span>
                        <h3>{ *title }</h3>
                        <div class="window">
                            <div class="window__bar"><span></span><span></span><span></span><em>{ *file }</em></div>
                            <Code code={*code} />
                        </div>
                    </article>
                }) }
                <article class="step" data-reveal="" style="--d:280ms">
                    <span class="step__num">{ 3 }</span>
                    <h3>{ "Lancez-la" }</h3>
                    <div class="window terminal">
                        <div class="window__bar"><span></span><span></span><span></span><em>{ "Terminal" }</em></div>
                        <pre class="code">
                            <span class="term term--cmd" style="--l:0"><span class="t-punct">{ "$ " }</span>{ "cargo run --release" }</span>
                            <span class="term" style="--l:1"><span class="t-kw">{ "   Compiling" }</span>{ " vitesse v0.1.0" }</span>
                            <span class="term" style="--l:2"><span class="t-kw">{ "    Finished" }</span>{ " `release` profile [optimized]" }</span>
                            <span class="term" style="--l:3"><span class="t-kw">{ "     Running" }</span>{ " `target/release/app`" }</span>
                            <span class="term term--ok" style="--l:4">{ "⚡ Vitesse écoute sur http://localhost:3000" }</span>
                        </pre>
                    </div>
                </article>
            </div>
        </section>
    }
}

// ----- Appel à l'action ---------------------------------------------------------------

#[component]
pub fn Cta() -> Html {
    html! {
        <section class="cta">
            <div class="cta__bg" aria-hidden="true"></div>
            <h2 class="cta__title" data-reveal="">
                <span>{ "Prêt à aller" }</span>
                <span class="cta__outline" data-text="plus vite ?">{ "plus vite ?" }</span>
            </h2>
            <div class="cta__actions" data-reveal="" style="--d:150ms">
                <Magnetic href="#demarrer" class={classes!("btn", "btn--primary", "btn--lg")} cursor={Some(AttrValue::from("Go"))}>
                    <span>{ "Démarrer un projet" }</span><Icon name="arrow" />
                </Magnetic>
                <Magnetic href={GITHUB} external=true class={classes!("btn", "btn--ghost", "btn--lg")}>
                    <Icon name="github" /><span>{ "Étoiler sur GitHub" }</span>
                </Magnetic>
            </div>
        </section>
    }
}
