//! Transforme une page Markdown de la documentation en HTML, sur le serveur :
//! titres avec ancres, liens réécrits vers les pages du site, code coloré avec
//! bouton « copier », encadrés `> [!NOTE]` et tableaux qui défilent sur mobile.

use std::collections::HashMap;

use pulldown_cmark::{
    BlockQuoteKind, CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html,
};

use crate::catalog;
use crate::data::GITHUB;
use crate::highlight::{self, Kind, escape};
use crate::i18n::Lang;
use crate::labels::labels;
use crate::routes::{doc_href, docs_href};

#[derive(Clone, PartialEq, Debug)]
pub struct TocEntry {
    pub level: u8,
    pub id: String,
    pub text: String,
}

pub struct Rendered {
    pub html: String,
    pub title: String,
    pub toc: Vec<TocEntry>,
}

/// Les identifiants de titres, comme GitHub : minuscules, ponctuation
/// retirée, espaces remplacées par des tirets, suffixe `-1`, `-2`… en cas de
/// doublon. Les ancres fonctionnent ainsi sur le site comme sur GitHub.
#[derive(Default)]
pub struct Slugger {
    seen: HashMap<String, usize>,
}

impl Slugger {
    pub fn slug(&mut self, text: &str) -> String {
        let base: String = text
            .to_lowercase()
            .chars()
            .filter_map(|c| match c {
                ' ' => Some('-'),
                c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
                _ => None,
            })
            .collect();
        let count = self.seen.entry(base.clone()).or_insert(0);
        let slug = if *count == 0 {
            base
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        slug
    }
}

fn level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Réécrit la destination d'un lien. Renvoie aussi `true` si le lien sort
/// du site (il s'ouvre alors dans un nouvel onglet).
pub fn rewrite_link(dest: &str, lang: Lang) -> (String, bool) {
    if dest.starts_with("http://") || dest.starts_with("https://") {
        return (dest.to_owned(), true);
    }
    if dest.starts_with("mailto:") || dest.starts_with('#') {
        return (dest.to_owned(), false);
    }
    let (path, anchor) = dest
        .split_once('#')
        .map_or((dest, None), |(p, a)| (p, Some(a)));
    // `routing.md`, `./routing.md` ou `../fr/routing.md` : une page de la doc.
    let file = path.rsplit('/').next().unwrap_or(path);
    if let Some(page) = file.strip_suffix(".md") {
        let same_folder = !path.trim_start_matches("./").contains('/');
        let other_language = path.starts_with("../") && path.matches('/').count() == 2;
        if same_folder || other_language {
            // `../fr/routing.md` mène à la page française.
            let target = if other_language {
                path.split('/').nth(1).and_then(Lang::parse).unwrap_or(lang)
            } else {
                lang
            };
            if page == "README" {
                return (docs_href(target), false);
            }
            if catalog::find(page).is_some() {
                return (doc_href(target, page, anchor), false);
            }
        }
    }
    // Un autre fichier du dépôt : on l'ouvre sur GitHub.
    (
        format!("{GITHUB}/blob/master/docs/{}/{dest}", lang.code()),
        true,
    )
}

fn spans(tokens: Vec<(Kind, String)>) -> String {
    highlight::to_html(&tokens)
}

/// Coloration légère pour le shell, TOML, YAML, Dockerfile, Nginx… :
/// commentaires `#`, chaînes, invite `$`, sections `[...]` et instructions
/// Dockerfile.
fn highlight_simple(code: &str, language: &str) -> Vec<(Kind, String)> {
    let mut out: Vec<(Kind, String)> = Vec::new();
    let push = |out: &mut Vec<(Kind, String)>, kind: Kind, text: &str| {
        if !text.is_empty() {
            out.push((kind, text.to_owned()));
        }
    };
    for (n, line) in code.split('\n').enumerate() {
        if n > 0 {
            push(&mut out, Kind::Plain, "\n");
        }
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        push(&mut out, Kind::Plain, indent);
        let mut rest = trimmed;
        if matches!(language, "toml" | "ini") && rest.starts_with('[') {
            push(&mut out, Kind::Type, rest);
            continue;
        }
        if matches!(language, "sh" | "bash" | "shell" | "console") && rest.starts_with("$ ") {
            push(&mut out, Kind::Punct, "$ ");
            rest = &rest[2..];
        }
        if language == "dockerfile" {
            let word_end = rest.find(' ').unwrap_or(rest.len());
            let word = &rest[..word_end];
            if !word.is_empty() && word.chars().all(|c| c.is_ascii_uppercase()) {
                push(&mut out, Kind::Keyword, word);
                rest = &rest[word_end..];
            }
        }
        let mut plain_start = 0;
        let mut chars = rest.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            let starts_comment = c == '#'
                && (i == 0 || rest[..i].ends_with(char::is_whitespace))
                && language != "http";
            if starts_comment {
                push(&mut out, Kind::Plain, &rest[plain_start..i]);
                push(&mut out, Kind::Comment, &rest[i..]);
                plain_start = rest.len();
                break;
            }
            if c == '"' || (c == '\'' && language != "text") {
                let end = rest[i + 1..].find(c).map(|j| i + 1 + j + 1);
                if let Some(end) = end {
                    push(&mut out, Kind::Plain, &rest[plain_start..i]);
                    push(&mut out, Kind::String, &rest[i..end]);
                    plain_start = end;
                    while chars.peek().is_some_and(|&(j, _)| j < end) {
                        chars.next();
                    }
                }
            }
        }
        push(&mut out, Kind::Plain, &rest[plain_start.min(rest.len())..]);
    }
    out
}

