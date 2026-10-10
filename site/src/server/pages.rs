//! Toutes les pages du site, écrites avec active : rendues sur le serveur avec leurs
//! balises SEO complètes ; seuls les îlots (`crate::islands`) sont hydratés dans le
//! navigateur, et les animations viennent du module `client`.

use active::prelude::*;

use super::Site;
use super::docs::Doc;
use super::{csp, pwa};
use crate::catalog::{self, CATEGORIES};
use crate::data::{FEATURES, GITHUB, HEROKU_DEPLOY, INSTALL};
use crate::highlight::{to_html, tokenize};
use crate::i18n::{Lang, Texts, number};
use crate::icons::{icon, logo};
use crate::labels::labels;
use crate::routes::{Page as PageKind, Route, base, doc_href, docs_href, home_href, link};

/// Ce qu'il faut pour rendre une page.
pub struct Ctx<'a> {
    pub site: &'a Site,
    pub route: Route,
    /// L'adresse publique du site, avec le préfixe, pour les URL canoniques et
    /// Open Graph (`https://maxlestage.github.io/Vitesse`).
    pub origin: String,
    /// Faire jouer l'écran de chargement (le premier accueil de la visite).
    pub intro: bool,
}

impl Ctx<'_> {
    fn lang(&self) -> Lang {
        self.route.lang
    }

    fn t(&self) -> &'static Texts {
        self.route.lang.texts()
    }

    fn is_docs(&self) -> bool {
        matches!(self.route.page, PageKind::Docs | PageKind::Doc(_))
    }
}

// ----- Le document --------------------------------------------------------------------

/// Le module JavaScript généré par wasm-bindgen et le binaire WebAssembly.
fn bundle(site: &Site) -> (String, String) {
    (
        site.assets.url("/pkg/vitesse_site.js"),
        site.assets.url("/pkg/vitesse_site_bg.wasm"),
    )
}

