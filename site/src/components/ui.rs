//! Les petits composants animés réutilisés partout.

use std::cell::RefCell;
use std::rc::Rc;

use gloo_timers::callback::{Interval, Timeout};
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;
use yew::prelude::*;

use crate::dom::{self, RafLoop};
use crate::highlight::{self, Kind};
use crate::i18n::{self, use_lang};

// ----- Bouton magnétique ----------------------------------------------------

#[derive(Properties, PartialEq)]
pub struct MagneticProps {
    pub href: AttrValue,
    #[prop_or_default]
    pub class: Classes,
    #[prop_or(0.35)]
    pub strength: f64,
    #[prop_or_default]
    pub external: bool,
    #[prop_or_default]
    pub cursor: Option<AttrValue>,
    #[prop_or_default]
    pub children: Html,
}

/// Un lien qui se laisse attirer par le curseur.
#[component]
pub fn Magnetic(props: &MagneticProps) -> Html {
    let strength = props.strength;
    let onmousemove = Callback::from(move |e: MouseEvent| {
        let Some(el) = e
            .current_target()
            .and_then(|t| t.dyn_into::<HtmlElement>().ok())
        else {
            return;
        };
        let rect = el.get_bounding_client_rect();
        let x = f64::from(e.client_x()) - rect.left() - rect.width() / 2.0;
        let y = f64::from(e.client_y()) - rect.top() - rect.height() / 2.0;
        dom::set_var(&el, "--mx", &format!("{:.1}px", x * strength));
        dom::set_var(&el, "--my", &format!("{:.1}px", y * strength));
    });
    let onmouseleave = Callback::from(|e: MouseEvent| {
        let Some(el) = e
            .current_target()
            .and_then(|t| t.dyn_into::<HtmlElement>().ok())
        else {
            return;
        };
        dom::set_var(&el, "--mx", "0px");
        dom::set_var(&el, "--my", "0px");
    });
    let (target, rel) = if props.external {
        (Some("_blank"), Some("noopener"))
    } else {
        (None, None)
    };
    html! {
        <a class={classes!("magnetic", props.class.clone())} href={props.href.clone()}
           target={target} rel={rel} data-cursor={props.cursor.clone()}
           {onmousemove} {onmouseleave}>
            <span class="magnetic__inner">{ props.children.clone() }</span>
        </a>
    }
}

// ----- Texte brouillé au survol ---------------------------------------------

const GLYPHS: &[char] = &[
    '#', '%', '&', '*', '+', '=', '/', '<', '>', '?', '0', '1', '⚡', '_',
];

#[derive(Properties, PartialEq)]
pub struct ScrambleProps {
    pub text: AttrValue,
}

/// Un texte dont les lettres se mélangent puis se remettent en place au survol.
#[component]
pub fn Scramble(props: &ScrambleProps) -> Html {
    let shown = use_state(|| props.text.to_string());
    let timer: Rc<RefCell<Option<Interval>>> = use_mut_ref(|| None);
    {
        // Le texte change avec la langue.
        let (shown, timer) = (shown.clone(), timer.clone());
        use_effect_with(props.text.clone(), move |text| {
            timer.borrow_mut().take();
            shown.set(text.to_string());
        });
    }
    let onmouseenter = {
        let shown = shown.clone();
        let timer = timer.clone();
        let text: Vec<char> = props.text.chars().collect();
        Callback::from(move |_: MouseEvent| {
            if dom::reduced_motion() {
                return;
            }
            let shown = shown.clone();
            let text = text.clone();
            let frame = Rc::new(RefCell::new(0usize));
            let total = text.len() * 2 + 4;
            let interval = Interval::new(28, move || {
                let f = {
                    let mut f = frame.borrow_mut();
                    *f += 1;
                    *f
                };
                let revealed = f.saturating_sub(4) / 2;
                let s: String = text
                    .iter()
                    .enumerate()
                    .map(|(i, &c)| {
                        if i < revealed || c == ' ' || f >= total {
                            c
                        } else {
                            GLYPHS[(dom::random() * GLYPHS.len() as f64) as usize % GLYPHS.len()]
                        }
                    })
                    .collect();
                shown.set(s);
            });
            let stop = timer.clone();
            *timer.borrow_mut() = Some(interval);
            Timeout::new((total as u32 + 1) * 28, move || {
                stop.borrow_mut().take();
            })
            .forget();
        })
    };
    html! { <span class="scramble" aria-label={props.text.clone()} {onmouseenter}>{ (*shown).clone() }</span> }
}

// ----- Compteur animé -------------------------------------------------------

#[derive(Properties, PartialEq)]
pub struct CounterProps {
    pub value: f64,
    #[prop_or_default]
    pub decimals: usize,
    #[prop_or_default]
    pub prefix: AttrValue,
    #[prop_or_default]
    pub suffix: AttrValue,
    #[prop_or(1800.0)]
    pub duration: f64,
}

