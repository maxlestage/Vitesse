//! Les écrans de la documentation : l'accueil (catégories et recherche) et
//! les pages (sommaire, contenu, « sur cette page », précédent / suivant).

use std::cell::Cell;
use std::rc::Rc;

use gloo_events::EventListener;
use gloo_timers::callback::Timeout;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Element, HtmlElement, HtmlInputElement, KeyboardEvent};
use yew::prelude::*;

use super::index::{self, CATEGORIES, PAGES, Page};
use super::render::{self, TocEntry, page_href};
use super::{fetch, search};
use crate::components::ui::Icon;
use crate::data::GITHUB;
use crate::dom;
use crate::i18n::{Lang, use_lang};

fn set_title(title: &str) {
    dom::document().set_title(title);
}

fn go(href: &str) {
    let _ = dom::window()
        .location()
        .set_hash(href.trim_start_matches('#'));
}

// ----- Recherche -------------------------------------------------------------------

#[derive(Properties, PartialEq)]
pub struct SearchProps {
    #[prop_or_default]
    pub large: bool,
}

#[component]
pub fn Search(props: &SearchProps) -> Html {
    let lang = use_lang().lang;
    let t = lang.texts();
    let query = use_state(String::new);
    let active = use_state(|| 0usize);
    let input = use_node_ref();
    let refresh = use_force_update();
    let prefetched_for = use_mut_ref(|| None::<Lang>);

    // Les pages sont téléchargées à la première recherche, pour chercher
    // aussi dans leur contenu.
    let prefetch = {
        let prefetched_for = prefetched_for.clone();
        Callback::from(move |_: ()| {
            if *prefetched_for.borrow() == Some(lang) {
                return;
            }
            *prefetched_for.borrow_mut() = Some(lang);
            let refresh = refresh.clone();
            spawn_local(async move {
                for page in &PAGES {
                    fetch::load(lang, page.slug).await;
                }
                refresh.force_update();
            });
        })
    };

    // « / » place le curseur dans la recherche.
    {
        let input = input.clone();
        use_effect_with((), move |_| {
            let listener = EventListener::new(&dom::window(), "keydown", move |e| {
                let Some(e) = e.dyn_ref::<KeyboardEvent>() else {
                    return;
                };
                let typing = dom::document()
                    .active_element()
                    .is_some_and(|el| matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA"));
                if e.key() == "/"
                    && !typing
                    && let Some(input) = input.cast::<HtmlInputElement>()
                {
                    e.prevent_default();
                    let _ = input.focus();
                }
            });
            move || drop(listener)
        });
    }

    let hits = search::search(lang, &query);
    let oninput = {
        let (query, active, prefetch) = (query.clone(), active.clone(), prefetch.clone());
        Callback::from(move |e: InputEvent| {
            if let Some(input) = e.target_dyn_into::<HtmlInputElement>() {
                query.set(input.value());
                active.set(0);
                prefetch.emit(());
            }
        })
    };
    let onfocus = {
        let prefetch = prefetch.clone();
        Callback::from(move |_: FocusEvent| prefetch.emit(()))
    };
    let targets: Vec<String> = hits
        .iter()
        .map(|h| page_href(h.slug, h.anchor.as_deref()))
        .collect();
    let onkeydown = {
        let (query, active, input) = (query.clone(), active.clone(), input.clone());
        let targets = targets.clone();
        Callback::from(move |e: KeyboardEvent| match e.key().as_str() {
            "ArrowDown" if !targets.is_empty() => {
                e.prevent_default();
                active.set((*active + 1) % targets.len());
            }
            "ArrowUp" if !targets.is_empty() => {
                e.prevent_default();
                active.set((*active + targets.len() - 1) % targets.len());
            }
            "Enter" => {
                if let Some(href) = targets.get(*active) {
                    go(href);
                    query.set(String::new());
                    if let Some(input) = input.cast::<HtmlElement>() {
                        let _ = input.blur();
                    }
                }
            }
            "Escape" => {
                query.set(String::new());
                if let Some(input) = input.cast::<HtmlElement>() {
                    let _ = input.blur();
                }
            }
            _ => {}
        })
    };
    let clear = {
        let query = query.clone();
        Callback::from(move |_: MouseEvent| query.set(String::new()))
    };
    html! {
        <div class={classes!("doc-search", props.large.then_some("doc-search--large"))}>
            <label class="doc-search__box">
                <Icon name="search" />
                <input ref={input} type="search" value={(*query).clone()} placeholder={t.docs_search}
                       aria-label={t.docs_search} autocomplete="off" spellcheck="false"
                       {oninput} {onkeydown} {onfocus} />
                <kbd title={t.docs_search_hint}>{ "/" }</kbd>
            </label>
            if !query.trim().is_empty() {
                <div class="doc-search__results" role="listbox">
                    if hits.is_empty() {
                        <p class="doc-search__empty">{ t.docs_no_results.replace("{}", query.trim()) }</p>
                    }
                    { for hits.iter().zip(targets.iter()).enumerate().map(|(i, (hit, href))| html! {
                        <a class={classes!("doc-hit", (i == *active).then_some("is-active"))}
                           href={href.clone()} onclick={clear.clone()} role="option">
                            <span class="doc-hit__title">
                                { hit.title }
                                if let Some(section) = &hit.section {
                                    <span class="doc-hit__section">{ " › " }{ section.clone() }</span>
                                }
                            </span>
                            if let Some((before, matched, after)) = &hit.snippet {
                                <span class="doc-hit__snippet">
                                    { before.clone() }<mark>{ matched.clone() }</mark>{ after.clone() }
                                </span>
                            }
                        </a>
                    }) }
                </div>
            }
        </div>
    }
}

// ----- Sommaire latéral ----------------------------------------------------------

#[derive(Properties, PartialEq)]
struct SidebarProps {
    current: &'static str,
    open: bool,
    on_close: Callback<()>,
}

#[component]
fn Sidebar(props: &SidebarProps) -> Html {
    let lang = use_lang().lang;
    let t = lang.texts();
    let close = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| on_close.emit(()))
    };
    html! {
        <aside class={classes!("doc-side", props.open.then_some("is-open"))}>
            <Search />
            <nav class="doc-side__nav" aria-label={t.docs_menu}>
                { for CATEGORIES.iter().enumerate().map(|(c, category)| html! {
                    <div class="doc-side__group">
                        <p class="doc-side__cat"><Icon name={category.icon} />{ lang.pick(category.title) }</p>
                        <ul>
                            { for PAGES.iter().filter(|p| p.category == c).map(|page| html! {
                                <li>
                                    <a href={page_href(page.slug, None)} onclick={close.clone()}
                                       class={classes!((page.slug == props.current).then_some("is-active"))}
                                       aria-current={(page.slug == props.current).then_some("page")}>
                                        { page.title(lang) }
                                    </a>
                                </li>
                            }) }
                        </ul>
                    </div>
                }) }
            </nav>
            <a class="doc-side__api" href="https://docs.rs/vitesse" target="_blank" rel="noopener">
                <Icon name="book" /><span>{ t.docs_api }</span>
            </a>
        </aside>
    }
}

