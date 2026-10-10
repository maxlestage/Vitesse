//! Les îlots : les parties interactives du site. Chacun est rendu par le serveur
//! comme n'importe quelle vue et, compilé en WebAssembly, hydraté dans le navigateur
//! à partir du même code. Le reste de la page est du HTML statique.

use active::prelude::*;

use crate::data::{GITHUB, SCENARIOS, SERVERS};
use crate::highlight::{to_html, tokenize, truncate};
use crate::i18n::{Lang, number};
use crate::icons::icon;
use crate::labels::labels;
use crate::routes::home_href;
use crate::search::Hit;

/// La langue des propriétés d'un îlot (`"fr"`, `"fr large"`…).
fn lang(props: &str) -> Lang {
    props.get(..2).and_then(Lang::parse).unwrap_or(Lang::En)
}

/// Vrai dans le navigateur : l'état de départ y attend l'animation, alors que le
/// serveur rend l'état final (lisible sans JavaScript et par les moteurs de
/// recherche).
const BROWSER: bool = cfg!(target_arch = "wasm32");

/// Tous les îlots, sous le nom donné à [`island`].
pub const ISLANDS: [(&str, active::IslandFn); 6] = [
    ("warp", warp),
    ("bench", bench),
    ("compare", compare),
    ("search", search),
    ("clock", clock),
    ("root", root),
];

// ----- Fond du héros -------------------------------------------------------------------

/// Un champ d'étoiles en « vitesse lumière », dessiné en Rust sur un canvas.
pub fn warp(_props: &str) -> Node {
    canvas()
        .class("warp")
        .attr("aria-hidden", "true")
        .on_mount(|_canvas| {
            #[cfg(target_arch = "wasm32")]
            crate::client::warp::start(_canvas);
        })
        .into()
}

// ----- Benchmark -----------------------------------------------------------------------

/// Les scénarios du benchmark : un onglet par scénario, des barres qui s'allongent
/// et des compteurs qui défilent quand le panneau apparaît.
pub fn bench(props: &str) -> Node {
    let lang = lang(props);
    let l = labels(lang);
    let selected = use_state(0usize);
    let visible = use_state(!BROWSER);
    // Avancement des compteurs, de 0 à 1.
    let progress = use_state(1.0f64);
    let count = SCENARIOS.len();
    fragment([
        Node::from(
            div()
                .class("bench__tabs")
                .attr("role", "tablist")
                .attr("data-reveal", "")
                .attr_dyn("style", move || {
                    format!("--tab:{}; --tabs:{count}", selected.get())
                })
                .child(span().class("bench__indicator").attr("aria-hidden", "true"))
                .children(l.scenarios.iter().enumerate().map(|(i, scenario)| {
                    button()
                        .class("bench__tab")
                        .class_if("is-active", move || selected.get() == i)
                        .attr("type", "button")
                        .attr("role", "tab")
                        .attr_dyn("aria-selected", move || (selected.get() == i).to_string())
                        .on_click(move |_| {
                            if selected.get() != i {
                                selected.set(i);
                                #[cfg(target_arch = "wasm32")]
                                crate::client::count_up(progress, 1400.0);
                            }
                        })
                        .text(scenario.title)
                })),
        ),
        div()
            .class("bench__panel card")
            .class_if("is-visible", move || visible.get())
            .attr("data-reveal", "")
            .on_mount(move |_panel| {
                #[cfg(target_arch = "wasm32")]
                crate::client::when_visible(_panel, 0.1, move || {
                    visible.set(true);
                    crate::client::count_up(progress, 1400.0);
                });
            })
            .child(
                div()
                    .class("bench__meta")
                    .child(
                        p().class("bench__detail")
                            .text_dyn(move || l.scenarios[selected.get()].text.to_owned()),
                    )
                    .child(
                        div()
                            .class("bench__badges")
                            .child(span().class("pill pill--hot").text_dyn(move || {
                                let s = &SCENARIOS[selected.get()];
                                let ratio = number(lang, s.rps[4] / s.rps[3], 2);
                                format!("×{ratio} {} actix-web", l.bench_vs)
                            }))
                            .child(span().class("pill").text_dyn(move || {
                                let s = &SCENARIOS[selected.get()];
                                let ratio = number(lang, s.rps[4] / s.rps[2], 1);
                                format!("×{ratio} {} axum", l.bench_vs)
                            })),
                    ),
            )
            .child(
                div()
                    .class("bench__rows")
                    .children(SERVERS.iter().enumerate().map(|(i, name)| {
                        let ours = i == SERVERS.len() - 1;
                        div()
                            .class(if ours { "bar bar--ours" } else { "bar" })
                            .style(format!("--i:{i}"))
                            .child(span().class("bar__name").text(*name))
                            .child(span().class("bar__track").child(
                                span().class("bar__fill").attr_dyn("style", move || {
                                    let s = &SCENARIOS[selected.get()];
                                    let max = s.rps.iter().copied().fold(0.0, f64::max);
                                    let width = if visible.get() {
                                        (s.rps[i] / max * 100.0).max(0.6)
                                    } else {
                                        0.0
                                    };
                                    format!("width:{width:.2}%")
                                }),
                            ))
                            .child(
                                span()
                                    .class("bar__value")
                                    .child(span().class("counter").text_dyn(move || {
                                        let rps = SCENARIOS[selected.get()].rps[i];
                                        number(lang, rps * progress.get(), 0)
                                    }))
                                    .child(small().text(" req/s")),
                            )
                            .child(span().class("bar__cpu").text_dyn(move || {
                                let cpu = SCENARIOS[selected.get()].cpu[i];
                                let decimals = if cpu < 10.0 { 1 } else { 0 };
                                format!("{} µs", number(lang, cpu, decimals))
                            }))
                    })),
            )
            .child(
                p().class("bench__note").text(l.bench_note).child(
                    a().href(format!("{GITHUB}/tree/master/bench"))
                        .attr("target", "_blank")
                        .attr("rel", "noopener")
                        .text(l.bench_reproduce),
                ),
            )
            .into(),
    ])
}