fn code_block(code: &str, language: &str, copy: &str, copied: &str) -> String {
    let code = code.trim_end_matches('\n');
    let language = language.to_ascii_lowercase();
    let body = match language.as_str() {
        "rust" | "rs" | "js" | "javascript" | "ts" | "typescript" | "json" => {
            spans(highlight::tokenize(code))
        }
        "" | "text" | "txt" | "plain" => escape(code),
        other => spans(highlight_simple(code, other)),
    };
    let label = if language.is_empty() {
        "text".to_owned()
    } else {
        escape(&language)
    };
    format!(
        "<div class=\"doc-code\"><div class=\"doc-code__bar\"><span>{label}</span>\
         <button type=\"button\" class=\"doc-code__copy\" data-copied=\"{}\">{}</button></div>\
         <pre class=\"code\"><code>{body}</code></pre></div>",
        escape(copied),
        escape(copy),
    )
}

pub fn render(markdown: &str, lang: Lang) -> Rendered {
    let texts = lang.texts();
    let l = labels(lang);
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    let mut events = Parser::new_ext(markdown, options);
    let mut out: Vec<Event> = Vec::new();
    let mut slugger = Slugger::default();
    let mut toc = Vec::new();
    let mut title = String::new();
    // Pour chaque citation ouverte : est-ce un encadré ?
    let mut quotes: Vec<bool> = Vec::new();
    let html_event = |s: String| Event::Html(CowStr::from(s));

    while let Some(event) = events.next() {
        match event {
            Event::Start(Tag::Heading { level: l, .. }) => {
                let n = level(l);
                let mut inner = Vec::new();
                let mut text = String::new();
                for e in events.by_ref() {
                    match &e {
                        Event::End(TagEnd::Heading(_)) => break,
                        Event::Text(t) | Event::Code(t) => text.push_str(t),
                        _ => {}
                    }
                    inner.push(e);
                }
                if n == 1 {
                    title = text;
                    out.push(html_event("<h1 class=\"doc-title\">".into()));
                    out.extend(inner);
                    out.push(html_event("</h1>".into()));
                    continue;
                }
                let id = slugger.slug(&text);
                if n <= 3 {
                    toc.push(TocEntry {
                        level: n,
                        id: id.clone(),
                        text,
                    });
                }
                out.push(html_event(format!("<h{n} id=\"{}\">", escape(&id))));
                out.extend(inner);
                out.push(html_event(format!(
                    "<a class=\"doc-anchor\" href=\"#{}\" aria-hidden=\"true\" tabindex=\"-1\">#</a></h{n}>",
                    escape(&id)
                )));
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let language = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split([',', ' ']).next().unwrap_or_default().to_owned()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                let mut code = String::new();
                for e in events.by_ref() {
                    match e {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(t) => code.push_str(&t),
                        _ => {}
                    }
                }
                out.push(html_event(code_block(&code, &language, l.copy, l.copied)));
            }
            Event::Start(Tag::Link {
                dest_url, title: t, ..
            }) => {
                let (href, external) = rewrite_link(&dest_url, lang);
                let mut tag = format!("<a href=\"{}\"", escape(&href));
                if !t.is_empty() {
                    tag.push_str(&format!(" title=\"{}\"", escape(&t)));
                }
                if external {
                    tag.push_str(" target=\"_blank\" rel=\"noopener\"");
                }
                tag.push('>');
                out.push(Event::InlineHtml(CowStr::from(tag)));
            }
            Event::End(TagEnd::Link) => out.push(Event::InlineHtml(CowStr::from("</a>"))),
            Event::Start(Tag::BlockQuote(Some(kind))) => {
                let (class, i) = match kind {
                    BlockQuoteKind::Note => ("note", 0),
                    BlockQuoteKind::Tip => ("tip", 1),
                    BlockQuoteKind::Important => ("important", 2),
                    BlockQuoteKind::Warning => ("warning", 3),
                    BlockQuoteKind::Caution => ("caution", 4),
                };
                quotes.push(true);
                out.push(html_event(format!(
                    "<div class=\"callout callout--{class}\"><p class=\"callout__title\">{}</p>",
                    texts.callouts[i]
                )));
            }
            Event::Start(Tag::BlockQuote(None)) => {
                quotes.push(false);
                out.push(event);
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                if quotes.pop() == Some(true) {
                    out.push(html_event("</div>".into()));
                } else {
                    out.push(event);
                }
            }
            Event::Start(Tag::Table(_)) => {
                out.push(html_event("<div class=\"table-wrap\">".into()));
                out.push(event);
            }
            Event::End(TagEnd::Table) => {
                out.push(event);
                out.push(html_event("</div>".into()));
            }
            // Pas de HTML brut dans la doc : on l'affiche tel quel.
            Event::Html(raw) | Event::InlineHtml(raw) => out.push(Event::Text(raw)),
            other => out.push(other),
        }
    }

    let mut html = String::new();
    html::push_html(&mut html, out.into_iter());
    Rendered { html, title, toc }
}