// ----- « Sur cette page » ----------------------------------------------------------

#[derive(Properties, PartialEq)]
struct TocProps {
    slug: &'static str,
    entries: Rc<Vec<TocEntry>>,
}

/// Le plan de la page, avec la section en cours de lecture en surbrillance.
#[component]
fn Toc(props: &TocProps) -> Html {
    let t = use_lang().lang.texts();
    let active = use_state(String::new);
    {
        let active = active.clone();
        use_effect_with(props.entries.clone(), move |entries| {
            let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
            let update = move || {
                let doc = dom::document();
                let mut current = ids.first().cloned().unwrap_or_default();
                for id in &ids {
                    match doc.get_element_by_id(id) {
                        Some(el) if el.get_bounding_client_rect().top() < 150.0 => {
                            current = id.clone();
                        }
                        Some(_) => break,
                        None => {}
                    }
                }
                if *active != current {
                    active.set(current);
                }
            };
            update();
            let listener = EventListener::new(&dom::window(), "scroll", move |_| update());
            move || drop(listener)
        });
    }
    if props.entries.is_empty() {
        return html! {};
    }
    html! {
        <nav class="doc-toc" aria-label={t.docs_toc}>
            <p class="doc-toc__title">{ t.docs_toc }</p>
            <ul>
                { for props.entries.iter().map(|e| html! {
                    <li class={classes!((e.level == 3).then_some("is-sub"))}>
                        <a href={page_href(props.slug, Some(&e.id))}
                           class={classes!((*active == e.id).then_some("is-active"))}>
                            { e.text.clone() }
                        </a>
                    </li>
                }) }
            </ul>
        </nav>
    }
}

// ----- Une page --------------------------------------------------------------------

struct Loaded {
    html: AttrValue,
    title: String,
    toc: Rc<Vec<TocEntry>>,
}

enum State {
    Loading,
    Ready(Rc<Loaded>),
    Missing,
}

#[derive(Properties, PartialEq)]
pub struct DocsPageProps {
    pub slug: &'static str,
    #[prop_or_default]
    pub anchor: Option<String>,
}