/// Ce que `Document` écrit en ligne : le démarrage du code du navigateur,
/// l'inscription du service worker et le style des îlots. La politique autorise
/// exactement leurs empreintes.
pub fn policy(site: &Site) -> String {
    let (js, wasm) = bundle(site);
    let loader = format!(r#"import init from "{js}";init({{module_or_path:"{wasm}"}});"#);
    let register = active::pwa::register_script(&link("/sw.js"));
    csp::policy(
        &[csp::source(&loader), csp::source(&register)],
        &[csp::source("active-island{display:contents}")],
    )
}

/// Les balises de `<head>` communes : couleurs, icône et polices préchargées.
fn head_tags() -> String {
    let b = base();
    format!(
        concat!(
            r#"<meta name="color-scheme" content="dark"/>"#,
            r#"<link rel="icon" href="{b}/assets/favicon.svg" type="image/svg+xml"/>"#,
            r#"<link rel="preload" href="{b}/assets/fonts/inter-latin.woff2" as="font" type="font/woff2" crossorigin=""/>"#,
            r#"<link rel="preload" href="{b}/assets/fonts/space-grotesk-latin.woff2" as="font" type="font/woff2" crossorigin=""/>"#,
        ),
        b = b
    )
}

/// Une page complète autour de `body` : feuille de style, code du navigateur,
/// manifeste et service worker.
fn document(site: &Site, lang: Lang, seo: Seo, body: Node, intro: bool) -> String {
    let (js, wasm) = bundle(site);
    let mut doc = Document::new(body)
        .lang(lang.code())
        .seo(seo)
        .stylesheet(site.assets.url("/assets/main.css"))
        .head(head_tags())
        .client(js, wasm)
        .manifest(&pwa::manifest_href(lang), &pwa::manifest(lang))
        .service_worker(&link("/sw.js"));
    if intro {
        doc = doc.html_attr("class", "with-loader");
    }
    if !base().is_empty() {
        // Lu par le code du navigateur pour construire ses liens.
        doc = doc.html_attr("data-base", base());
    }
    doc.render()
}

/// Les balises SEO de tout le site.
fn site_seo(origin: &str, lang: Lang) -> Seo {
    Seo::new()
        .site_name("Vitesse")
        .base_url(origin)
        .image("/assets/og-image.png")
        .og_type("website")
        .locale(lang.og_locale())
}

/// L'URL canonique et la même page dans chaque langue.
fn alternates(seo: Seo, route: Route) -> Seo {
    let seo = Lang::ALL
        .into_iter()
        .fold(seo.canonical(route.path()), |seo, l| {
            seo.alternate(l.code(), route.with_lang(l).path())
        });
    seo.alternate("x-default", route.with_lang(Lang::En).path())
}

/// Une chaîne JSON (sans les guillemets).
fn json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

// ----- Petits éléments ------------------------------------------------------------------

/// Un lien attiré par le curseur (`data-magnetic`, animé par le navigateur).
fn magnetic(href: &str, class: &str, external: bool, children: Vec<Node>) -> Element {
    let link = a()
        .class("magnetic")
        .class(class.to_owned())
        .href(href.to_owned())
        .attr("data-magnetic", "");
    let link = if external {
        link.attr("target", "_blank").attr("rel", "noopener")
    } else {
        link
    };
    link.child(span().class("magnetic__inner").children(children))
}

fn label_arrow(text: &str) -> Vec<Node> {
    vec![span().text(text.to_owned()).into(), icon("arrow").into()]
}

/// Chaque lettre monte à son tour (CSS) ; `offset` enchaîne plusieurs lignes.
fn split(text: &str, offset: usize) -> Element {
    let words: Vec<&str> = text.split(' ').collect();
    let mut root = span().class("split").attr("aria-label", text.to_owned());
    let mut start = offset;
    for (w, word) in words.iter().enumerate() {
        let letters = (start..).zip(word.chars()).map(|(i, c)| {
            span()
                .class("ch")
                .attr("aria-hidden", "true")
                .style(format!("--i:{i}"))
                .text(c.to_string())
        });
        root = root.child(span().class("word").children(letters));
        // Une de plus pour l'espace.
        start += word.chars().count() + 1;
        if w + 1 < words.len() {
            root = root.text(" ");
        }
    }
    root
}

/// Le titre d'une section : numéro, étiquette, titre et chapeau.
fn head(num: &str, label: &str, title: &str, lead: Option<&str>) -> Element {
    div()
        .class("head")
        .child(
            p().class("eyebrow")
                .attr("data-reveal", "")
                .child(span().class("eyebrow__num").text(num.to_owned()))
                .text(label.to_owned()),
        )
        .child(
            h2().class("head__title")
                .attr("data-reveal", "")
                .style("--d:80ms")
                .text(title.to_owned()),
        )
        .child(lead.map(|lead| {
            p().class("head__lead")
                .attr("data-reveal", "")
                .style("--d:160ms")
                .text(lead.to_owned())
        }))
}

/// Une fenêtre d'éditeur avec du code coloré.
fn code_window(class: &str, file: &str, source: &str) -> Element {
    div().class(class.to_owned()).child(window_bar(file)).child(
        pre()
            .class("code")
            .child(code().html(to_html(&tokenize(source)))),
    )
}

fn window_bar(file: &str) -> Element {
    div()
        .class("window__bar")
        .child(span())
        .child(span())
        .child(span())
        .child(em().text(file.to_owned()))
}

/// Un bouton « Copier » : le navigateur copie `data-copy`.
fn copy_button(lang: Lang, text: &str) -> Element {
    let l = labels(lang);
    button()
        .class("copy")
        .attr("type", "button")
        .attr("data-copy", text.to_owned())
        .attr("data-copied", l.copied)
        .attr("data-cursor", l.copy)
        .attr("aria-label", l.copy_aria)
        .text(l.copy)
}

// ----- Mise en page -----------------------------------------------------------------------

/// Les sections de l'accueil, liées depuis la navigation.
const SECTIONS: [&str; 4] = ["performances", "fonctionnalites", "express", "demarrer"];

/// Le sélecteur de langue : la même page dans chaque langue. Le choix est
/// mémorisé par le navigateur.
fn lang_switch(ctx: &Ctx<'_>) -> Element {
    div()
        .class("lang")
        .attr("role", "group")
        .attr("aria-label", ctx.t().lang_aria)
        .child(icon("globe"))
        .children(Lang::ALL.map(|l| {
            let route = if ctx.route.page == PageKind::NotFound {
                Route::new(l, PageKind::Home)
            } else {
                ctx.route.with_lang(l)
            };
            let link = a()
                .class("lang__btn")
                .href(route.href())
                .attr("hreflang", l.code())
                .attr("lang", l.code())
                .attr("title", l.name())
                .attr("data-lang", l.code())
                .text(l.code().to_uppercase());
            if l == ctx.lang() {
                link.class("is-active").attr("aria-current", "true")
            } else {
                link
            }
        }))
}

/// Un point qui suit la souris et un anneau qui le rattrape.
fn cursor() -> Element {
    div()
        .class("cursor")
        .attr("aria-hidden", "true")
        .child(div().class("cursor__dot"))
        .child(div().class("cursor__ring").child(span()))
}

/// Un éclair qui se dessine pendant qu'un compteur monte à 100, puis le rideau se
/// lève : tout en CSS, joué au premier accueil de la visite.
fn loader() -> Element {
    div()
        .class("loader")
        .attr("aria-hidden", "true")
        .child(
            div()
                .class("loader__inner")
                .child(
                    svg()
                        .class("loader__bolt")
                        .attr("viewBox", "0 0 32 32")
                        .child(path().attr("d", "M18.5 2 6 18h8.5l-2 12L26 13h-8.7z")),
                )
                .child(div().class("loader__word").text("Vitesse"))
                .child(div().class("loader__bar").child(span()))
                .child(div().class("loader__percent")),
        )
        .child(div().class("loader__curtain"))
}

fn site_header(ctx: &Ctx<'_>) -> Element {
    let t = ctx.t();
    let lang = ctx.lang();
    let docs = {
        let link = a()
            .class("nav__link nav__link--docs")
            .href(docs_href(lang))
            .child(icon("book"))
            .child(
                span()
                    .class("scramble")
                    .attr("data-text", t.nav_docs)
                    .text(t.nav_docs),
            );
        if ctx.is_docs() {
            link.class("is-active")
        } else {
            link
        }
    };
    header()
        .class("nav")
        .id("nav")
        .child(
            a().class("nav__brand")
                .href(home_href(lang, ""))
                .attr("aria-label", t.brand_aria)
                .child(logo())
                .child(span().text("Vitesse")),
        )
        .child(
            nav()
                .class("nav__links")
                .attr("aria-label", t.nav_aria)
                .children(SECTIONS.iter().zip(t.nav_links).map(|(id, text)| {
                    a().class("nav__link")
                        .href(home_href(lang, id))
                        .child(span().class("scramble").attr("data-text", text).text(text))
                }))
                .child(docs),
        )
        .child(lang_switch(ctx))
        .child(
            magnetic(
                GITHUB,
                "btn btn--ghost nav__cta",
                true,
                vec![icon("github").into(), span().text("GitHub").into()],
            )
            .attr("data-strength", "0.25"),
        )
        .child(
            button()
                .class("nav__burger")
                .attr("type", "button")
                .attr("aria-label", t.menu)
                .attr("aria-expanded", "false")
                .attr("aria-controls", "menu")
                .child(span())
                .child(span()),
        )
}

/// Le menu plein écran du téléphone.
fn site_menu(ctx: &Ctx<'_>) -> Element {
    let t = ctx.t();
    let lang = ctx.lang();
    let item = |i: usize, href: String, text: &str| {
        a().href(href)
            .style(format!("--i:{i}"))
            .child(span().class("menu__num").text(format!("0{}", i + 1)))
            .text(text.to_owned())
    };
    div()
        .class("menu")
        .id("menu")
        .attr("aria-hidden", "true")
        .attr("inert", "")
        .child(
            nav()
                .attr("aria-label", t.menu)
                .children(
                    SECTIONS
                        .iter()
                        .zip(t.nav_links)
                        .enumerate()
                        .map(|(i, (id, text))| item(i, home_href(lang, id), text)),
                )
                .child(item(4, docs_href(lang), t.docs_title))
                .child(
                    item(5, GITHUB.to_owned(), "GitHub")
                        .attr("target", "_blank")
                        .attr("rel", "noopener"),
                ),
        )
}

fn site_footer(ctx: &Ctx<'_>) -> Element {
    let t = ctx.t();
    let lang = ctx.lang();
    let project = [
        GITHUB.to_owned(),
        format!("{GITHUB}/tree/master/bench"),
        format!("{GITHUB}/tree/master/examples"),
        format!("{GITHUB}/tree/master/site"),
    ];
    let docs = [
        docs_href(lang),
        doc_href(lang, "from-express", None),
        doc_href(lang, "performance", None),
        doc_href(lang, "heroku-mobile", None),
    ];
    let community = [
        format!("{GITHUB}/issues/new"),
        format!("{GITHUB}/issues"),
        format!("{GITHUB}/pulls"),
        format!("{GITHUB}/commits/master"),
    ];
    let columns: [(&str, &[&str; 4], [String; 4], bool); 3] = [
        (t.footer_cols[0], &t.footer_project, project, true),
        (t.footer_cols[1], &t.footer_docs, docs, false),
        (t.footer_cols[2], &t.footer_community, community, true),
    ];
    footer()
        .class("footer")
        .child(div().class("footer__glow").attr("aria-hidden", "true"))
        .child(
            div()
                .class("footer__top")
                .child(
                    div()
                        .class("footer__pitch")
                        .attr("data-reveal", "")
                        .child(p().class("eyebrow").text(t.footer_eyebrow))
                        .child(p().class("footer__lead").text(t.footer_lead))
                        .child(magnetic(
                            &doc_href(lang, "installation", None),
                            "btn btn--primary",
                            false,
                            label_arrow(t.footer_cta),
                        )),
                )
                .child(
                    div()
                        .class("footer__cols")
                        .children(columns.into_iter().enumerate().map(
                            |(c, (title, texts, hrefs, external))| {
                                div()
                                    .class("footer__col")
                                    .attr("data-reveal", "")
                                    .style(format!("--d:{}ms", c * 90))
                                    .child(h4().text(title))
                                    .child(ul().children(texts.iter().zip(hrefs).map(
                                        |(text, href)| {
                                            let link = a()
                                                .href(href)
                                                .class("footer__link")
                                                .child(span().text(*text))
                                                .child(icon("arrow"));
                                            li().child(if external {
                                                link.attr("target", "_blank")
                                                    .attr("rel", "noopener")
                                            } else {
                                                link
                                            })
                                        },
                                    )))
                            },
                        )),
                ),
        )
        .child(
            div()
                .class("footer__word")
                .attr("aria-hidden", "true")
                .attr("data-reveal", "")
                .children(
                    "VITESSE"
                        .chars()
                        .enumerate()
                        .map(|(i, c)| span().style(format!("--i:{i}")).text(c.to_string())),
                ),
        )
        .child(
            div()
                .class("footer__bottom")
                .child(span().text(t.footer_copyright))
                .child(island("clock", lang.code(), crate::islands::clock))
                .child(
                    a().href("#top")
                        .class("totop")
                        .attr("data-cursor", t.totop_cursor)
                        .attr("aria-label", t.totop_aria)
                        .child(
                            svg()
                                .attr("viewBox", "0 0 100 100")
                                .class("totop__text")
                                .attr("aria-hidden", "true")
                                .child(defs().child(path().id("totop-circle").attr(
                                    "d",
                                    "M50 50 m-38 0 a38 38 0 1 1 76 0 a38 38 0 1 1 -76 0",
                                )))
                                // La longueur du texte épouse exactement le cercle,
                                // quelle que soit la langue.
                                .child(
                                    text_svg().child(
                                        el("textPath")
                                            .attr("href", "#totop-circle")
                                            .attr("textLength", "236")
                                            .attr("lengthAdjust", "spacing")
                                            .text(t.totop),
                                    ),
                                ),
                        )
                        .child(icon("up")),
                ),
        )
}

/// Une page du site autour de `content`.
fn layout(ctx: &Ctx<'_>, seo: Seo, content: Node) -> String {
    let t = ctx.t();
    let body = fragment([
        Node::from(a().class("skip").href("#main").text(t.skip)),
        ctx.intro.then(loader).into(),
        cursor().into(),
        div()
            .class("progress")
            .id("progress")
            .attr("aria-hidden", "true")
            .into(),
        site_header(ctx).into(),
        site_menu(ctx).into(),
        el("main")
            .id("main")
            .class(if ctx.is_docs() { "main--docs" } else { "" })
            .child(content)
            .into(),
        site_footer(ctx).into(),
        div().class("noise").attr("aria-hidden", "true").into(),
    ]);
    let seo = site_seo(&ctx.origin, ctx.lang()).merged(&seo);
    document(ctx.site, ctx.lang(), seo, body, ctx.intro)
}

// ----- Accueil -------------------------------------------------------------------------------

fn hero(ctx: &Ctx<'_>) -> Element {
    let t = ctx.t();
    let lang = ctx.lang();
    let [line1, line2, line3] = t.hero_title;
    // Chaque ligne enchaîne son animation après la précédente.
    let offset2 = line1.chars().count() + 1;
    let offset3 = offset2 + line2.chars().count() + 1;
    section()
        .class("hero")
        .id("top")
        .child(
            div()
                .class("hero__bg")
                .attr("aria-hidden", "true")
                .child(island("warp", "", crate::islands::warp))
                .child(div().class("blob blob--a").attr("data-speed", "0.12"))
                .child(div().class("blob blob--b").attr("data-speed", "-0.08"))
                .child(div().class("blob blob--c").attr("data-speed", "0.05"))
                .child(div().class("hero__grid"))
                .child(div().class("hero__fade")),
        )
        .child(
            div()
                .class("hero__content")
                .child(
                    a().class("badge intro")
                        .style("--d:0ms")
                        .href("#performances")
                        .child(span().class("badge__dot"))
                        .child(span().text(t.hero_badge))
                        .child(icon("arrow")),
                )
                .child(
                    h1().class("hero__title")
                        .child(span().class("hero__line").child(split(line1, 0)))
                        .child(
                            span()
                                .class("hero__line")
                                .child(split(line2, offset2))
                                .text(" ")
                                .child(span().class("grad-text").child(split(line3, offset3))),
                        ),
                )
                .child(
                    p().class("hero__sub intro")
                        .style("--d:900ms")
                        .text(t.hero_sub[0])
                        .child(span().class("rotating").child(
                            span().class("rotating__track").style("--idx:0").children(
                                t.hero_words.iter().enumerate().map(|(i, w)| {
                                    let word = span().class("rotating__word").text(*w);
                                    if i == 0 {
                                        word.class("is-active")
                                    } else {
                                        word
                                    }
                                }),
                            ),
                        ))
                        .text(t.hero_sub[1]),
                )
                .child(
                    p().class("hero__lead intro")
                        .style("--d:1050ms")
                        .text(t.hero_lead[0])
                        .child(code().text("app.get"))
                        .text(", ")
                        .child(code().text("req.params"))
                        .text(", ")
                        .child(code().text("res.json"))
                        .text(t.hero_lead[1]),
                )
                .child(
                    div()
                        .class("hero__ctas intro")
                        .style("--d:1200ms")
                        .child(
                            magnetic(
                                &doc_href(lang, "installation", None),
                                "btn btn--primary btn--lg",
                                false,
                                label_arrow(t.hero_start),
                            )
                            .attr("data-cursor", "Go"),
                        )
                        .child(magnetic(
                            "#performances",
                            "btn btn--ghost btn--lg",
                            false,
                            vec![span().text(t.hero_perf).into()],
                        )),
                )
                .child(
                    div()
                        .class("install intro")
                        .style("--d:1350ms")
                        .child(span().class("install__prompt").text("$"))
                        .child(code().text(INSTALL))
                        .child(copy_button(lang, INSTALL)),
                ),
        )
        .child(
            a().class("scroll-hint intro")
                .style("--d:1600ms")
                .href("#chiffres")
                .attr("aria-label", t.scroll_aria)
                .child(span().class("scroll-hint__mouse").child(span()))
                .child(span().text(t.scroll)),
        )
}

fn marquee(t: &Texts) -> Element {
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
        let group = |hidden: bool| {
            let group = div()
                .class("marquee__group")
                .children(items.iter().flat_map(|w| {
                    [
                        Node::from(span().class("marquee__item").text(w.to_string())),
                        span().class("marquee__star").text("✦").into(),
                    ]
                }));
            if hidden {
                group.attr("aria-hidden", "true")
            } else {
                group
            }
        };
        div().class("marquee__row").class(class).child(
            div()
                .class("marquee__track")
                .child(group(false))
                .child(group(true)),
        )
    };
    section()
        .class("marquee")
        .attr("aria-label", t.marquee_aria)
        .child(row(&t.marquee, "marquee__row--a"))
        .child(row(&code, "marquee__row--b"))
}