/// Un nombre qui défile jusqu'à sa valeur quand il apparaît à l'écran.
#[component]
pub fn Counter(props: &CounterProps) -> Html {
    let lang = use_lang().lang;
    let node = use_node_ref();
    let anim: Rc<RefCell<Option<RafLoop>>> = use_mut_ref(|| None);
    {
        let node = node.clone();
        let (value, decimals, duration) = (props.value, props.decimals, props.duration);
        let (prefix, suffix) = (props.prefix.to_string(), props.suffix.to_string());
        use_effect_with((value.to_bits(), lang), move |_| {
            let mut observer = None;
            if let Some(el) = node.cast::<HtmlElement>() {
                let render = move |el: &HtmlElement, v: f64| {
                    el.set_text_content(Some(&format!(
                        "{prefix}{}{suffix}",
                        i18n::number(lang, v, decimals)
                    )));
                };
                if dom::reduced_motion() {
                    render(&el, value);
                } else {
                    render(&el, 0.0);
                    let target = el.clone();
                    observer = Some(dom::on_first_visible(&el, 0.4, move || {
                        let mut start = None;
                        *anim.borrow_mut() = Some(dom::raf_loop(move |t| {
                            let t0 = *start.get_or_insert(t);
                            let p = ((t - t0) / duration).min(1.0);
                            // Décélération exponentielle.
                            let eased = if p >= 1.0 {
                                1.0
                            } else {
                                1.0 - 2f64.powf(-10.0 * p)
                            };
                            render(&target, value * eased);
                            p < 1.0
                        }));
                    }));
                }
            }
            move || {
                if let Some(o) = observer {
                    o.disconnect();
                }
            }
        });
    }
    html! { <span class="counter" ref={node}>{ format!("{}0{}", props.prefix, props.suffix) }</span> }
}

// ----- Bouton copier --------------------------------------------------------

#[derive(Properties, PartialEq)]
pub struct CopyProps {
    pub text: AttrValue,
}

#[component]
pub fn CopyButton(props: &CopyProps) -> Html {
    let t = use_lang().lang.texts();
    let copied = use_state(|| false);
    let onclick = {
        let copied = copied.clone();
        let text = props.text.clone();
        Callback::from(move |_: MouseEvent| {
            let _ = dom::window().navigator().clipboard().write_text(&text);
            copied.set(true);
            let copied = copied.clone();
            Timeout::new(1600, move || copied.set(false)).forget();
        })
    };
    html! {
        <button class={classes!("copy", copied.then_some("copy--done"))} {onclick}
                data-cursor={t.copy} aria-label={t.copy_aria}>
            if *copied { { t.copied } } else { { t.copy } }
        </button>
    }
}

// ----- Texte découpé en lettres ---------------------------------------------

#[derive(Properties, PartialEq)]
pub struct SplitProps {
    pub text: AttrValue,
    #[prop_or_default]
    pub class: Classes,
    /// Décalage (en lettres) pour enchaîner plusieurs lignes.
    #[prop_or_default]
    pub offset: usize,
}

/// Chaque lettre monte à son tour (l'animation est en CSS).
#[component]
pub fn Split(props: &SplitProps) -> Html {
    let words: Vec<&str> = props.text.split(' ').collect();
    let mut index = props.offset;
    html! {
        <span class={classes!("split", props.class.clone())} aria-label={props.text.clone()}>
            { for words.iter().enumerate().map(|(w, word)| {
                let chars: Vec<Html> = word.chars().map(|c| {
                    let i = index;
                    index += 1;
                    html! { <span class="ch" aria-hidden="true" style={format!("--i:{i}")}>{ c }</span> }
                }).collect();
                index += 1;
                html! {
                    <>
                        <span class="word">{ for chars }</span>
                        if w + 1 < words.len() { { " " } }
                    </>
                }
            }) }
        </span>
    }
}

// ----- Mots qui défilent ----------------------------------------------------

#[derive(Properties, PartialEq)]
pub struct RotatingProps {
    pub words: Vec<AttrValue>,
}

#[component]
pub fn Rotating(props: &RotatingProps) -> Html {
    let index = use_state(|| 0usize);
    {
        let index = index.clone();
        let len = props.words.len();
        use_effect_with(len, move |_| {
            let interval = (!dom::reduced_motion()).then(|| {
                let mut i = 0;
                Interval::new(2300, move || {
                    i = (i + 1) % len;
                    index.set(i);
                })
            });
            move || drop(interval)
        });
    }
    html! {
        <span class="rotating">
            <span class="rotating__track" style={format!("--idx:{}", *index)}>
                { for props.words.iter().enumerate().map(|(i, w)| html! {
                    <span class={classes!("rotating__word", (i == *index).then_some("is-active"))}>{ w.clone() }</span>
                }) }
            </span>
        </span>
    }
}