fn loaded(markdown: &str, lang: Lang, slug: &'static str) -> State {
    let r = render::render(markdown, lang, slug);
    State::Ready(Rc::new(Loaded {
        html: AttrValue::from(r.html),
        title: r.title,
        toc: Rc::new(r.toc),
    }))
}

/// Copie le bloc de code dont on a cliqué le bouton « Copier ».
fn copy_code(e: MouseEvent) {
    let Some(target) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
        return;
    };
    let Ok(Some(button)) = target.closest(".doc-code__copy") else {
        return;
    };
    let code = button
        .parent_element()
        .and_then(|bar| bar.next_element_sibling())
        .and_then(|pre| pre.text_content())
        .unwrap_or_default();
    let _ = dom::window().navigator().clipboard().write_text(&code);
    let label = button.text_content();
    button.set_text_content(button.get_attribute("data-copied").as_deref());
    let _ = button.class_list().add_1("is-done");
    Timeout::new(1600, move || {
        button.set_text_content(label.as_deref());
        let _ = button.class_list().remove_1("is-done");
    })
    .forget();
}

#[component]
pub fn DocsPage(props: &DocsPageProps) -> Html {
    let ctx = use_lang();
    let lang = ctx.lang;
    let t = lang.texts();
    let slug = props.slug;
    let state = use_state(|| match fetch::cached(lang, slug) {
        Some(md) => loaded(&md, lang, slug),
        None => State::Loading,
    });
    let side_open = use_state(|| false);

    {
        let state = state.clone();
        use_effect_with((lang, slug), move |&(lang, slug)| {
            let cancelled = Rc::new(Cell::new(false));
            if let Some(md) = fetch::cached(lang, slug) {
                state.set(loaded(&md, lang, slug));
            } else {
                state.set(State::Loading);
                let cancelled = cancelled.clone();
                spawn_local(async move {
                    let md = fetch::load(lang, slug).await;
                    if cancelled.get() {
                        return;
                    }
                    state.set(match md {
                        Some(md) => loaded(&md, lang, slug),
                        None => State::Missing,
                    });
                });
            }
            move || cancelled.set(true)
        });
    }

    let page = index::find(slug).map(|(_, p)| p);
    let title = match &*state {
        State::Ready(l) if !l.title.is_empty() => l.title.clone(),
        _ => page.map_or(slug, |p| p.title(lang)).to_owned(),
    };
    {
        let title = format!("{title} — {}", t.docs_meta);
        use_effect_with(title, |title| set_title(title));
    }

    // Va à l'ancre demandée une fois la page affichée, sinon en haut.
    let ready = matches!(*state, State::Ready(_));
    use_effect_with(
        (slug, props.anchor.clone(), lang, ready),
        |(_, anchor, _, ready)| {
            if !*ready {
                return;
            }
            let target = anchor
                .as_deref()
                .and_then(|a| dom::document().get_element_by_id(a));
            match target {
                Some(el) => el.scroll_into_view(),
                None => dom::window().scroll_to_with_x_and_y(0.0, 0.0),
            }
        },
    );

    let position = index::find(slug).map(|(i, _)| i);
    let prev = position.and_then(|i| i.checked_sub(1)).map(|i| &PAGES[i]);
    let next = position.and_then(|i| PAGES.get(i + 1));
    let category = page.map(|p| lang.pick(CATEGORIES[p.category].title));

    let toggle = {
        let side_open = side_open.clone();
        Callback::from(move |_: MouseEvent| side_open.set(!*side_open))
    };
    let close = {
        let side_open = side_open.clone();
        Callback::from(move |_: ()| side_open.set(false))
    };
    let close_click = {
        let close = close.clone();
        Callback::from(move |_: MouseEvent| close.emit(()))
    };
    let read_english = {
        let set = ctx.set.clone();
        Callback::from(move |e: MouseEvent| {
            e.prevent_default();
            set.emit(Lang::En);
        })
    };
    let pager = |page: &'static Page, label: &'static str, class: &'static str| {
        html! {
            <a class={classes!("doc-pager__link", class)} href={page_href(page.slug, None)}>
                <span class="doc-pager__label">{ label }</span>
                <span class="doc-pager__title">{ page.title(lang) }</span>
            </a>
        }
    };

    html! {
        <div class="docs">
            <div class="docs__bar">
                <button class="docs__menu" onclick={toggle} aria-expanded={(*side_open).to_string()}>
                    <Icon name="menu" /><span>{ t.docs_menu }</span>
                </button>
                <span class="docs__bar-title">{ page.map_or(slug, |p| p.title(lang)) }</span>
            </div>
            <Sidebar current={slug} open={*side_open} on_close={close} />
            if *side_open {
                <div class="docs__scrim" onclick={close_click}></div>
            }
            <article class="docs__main">
                <p class="docs__crumb">
                    <a href="#/docs">{ t.docs_title }</a>
                    if let Some(category) = category {
                        <span>{ " / " }{ category }</span>
                    }
                </p>
                { match &*state {
                    State::Loading => html! {
                        <div class="doc-skeleton" aria-busy="true" aria-label={t.docs_loading}>
                            <span></span><span></span><span></span><span></span><span></span>
                        </div>
                    },
                    State::Missing => html! {
                        <div class="doc-missing card">
                            <h1 class="doc-title">{ title.clone() }</h1>
                            <p>{ t.docs_missing }</p>
                            if lang != Lang::En {
                                <a class="btn btn--primary" href={page_href(slug, None)} onclick={read_english}>
                                    <span>{ t.docs_read_en }</span><Icon name="arrow" />
                                </a>
                            }
                        </div>
                    },
                    State::Ready(page) => html! {
                        <div class="doc-body" key={format!("{}-{slug}", lang.code())} onclick={Callback::from(copy_code)}>
                            { Html::from_html_unchecked(page.html.clone()) }
                        </div>
                    },
                } }
                <div class="doc-foot">
                    <a class="doc-edit" target="_blank" rel="noopener"
                       href={format!("{GITHUB}/edit/master/docs/{}/{slug}.md", lang.code())}>
                        <Icon name="edit" /><span>{ t.docs_edit }</span>
                    </a>
                </div>
                <nav class="doc-pager">
                    if let Some(prev) = prev { { pager(prev, t.docs_prev, "doc-pager__link--prev") } }
                    if let Some(next) = next { { pager(next, t.docs_next, "doc-pager__link--next") } }
                </nav>
            </article>
            <aside class="docs__toc">
                if let State::Ready(page) = &*state {
                    <Toc slug={slug} entries={page.toc.clone()} />
                }
            </aside>
        </div>
    }
}