fn stats(lang: Lang, t: &Texts) -> Element {
    let values: [(f64, usize, &str, &str); 4] = [
        (2.78, 2, "", " M"),
        (1.62, 2, "×", ""),
        (6.1, 1, "", " µs"),
        (50.0, 0, "×", ""),
    ];
    section()
        .class("section")
        .id("chiffres")
        .child(head("01", t.stats_label, t.stats_title, None))
        .child(
            div()
                .class("stats")
                .children(values.iter().zip(&t.stats).enumerate().map(
                    |(i, ((value, decimals, prefix, suffix), stat))| {
                        article()
                            .class("stat card")
                            .attr("data-reveal", "")
                            .style(format!("--d:{}ms", i * 110))
                            .child(
                                div().class("stat__value").child(
                                    span()
                                        .class("counter")
                                        .attr("data-count", value.to_string())
                                        .attr("data-decimals", decimals.to_string())
                                        .attr("data-prefix", *prefix)
                                        .attr("data-suffix", *suffix)
                                        .text(format!(
                                            "{prefix}{}{suffix}",
                                            number(lang, *value, *decimals)
                                        )),
                                ),
                            )
                            .child(h3().class("stat__label").text(stat.title))
                            .child(p().class("stat__detail").text(stat.text))
                            .child(span().class("stat__line"))
                    },
                )),
        )
}