/// Le texte brut d'une ligne de Markdown, pour la recherche.
pub fn plain_line(line: &str) -> String {
    let mut s = line.trim();
    s = s.trim_start_matches('#').trim_start();
    s = s.trim_start_matches('>').trim_start();
    for marker in [
        "[!NOTE]",
        "[!TIP]",
        "[!IMPORTANT]",
        "[!WARNING]",
        "[!CAUTION]",
    ] {
        s = s.trim_start_matches(marker);
    }
    if let Some(rest) = s.strip_prefix("- ").or_else(|| s.strip_prefix("* ")) {
        s = rest;
    }
    // [texte](lien) → texte ; on retire `, ** et _.
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '`' | '*' => {}
            ']' if chars.peek() == Some(&'(') => {
                for c in chars.by_ref() {
                    if c == ')' {
                        break;
                    }
                }
            }
            '[' => {}
            '|' => out.push(' '),
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_like_github() {
        let mut s = Slugger::default();
        assert_eq!(s.slug("Paramètres de route"), "paramètres-de-route");
        assert_eq!(s.slug("D'Express à Vitesse"), "dexpress-à-vitesse");
        assert_eq!(s.slug("`app.get()` et `app.post()`"), "appget-et-apppost");
        assert_eq!(s.slug("Paramètres de route"), "paramètres-de-route-1");
        assert_eq!(s.slug("¿Qué es?"), "qué-es");
    }

    #[test]
    fn rewrites_links() {
        let l = Lang::Fr;
        assert_eq!(rewrite_link("routing.md", l).0, "/fr/docs/routing/");
        assert_eq!(
            rewrite_link("routing.md#jokers", l).0,
            "/fr/docs/routing/#jokers"
        );
        assert_eq!(rewrite_link("#plus-loin", l).0, "#plus-loin");
        assert_eq!(rewrite_link("../en/routing.md", l).0, "/en/docs/routing/");
        assert_eq!(rewrite_link("README.md", l).0, "/fr/docs/");
        let (href, external) = rewrite_link("../../examples/demo.rs", l);
        assert!(external);
        assert_eq!(
            href,
            "https://github.com/maxlestage/Vitesse/blob/master/docs/fr/../../examples/demo.rs"
        );
        assert_eq!(
            rewrite_link("https://docs.rs/vitesse", l),
            ("https://docs.rs/vitesse".to_owned(), true)
        );
    }

    #[test]
    fn renders_headings_code_and_callouts() {
        let md = "# Titre\n\nIntro avec [un lien](routing.md).\n\n## Une section\n\n```rust\nlet x = 1;\n```\n\n> [!TIP]\n> Astuce !\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        let r = render(md, Lang::Fr);
        assert_eq!(r.title, "Titre");
        assert_eq!(r.toc.len(), 1);
        assert_eq!(r.toc[0].id, "une-section");
        assert!(r.html.contains("<h2 id=\"une-section\">"));
        assert!(r.html.contains("href=\"/fr/docs/routing/\""));
        assert!(r.html.contains("href=\"#une-section\""));
        assert!(r.html.contains("<span class=\"t-kw\">let</span>"));
        assert!(r.html.contains("callout--tip"));
        assert!(r.html.contains("Astuce"));
        assert!(r.html.contains("<div class=\"table-wrap\"><table>"));
    }

    #[test]
    fn keeps_escaped_pipes_in_tables() {
        let r = render(
            "| a | b |\n|---|---|\n| `\\|req\\| async {}` | x |\n",
            Lang::En,
        );
        assert!(r.html.contains("<code>|req| async {}</code>"), "{}", r.html);
    }

    #[test]
    fn escapes_raw_html() {
        let r = render("Bonjour <script>alert(1)</script>", Lang::Fr);
        assert!(!r.html.contains("<script>"));
    }

    #[test]
    fn plain_lines_for_search() {
        assert_eq!(
            plain_line("> [!NOTE] Voir [le routage](routing.md) et `app.get`"),
            "Voir le routage et app.get"
        );
        assert_eq!(plain_line("## Un **titre**"), "Un titre");
    }
}