// ----- Accueil de la documentation -------------------------------------------------

const QUICK: [(&str, &str); 3] = [
    ("first-app", "engine"),
    ("from-express", "layers"),
    ("heroku-mobile", "phone"),
];

#[component]
pub fn DocsHome() -> Html {
    let lang = use_lang().lang;
    let t = lang.texts();
    use_effect_with(lang, move |lang| {
        set_title(lang.texts().docs_meta);
    });
    use_effect_with((), |_| dom::window().scroll_to_with_x_and_y(0.0, 0.0));
    html! {
        <section class="docs-home">
            <div class="docs-home__hero">
                <div class="docs-home__glow" aria-hidden="true"></div>
                <p class="eyebrow"><span class="eyebrow__num">{ "Vitesse" }</span>{ lang.name() }</p>
                <h1 class="docs-home__title">{ t.docs_title }</h1>
                <p class="docs-home__lead">{ t.docs_lead }</p>
                <Search large=true />
                <p class="docs-home__hint">{ t.docs_search_hint }</p>
            </div>
            <div class="docs-home__quick">
                <h2 class="docs-home__h2">{ t.docs_quick }</h2>
                <div class="docs-home__quick-grid">
                    { for QUICK.iter().filter_map(|(slug, icon)| index::find(slug).map(|(_, p)| (p, *icon))).map(|(page, icon)| html! {
                        <a class="doc-quick card" href={page_href(page.slug, None)}>
                            <span class="doc-quick__icon"><Icon name={icon} /></span>
                            <span class="doc-quick__title">{ page.title(lang) }</span>
                            <span class="doc-quick__text">{ page.summary(lang) }</span>
                            <span class="doc-quick__arrow"><Icon name="arrow" /></span>
                        </a>
                    }) }
                </div>
            </div>
            <div class="docs-home__cats">
                { for CATEGORIES.iter().enumerate().map(|(c, category)| {
                    let pages: Vec<&Page> = PAGES.iter().filter(|p| p.category == c).collect();
                    html! {
                        <article class="doc-cat card">
                            <div class="doc-cat__head">
                                <span class="doc-cat__icon"><Icon name={category.icon} /></span>
                                <h2>{ lang.pick(category.title) }</h2>
                                <span class="doc-cat__count">{ t.docs_pages.replace("{}", &pages.len().to_string()) }</span>
                            </div>
                            <ul>
                                { for pages.iter().map(|page| html! {
                                    <li>
                                        <a href={page_href(page.slug, None)}>
                                            <strong>{ page.title(lang) }</strong>
                                            <span>{ page.summary(lang) }</span>
                                        </a>
                                    </li>
                                }) }
                            </ul>
                        </article>
                    }
                }) }
            </div>
        </section>
    }
}