fn features(t: &Texts) -> Element {
    section()
        .class("section")
        .id("fonctionnalites")
        .child(head(
            "03",
            t.features_label,
            t.features_title,
            Some(t.features_lead),
        ))
        .child(
            div()
                .class("features")
                .children(FEATURES.iter().zip(&t.features).enumerate().map(
                    |(i, ((name, tag), f))| {
                        div()
                            .class("feature-wrap")
                            .attr("data-reveal", "")
                            .style(format!("--d:{}ms", (i % 4) * 90))
                            .child(
                                article()
                                    .class("feature card")
                                    .attr("data-tilt", "")
                                    .child(
                                        span().class("feature__spot").attr("aria-hidden", "true"),
                                    )
                                    .child(span().class("feature__icon").child(icon(name)))
                                    .child(h3().text(f.title))
                                    .child(p().text(f.text))
                                    .child(code().class("feature__tag").text(*tag)),
                            )
                    },
                )),
        )
}

fn journey(t: &Texts) -> Element {
    section().class("journey").id("journey").child(
        div()
            .class("journey__sticky")
            .child(
                div()
                    .class("journey__head")
                    .child(
                        p().class("eyebrow")
                            .child(span().class("eyebrow__num").text("04"))
                            .text(t.journey_label),
                    )
                    .child(h2().class("head__title").text(t.journey_title))
                    .child(
                        div()
                            .class("journey__bar")
                            .attr("aria-hidden", "true")
                            .child(span()),
                    ),
            )
            .child(
                div()
                    .class("journey__track")
                    .id("journey-track")
                    .children(t.journey.iter().enumerate().map(|(i, step)| {
                        article()
                            .class("panel")
                            .child(
                                div()
                                    .class("panel__top")
                                    .child(span().class("panel__num").text(format!("0{}", i + 1)))
                                    .child(span().class("pill pill--hot").text(step.metric)),
                            )
                            .child(
                                div()
                                    .class(format!("viz viz--{}", i + 1))
                                    .attr("aria-hidden", "true")
                                    .children((0..12).map(|k| span().style(format!("--k:{k}")))),
                            )
                            .child(h3().text(step.title))
                            .child(p().text(step.text))
                    }))
                    .child(
                        article()
                            .class("panel panel--end")
                            .child(p().class("panel__big").text("≈ 6 µs"))
                            .child(p().text(t.journey_end))
                            .child(magnetic(
                                "#express",
                                "btn btn--primary",
                                false,
                                label_arrow(t.journey_cta),
                            )),
                    ),
            ),
    )
}