// ----- D'Express à Vitesse ---------------------------------------------------------------

/// Une fenêtre d'éditeur : trois pastilles, le nom du fichier et le code.
fn window(class: &'static str, file: &'static str, code: Element) -> Element {
    div()
        .class(class)
        .child(
            div()
                .class("window__bar")
                .child(span())
                .child(span())
                .child(span())
                .child(em().text(file)),
        )
        .child(pre().class("code").child(code))
}

/// Le même programme en Express et en Vitesse ; le code Vitesse s'écrit tout seul
/// quand il apparaît, et à chaque changement d'exemple.
pub fn compare(props: &str) -> Node {
    let l = labels(lang(props));
    let selected = use_state(0usize);
    let typed = use_state(if BROWSER { 0 } else { usize::MAX });
    let total = move |i: usize| l.examples[i].vitesse.chars().count();
    fragment([
        Node::from(
            div()
                .class("compare__tabs")
                .attr("data-reveal", "")
                .children(l.examples.iter().enumerate().map(|(i, example)| {
                    button()
                        .class("chip")
                        .class_if("is-active", move || selected.get() == i)
                        .attr("type", "button")
                        .attr_dyn("aria-pressed", move || (selected.get() == i).to_string())
                        .on_click(move |_| {
                            selected.set(i);
                            #[cfg(target_arch = "wasm32")]
                            crate::client::type_code(typed, total(i));
                        })
                        .text(example.label)
                })),
        ),
        div()
            .class("compare")
            .attr("data-reveal", "")
            .on_mount(move |_compare| {
                #[cfg(target_arch = "wasm32")]
                crate::client::when_visible(_compare, 0.35, move || {
                    crate::client::type_code(typed, total(selected.get()));
                });
            })
            .child(window(
                "window window--dim",
                "server.js — Express",
                code().html_dyn(move || to_html(&tokenize(l.examples[selected.get()].express))),
            ))
            .child(
                div()
                    .class("compare__arrow")
                    .attr("aria-hidden", "true")
                    .child(icon("arrow")),
            )
            .child(window(
                "window window--glow",
                "main.rs — Vitesse",
                code().html_dyn(move || {
                    let i = selected.get();
                    let shown = typed.get();
                    let mut html = to_html(&truncate(&tokenize(l.examples[i].vitesse), shown));
                    if shown < total(i) {
                        html.push_str(r#"<span class="caret" aria-hidden="true"></span>"#);
                    }
                    html
                }),
            ))
            .into(),
    ])
}

// ----- Recherche dans la documentation ------------------------------------------------------

/// Un champ de recherche et ses résultats. L'index de la langue (`/search/{lang}.txt`)
/// est téléchargé au premier focus, puis chaque touche cherche dedans sans réseau.
/// « / » place le curseur dans le champ.
pub fn search(props: &str) -> Node {
    let lang = lang(props);
    let large = props.ends_with(" large");
    let l = labels(lang);
    let query = use_state(String::new());
    let hits = use_state(Vec::<Hit>::new());
    let loaded = use_state(false);
    let active = use_state(0usize);
    div()
        .class("doc-search")
        .class(if large { "doc-search--large" } else { "" })
        .child(
            label()
                .class("doc-search__box")
                .child(icon("search"))
                .child(
                    input()
                        .attr("type", "search")
                        .attr("placeholder", l.search)
                        .attr("aria-label", l.search)
                        .attr("autocomplete", "off")
                        .attr("spellcheck", "false")
                        .attr("enterkeyhint", "search")
                        .attr_dyn("value", move || query.get())
                        .on_input(move |e| {
                            active.set(0);
                            query.set(e.value());
                            #[cfg(target_arch = "wasm32")]
                            crate::client::search::refresh(lang, query, hits, loaded);
                        })
                        .on("focus", move |_| {
                            #[cfg(target_arch = "wasm32")]
                            crate::client::search::refresh(lang, query, hits, loaded);
                        })
                        .on("keydown", move |e| match e.key().as_str() {
                            "ArrowDown" | "ArrowUp" => {
                                e.prevent_default();
                                let n = hits.with(Vec::len);
                                if n > 0 {
                                    let step = if e.key() == "ArrowDown" { 1 } else { n - 1 };
                                    active.update(|a| *a = (*a + step) % n);
                                }
                            }
                            "Enter" => {
                                if let Some(href) =
                                    hits.with(|h| h.get(active.get()).map(|h| h.href.clone()))
                                {
                                    e.prevent_default();
                                    #[cfg(target_arch = "wasm32")]
                                    crate::client::search::navigate(&href);
                                    let _ = href;
                                }
                            }
                            "Escape" => {
                                query.set(String::new());
                                #[cfg(target_arch = "wasm32")]
                                crate::client::search::blur(&e);
                            }
                            _ => {}
                        })
                        .on_mount(|_input| {
                            #[cfg(target_arch = "wasm32")]
                            crate::client::search::shortcut(_input);
                        }),
                )
                .child(kbd().attr("title", l.search_hint).text("/")),
        )
        .child(
            div()
                .class("doc-search__results")
                .attr("role", "listbox")
                .attr("aria-label", l.search)
                .bool_attr("hidden", move || query.with(|q| q.trim().is_empty()))
                .children_dyn(move || {
                    let q = query.get();
                    if q.trim().is_empty() || !loaded.get() {
                        return Vec::new();
                    }
                    let list = hits.get();
                    if list.is_empty() {
                        let text = l.no_results.replace("{}", q.trim());
                        return vec![p().class("doc-search__empty").text(text).into()];
                    }
                    list.into_iter()
                        .enumerate()
                        .map(|(i, hit)| {
                            a().class("doc-hit")
                                .class_if("is-active", move || active.get() == i)
                                .href(hit.href)
                                .attr("role", "option")
                                .child(span().class("doc-hit__title").text(hit.title).child(
                                    hit.section.map(|s| {
                                        span().class("doc-hit__section").text(format!(" › {s}"))
                                    }),
                                ))
                                .child(hit.snippet.map(|(before, matched, after)| {
                                    span()
                                        .class("doc-hit__snippet")
                                        .text(before)
                                        .child(mark().text(matched))
                                        .text(after)
                                }))
                                .into()
                        })
                        .collect()
                }),
        )
        .into()
}

// ----- Horloge du pied de page ---------------------------------------------------------------

/// L'heure locale du visiteur, à la seconde.
pub fn clock(props: &str) -> Node {
    let lang = lang(props);
    let l = labels(lang);
    let time = use_state(String::from("--:--:--"));
    span()
        .class("clock")
        .child(span().class("clock__dot"))
        .text_dyn(move || format!("{} {}", l.clock, time.get()))
        .on_mount(move |_| {
            #[cfg(target_arch = "wasm32")]
            crate::client::clock(lang, time);
        })
        .into()
}

// ----- Racine de l'export statique -------------------------------------------------------------

/// La page `/` d'un hébergement statique, où aucun serveur ne lit
/// `Accept-Language` : les trois langues, et le navigateur part aussitôt vers celle
/// choisie la dernière fois (ou la sienne). Les anciennes adresses en
/// `#/docs/…` mènent à leur nouvelle page.
pub fn root(_props: &str) -> Node {
    ul().class("root__langs")
        .children(Lang::ALL.map(|l| {
            li().child(
                a().href(home_href(l, ""))
                    .attr("hreflang", l.code())
                    .attr("lang", l.code())
                    .attr("data-lang", l.code())
                    .child(span().class("root__code").text(l.code().to_uppercase()))
                    .child(span().text(l.name())),
            )
        }))
        .on_mount(|_| {
            #[cfg(target_arch = "wasm32")]
            crate::client::legacy::redirect_root();
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn islands_render_on_the_server() {
        for (name, island_fn) in ISLANDS {
            let html = active::island(name, "fr", island_fn).render();
            assert!(
                html.starts_with(&format!(
                    r#"<active-island data-island="{name}" data-props="fr">"#
                )),
                "{name}"
            );
        }
    }

    #[test]
    fn the_server_renders_the_final_state() {
        let bench = active::island("bench", "en", bench).render();
        // Les barres sont pleines et les compteurs à leur valeur.
        assert!(bench.contains("290,478"), "{bench}");
        assert!(bench.contains("width:100.00%"));
        assert!(bench.contains("×1.62 vs actix-web"));
        assert_eq!(bench.matches("role=\"tab\"").count(), SCENARIOS.len());
        let compare = active::island("compare", "fr", compare).render();
        // Tout le code Vitesse, sans curseur.
        assert!(compare.contains("&quot;Hello World!&quot;"), "{compare}");
        assert!(!compare.contains("caret"));
        let search = active::island("search", "es large", search).render();
        assert!(search.contains("doc-search--large"));
        assert!(search.contains("Buscar en la documentación"));
        assert!(search.contains(" hidden"));
        let root = active::island("root", "", root).render();
        assert!(root.contains(r#"href="/fr/""#));
    }

    #[test]
    fn props_give_the_language() {
        assert_eq!(lang("fr"), Lang::Fr);
        assert_eq!(lang("es large"), Lang::Es);
        assert_eq!(lang(""), Lang::En);
        assert_eq!(lang("de"), Lang::En);
    }
}