// ----- Icônes ---------------------------------------------------------------

#[derive(Properties, PartialEq)]
pub struct IconProps {
    pub name: AttrValue,
}

#[component]
pub fn Icon(props: &IconProps) -> Html {
    let paths: &[&str] = match props.name.as_str() {
        "engine" => &["M13 2 4 14h7l-1 8 9-12h-7z"],
        "cores" => &[
            "M4 4h6v6H4z",
            "M14 4h6v6h-6z",
            "M4 14h6v6H4z",
            "M14 14h6v6h-6z",
        ],
        "tree" => &[
            "M12 3v6",
            "M12 9 6 15",
            "M12 9l6 6",
            "M6 15v4",
            "M18 15v4",
            "M12 9v10",
        ],
        "layers" => &["m12 3 9 5-9 5-9-5z", "m3 13 9 5 9-5"],
        "files" => &["M14 3H6v18h12V7z", "M14 3v4h4", "M9 13h6", "M9 17h6"],
        "shield" => &[
            "M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6z",
            "m9 12 2 2 4-4",
        ],
        "flask" => &["M9 3h6", "M10 3v6L4 20h16L14 9V3", "M7 15h10"],
        "power" => &["M12 3v9", "M6.3 7.5a8 8 0 1 0 11.4 0"],
        "github" => &[
            "M9 19c-5 1.5-5-2.5-7-3m14 6v-3.9a3.4 3.4 0 0 0-.9-2.6c3-.3 6.1-1.5 6.1-6.6a5.1 5.1 0 0 0-1.4-3.6 4.8 4.8 0 0 0-.1-3.5s-1.1-.3-3.6 1.4a12.3 12.3 0 0 0-6.4 0C6.2 1.6 5.1 1.9 5.1 1.9a4.8 4.8 0 0 0-.1 3.5A5.1 5.1 0 0 0 3.6 9c0 5.1 3.1 6.3 6.1 6.6a3.4 3.4 0 0 0-.9 2.6V22",
        ],
        "arrow" => &["M5 12h14", "m13 6 6 6-6 6"],
        "up" => &["M12 19V5", "m6 11 6-6 6 6"],
        "search" => &["M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14z", "m20 20-4.2-4.2"],
        "edit" => &["M4 20h4L19 9l-4-4L4 16z", "m13.5 6.5 4 4"],
        "book" => &[
            "M4 5.5A2.5 2.5 0 0 1 6.5 3H20v15H6.5A2.5 2.5 0 0 0 4 20.5z",
            "M4 20.5A2.5 2.5 0 0 0 6.5 23H20v-5",
            "M8 7h8",
        ],
        "phone" => &[
            "M8 2h8a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z",
            "M11 18h2",
        ],
        "menu" => &["M4 7h16", "M4 12h16", "M4 17h10"],
        "globe" => &[
            "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z",
            "M3 12h18",
            "M12 3c2.5 2.5 3.8 5.5 3.8 9s-1.3 6.5-3.8 9c-2.5-2.5-3.8-5.5-3.8-9S9.5 5.5 12 3z",
        ],
        _ => &[],
    };
    html! {
        <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"
             stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            { for paths.iter().map(|d| html! { <path d={*d} /> }) }
        </svg>
    }
}

/// Le logo : un éclair.
#[component]
pub fn Logo() -> Html {
    html! {
        <svg class="logo-mark" viewBox="0 0 32 32" aria-hidden="true">
            <defs>
                <linearGradient id="logo-grad" x1="0" y1="0" x2="1" y2="1">
                    <stop offset="0%" stop-color="#ffd23f" />
                    <stop offset="55%" stop-color="#ff7a1a" />
                    <stop offset="100%" stop-color="#ff2e63" />
                </linearGradient>
            </defs>
            <path d="M18.5 2 6 18h8.5l-2 12L26 13h-8.7z" fill="url(#logo-grad)" />
        </svg>
    }
}

// ----- Bloc de code ---------------------------------------------------------

#[derive(Properties, PartialEq)]
pub struct CodeProps {
    pub code: AttrValue,
    /// Nombre de caractères affichés (effet machine à écrire).
    #[prop_or(usize::MAX)]
    pub limit: usize,
    #[prop_or_default]
    pub caret: bool,
}

#[component]
pub fn Code(props: &CodeProps) -> Html {
    let tokens = use_memo(props.code.clone(), |code| highlight::tokenize(code));
    let shown = if props.limit == usize::MAX {
        (*tokens).clone()
    } else {
        highlight::truncate(&tokens, props.limit)
    };
    html! {
        <pre class="code"><code>
            { for shown.into_iter().map(|(kind, text)| {
                if kind == Kind::Plain { html! { { text } } }
                else { html! { <span class={kind.class()}>{ text }</span> } }
            }) }
            if props.caret { <span class="caret" aria-hidden="true"></span> }
        </code></pre>
    }
}