fn start(lang: Lang, t: &Texts) -> Element {
    let steps: [(&str, &str, &str); 2] = [
        (
            t.start_steps[0],
            "Cargo.toml",
            "[dependencies]\nvitesse = \"0.1\"",
        ),
        (
            t.start_steps[1],
            "src/main.rs",
            "use vitesse::prelude::*;\n\nfn main() -> std::io::Result<()> {\n    let mut app = App::new();\n    app.get(\"/\", |_| async { \"Hello World!\" });\n    app.run(3000)\n}",
        ),
    ];
    let term = |l: usize, class: &str| {
        span()
            .class("term")
            .class(class.to_owned())
            .style(format!("--l:{l}"))
    };
    let kw = |label: &str, rest: &str| {
        fragment([
            Node::from(span().class("t-kw").text(label.to_owned())),
            text(rest.to_owned()),
        ])
    };
    section()
        .class("section")
        .id("demarrer")
        .child(head("06", t.start_label, t.start_title, None))
        .child(
            div()
                .class("steps")
                .child(
                    span()
                        .class("steps__line")
                        .attr("data-reveal", "")
                        .attr("aria-hidden", "true"),
                )
                .children(steps.iter().enumerate().map(|(i, (title, file, code))| {
                    article()
                        .class("step")
                        .attr("data-reveal", "")
                        .style(format!("--d:{}ms", i * 140))
                        .child(span().class("step__num").text((i + 1).to_string()))
                        .child(h3().text(*title))
                        .child(code_window("window", file, code))
                }))
                .child(
                    article()
                        .class("step")
                        .attr("data-reveal", "")
                        .style("--d:280ms")
                        .child(span().class("step__num").text("3"))
                        .child(h3().text(t.start_steps[2]))
                        .child(
                            div()
                                .class("window terminal")
                                .child(window_bar("Terminal"))
                                .child(
                                    pre()
                                        .class("code")
                                        .child(
                                            term(0, "term--cmd")
                                                .child(span().class("t-punct").text("$ "))
                                                .text("cargo run --release"),
                                        )
                                        .child(
                                            term(1, "")
                                                .child(kw("   Compiling", " vitesse v0.1.0")),
                                        )
                                        .child(term(2, "").child(kw(
                                            "    Finished",
                                            " `release` profile [optimized]",
                                        )))
                                        .child(
                                            term(3, "")
                                                .child(kw("     Running", " `target/release/app`")),
                                        )
                                        .child(term(4, "term--ok").text(t.start_listening)),
                                ),
                        ),
                ),
        )
        .child(
            div()
                .class("start__more")
                .attr("data-reveal", "")
                .style("--d:200ms")
                .child(magnetic(
                    &docs_href(lang),
                    "btn btn--primary",
                    false,
                    vec![icon("book").into(), span().text(t.start_docs).into()],
                ))
                .child(magnetic(
                    HEROKU_DEPLOY,
                    "btn btn--ghost",
                    true,
                    vec![icon("phone").into(), span().text(t.start_heroku).into()],
                )),
        )
}

fn cta(lang: Lang, t: &Texts) -> Element {
    section()
        .class("cta")
        .child(div().class("cta__bg").attr("aria-hidden", "true"))
        .child(
            h2().class("cta__title")
                .attr("data-reveal", "")
                .child(span().text(t.cta_title[0]))
                .child(
                    span()
                        .class("cta__outline")
                        .attr("data-text", t.cta_title[1])
                        .text(t.cta_title[1]),
                ),
        )
        .child(
            div()
                .class("cta__actions")
                .attr("data-reveal", "")
                .style("--d:150ms")
                .child(
                    magnetic(
                        &doc_href(lang, "first-app", None),
                        "btn btn--primary btn--lg",
                        false,
                        label_arrow(t.cta_start),
                    )
                    .attr("data-cursor", "Go"),
                )
                .child(magnetic(
                    GITHUB,
                    "btn btn--ghost btn--lg",
                    true,
                    vec![icon("github").into(), span().text(t.cta_star).into()],
                )),
        )
}

fn home(ctx: &Ctx<'_>) -> String {
    let lang = ctx.lang();
    let t = ctx.t();
    let seo = alternates(
        Seo::new()
            .title(t.meta_title)
            .title_template("%s")
            .description(t.meta_description),
        ctx.route,
    )
    .json_ld(format!(
        r#"{{"@context":"https://schema.org","@type":"SoftwareSourceCode","name":"Vitesse","description":"{}","codeRepository":"{GITHUB}","programmingLanguage":"Rust","license":"https://opensource.org/licenses/MIT","url":"{}/{}/"}}"#,
        json(t.meta_description),
        ctx.origin,
        lang.code()
    ))
    .json_ld(format!(
        r#"{{"@context":"https://schema.org","@type":"WebSite","name":"Vitesse","url":"{}/","inLanguage":"{}"}}"#,
        ctx.origin,
        lang.code()
    ));
    let content = fragment([
        Node::from(hero(ctx)),
        marquee(t).into(),
        stats(lang, t).into(),
        section()
            .class("section bench")
            .id("performances")
            .child(head("02", t.bench_label, t.bench_title, Some(t.bench_lead)))
            .child(island("bench", lang.code(), crate::islands::bench))
            .into(),
        features(t).into(),
        journey(t).into(),
        section()
            .class("section")
            .id("express")
            .child(head(
                "05",
                t.compare_label,
                t.compare_title,
                Some(t.compare_lead),
            ))
            .child(island("compare", lang.code(), crate::islands::compare))
            .into(),
        start(lang, t).into(),
        cta(lang, t).into(),
    ]);
    layout(ctx, seo, content)
}

// ----- Documentation -----------------------------------------------------------------------

/// Les pages mises en avant sur l'accueil de la documentation.
const QUICK: [(&str, &str); 3] = [
    ("first-app", "engine"),
    ("from-express", "layers"),
    ("heroku-mobile", "phone"),
];

fn docs_home(ctx: &Ctx<'_>) -> String {
    let lang = ctx.lang();
    let t = ctx.t();
    let seo = alternates(
        Seo::new()
            .title(t.docs_meta)
            .title_template("%s")
            .description(t.docs_lead),
        ctx.route,
    );
    let content = section()
        .class("docs-home")
        .child(
            div()
                .class("docs-home__hero")
                .child(div().class("docs-home__glow").attr("aria-hidden", "true"))
                .child(
                    p().class("eyebrow")
                        .child(span().class("eyebrow__num").text("Vitesse"))
                        .text(lang.name()),
                )
                .child(h1().class("docs-home__title").text(t.docs_title))
                .child(p().class("docs-home__lead").text(t.docs_lead))
                .child(island(
                    "search",
                    format!("{} large", lang.code()),
                    crate::islands::search,
                ))
                .child(p().class("docs-home__hint").text(labels(lang).search_hint)),
        )
        .child(
            div()
                .class("docs-home__quick")
                .child(h2().class("docs-home__h2").text(t.docs_quick))
                .child(
                    div()
                        .class("docs-home__quick-grid")
                        .children(QUICK.iter().filter_map(|(slug, name)| {
                            let (_, page) = catalog::find(slug)?;
                            Some(
                                a().class("doc-quick card")
                                    .href(doc_href(lang, page.slug, None))
                                    .child(span().class("doc-quick__icon").child(icon(name)))
                                    .child(span().class("doc-quick__title").text(page.title(lang)))
                                    .child(span().class("doc-quick__text").text(page.summary(lang)))
                                    .child(span().class("doc-quick__arrow").child(icon("arrow"))),
                            )
                        })),
                ),
        )
        .child(
            div()
                .class("docs-home__cats")
                .children(CATEGORIES.iter().enumerate().map(|(c, category)| {
                    let pages: Vec<_> = catalog::pages_of(c).collect();
                    article()
                        .class("doc-cat card")
                        .child(
                            div()
                                .class("doc-cat__head")
                                .child(span().class("doc-cat__icon").child(icon(category.icon)))
                                .child(h2().text(lang.pick(category.title)))
                                .child(
                                    span()
                                        .class("doc-cat__count")
                                        .text(t.docs_pages.replace("{}", &pages.len().to_string())),
                                ),
                        )
                        .child(ul().children(pages.into_iter().map(|page| {
                            li().child(
                                a().href(doc_href(lang, page.slug, None))
                                    .child(strong().text(page.title(lang)))
                                    .child(span().text(page.summary(lang))),
                            )
                        })))
                })),
        );
    layout(ctx, seo, content.into())
}

fn sidebar(ctx: &Ctx<'_>, current: &str) -> Element {
    let lang = ctx.lang();
    let t = ctx.t();
    aside()
        .class("doc-side")
        .id("doc-side")
        .child(island("search", lang.code(), crate::islands::search))
        .child(
            nav()
                .class("doc-side__nav")
                .attr("aria-label", t.docs_menu)
                .children(CATEGORIES.iter().enumerate().map(|(c, category)| {
                    div()
                        .class("doc-side__group")
                        .child(
                            p().class("doc-side__cat")
                                .child(icon(category.icon))
                                .text(lang.pick(category.title)),
                        )
                        .child(ul().children(catalog::pages_of(c).map(|page| {
                            let link = a()
                                .href(doc_href(lang, page.slug, None))
                                .text(page.title(lang));
                            li().child(if page.slug == current {
                                link.class("is-active").attr("aria-current", "page")
                            } else {
                                link
                            })
                        })))
                })),
        )
        .child(
            a().class("doc-side__api")
                .href("https://docs.rs/vitesse")
                .attr("target", "_blank")
                .attr("rel", "noopener")
                .child(icon("book"))
                .child(span().text(t.docs_api)),
        )
}

fn toc(doc: &Doc, t: &Texts) -> Option<Element> {
    (!doc.toc.is_empty()).then(|| {
        nav()
            .class("doc-toc")
            .attr("aria-label", t.docs_toc)
            .child(p().class("doc-toc__title").text(t.docs_toc))
            .child(ul().children(doc.toc.iter().map(|e| {
                let item = li().child(a().href(format!("#{}", e.id)).text(e.text.clone()));
                if e.level == 3 {
                    item.class("is-sub")
                } else {
                    item
                }
            })))
    })
}

fn doc_page(ctx: &Ctx<'_>, slug: &'static str) -> String {
    let lang = ctx.lang();
    let t = ctx.t();
    let (Some(doc), Some((_, page))) = (ctx.site.docs.get(lang, slug), catalog::find(slug)) else {
        return not_found(ctx);
    };
    let category = lang.pick(CATEGORIES[page.category].title);
    let seo = alternates(
        Seo::new()
            .title(format!("{} — {}", doc.title, t.docs_meta))
            .title_template("%s")
            .description(page.summary(lang))
            .og_type("article"),
        ctx.route,
    )
    .json_ld(format!(
        r#"{{"@context":"https://schema.org","@type":"TechArticle","headline":"{}","description":"{}","inLanguage":"{}","url":"{}{}"}}"#,
        json(&doc.title),
        json(page.summary(lang)),
        if doc.fallback { "en" } else { lang.code() },
        ctx.origin,
        ctx.route.path()
    ))
    .json_ld(format!(
        r#"{{"@context":"https://schema.org","@type":"BreadcrumbList","itemListElement":[{{"@type":"ListItem","position":1,"name":"{}","item":"{o}/{l}/docs/"}},{{"@type":"ListItem","position":2,"name":"{}","item":"{o}/{l}/docs/{slug}/"}}]}}"#,
        json(t.docs_title),
        json(&doc.title),
        o = ctx.origin,
        l = lang.code()
    ));
    let (prev, next) = catalog::neighbours(slug);
    let pager = |page: Option<&'static catalog::Page>, label: &str, class: &str, rel: &str| {
        page.map(|page| {
            a().class("doc-pager__link")
                .class(class.to_owned())
                .href(doc_href(lang, page.slug, None))
                .attr("rel", rel.to_owned())
                .child(span().class("doc-pager__label").text(label.to_owned()))
                .child(span().class("doc-pager__title").text(page.title(lang)))
        })
    };
    let main = article()
        .class("docs__main")
        .child(
            p().class("docs__crumb")
                .child(a().href(docs_href(lang)).text(t.docs_title))
                .child(span().text(format!(" / {category}"))),
        )
        .child(doc.fallback.then(|| {
            div()
                .class("callout callout--note")
                .child(p().class("callout__title").text(t.callouts[0]))
                .child(p().text(t.docs_missing))
        }))
        .child(div().class("doc-body").html(doc.html.clone()))
        .child(
            div().class("doc-foot").child(
                a().class("doc-edit")
                    .attr("target", "_blank")
                    .attr("rel", "noopener")
                    .href(format!(
                        "{GITHUB}/edit/master/docs/{}/{slug}.md",
                        lang.code()
                    ))
                    .child(icon("edit"))
                    .child(span().text(t.docs_edit)),
            ),
        )
        .child(
            nav()
                .class("doc-pager")
                .child(pager(prev, t.docs_prev, "doc-pager__link--prev", "prev"))
                .child(pager(next, t.docs_next, "doc-pager__link--next", "next")),
        );
    let content = div()
        .class("docs")
        .child(
            div()
                .class("docs__bar")
                .child(
                    button()
                        .class("docs__menu")
                        .attr("type", "button")
                        .attr("aria-expanded", "false")
                        .attr("aria-controls", "doc-side")
                        .child(icon("menu"))
                        .child(span().text(t.docs_menu)),
                )
                .child(span().class("docs__bar-title").text(page.title(lang))),
        )
        .child(sidebar(ctx, slug))
        .child(div().class("docs__scrim").attr("aria-hidden", "true"))
        .child(main)
        .child(aside().class("docs__toc").child(toc(doc, t)));
    layout(ctx, seo, content.into())
}

// ----- Page introuvable, hors ligne, racine ------------------------------------------------------

fn not_found(ctx: &Ctx<'_>) -> String {
    let lang = ctx.lang();
    let t = ctx.t();
    let seo = Seo::new()
        .title(format!("{} — Vitesse", t.not_found_title))
        .title_template("%s")
        .description(t.not_found_text)
        .noindex(true);
    let content = section().class("docs-home notfound").child(
        div()
            .class("docs-home__hero")
            .child(div().class("docs-home__glow").attr("aria-hidden", "true"))
            .child(
                p().class("notfound__code")
                    .attr("aria-hidden", "true")
                    .text("404"),
            )
            .child(h1().class("docs-home__title").text(t.not_found_title))
            .child(p().class("docs-home__lead").text(t.not_found_text))
            .child(
                div()
                    .class("notfound__actions")
                    .child(magnetic(
                        &home_href(lang, ""),
                        "btn btn--primary",
                        false,
                        label_arrow(t.not_found_back),
                    ))
                    .child(magnetic(
                        &docs_href(lang),
                        "btn btn--ghost",
                        false,
                        vec![icon("book").into(), span().text(t.docs_title).into()],
                    )),
            ),
    );
    layout(ctx, seo, content.into())
}

/// Une petite page sans navigation, centrée (hors ligne, choix de la langue).
fn standalone(site: &Site, origin: &str, title: &str, description: &str, body: Element) -> String {
    let seo = site_seo(origin, Lang::En)
        .title(title.to_owned())
        .title_template("%s")
        .description(description.to_owned())
        .canonical("/en/")
        .noindex(true);
    let body = fragment([
        Node::from(el("main").id("main").class("standalone").child(body)),
        div().class("noise").attr("aria-hidden", "true").into(),
    ]);
    document(site, Lang::En, seo, body, false)
}

/// `/offline/` : montrée par le service worker quand une page n'est pas en cache
/// et que le réseau manque, dans les trois langues.
pub fn offline(site: &Site, origin: &str) -> String {
    let en = Lang::En.texts();
    let body = div()
        .class("standalone__card card")
        .child(span().class("standalone__icon").child(icon("offline")))
        .children(Lang::ALL.map(|lang| {
            let t = lang.texts();
            div()
                .class("standalone__lang")
                .attr("lang", lang.code())
                .child(h1().text(t.offline_title))
                .child(p().text(t.offline_text))
                .child(
                    p().class("standalone__links")
                        .child(a().href(home_href(lang, "")).text(lang.name()))
                        .text(" · ")
                        .child(a().href(docs_href(lang)).text(t.docs_title)),
                )
        }));
    standalone(
        site,
        origin,
        &format!("{} — Vitesse", en.offline_title),
        en.offline_text,
        body,
    )
}

/// `/` d'un hébergement statique : le choix de la langue (l'îlot `root` emmène
/// aussitôt le visiteur vers la sienne).
pub fn root(site: &Site, origin: &str) -> String {
    let en = Lang::En.texts();
    let choose: Vec<&str> = Lang::ALL.iter().map(|l| l.texts().lang_choose).collect();
    let body = div()
        .class("standalone__card card")
        .child(span().class("standalone__logo").child(logo()))
        .child(h1().text("Vitesse"))
        .child(p().text(choose.join(" · ")))
        .child(island("root", "", crate::islands::root));
    standalone(site, origin, en.meta_title, en.meta_description, body)
}

/// Rend la page de `ctx.route`.
pub fn render(ctx: &Ctx<'_>) -> String {
    match ctx.route.page {
        PageKind::Home => home(ctx),
        PageKind::Docs => docs_home(ctx),
        PageKind::Doc(slug) => doc_page(ctx, slug),
        PageKind::NotFound => not_found(ctx),
    }
}
