//! Les adresses du site : `/{langue}/`, `/{langue}/docs/`, `/{langue}/docs/{page}/`,
//! derrière un préfixe éventuel (`/Vitesse` sur GitHub Pages).

use std::sync::OnceLock;

use crate::catalog;
pub use crate::i18n::Lang;

static BASE: OnceLock<String> = OnceLock::new();

/// Fixe le préfixe une fois, avant tout rendu : `""` (par défaut) ou `/Vitesse`. Le
/// serveur le lit dans `BASE_PATH`, le navigateur dans l'attribut `data-base` de
/// `<html>`.
pub fn set_base(base: &str) {
    let _ = BASE.set(normalize_base(base));
}

/// `""`, ou `/quelque-chose` sans barre finale.
pub fn normalize_base(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    match base {
        "" => String::new(),
        b if b.starts_with('/') => b.to_owned(),
        b => format!("/{b}"),
    }
}

/// Le préfixe, sans barre finale (vide par défaut).
pub fn base() -> &'static str {
    BASE.get().map_or("", String::as_str)
}

/// Un lien vers `path` (qui commence par `/`), derrière le préfixe.
pub fn link(path: &str) -> String {
    format!("{}{path}", base())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Docs,
    Doc(&'static str),
    NotFound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    pub lang: Lang,
    pub page: Page,
}

impl Route {
    pub fn new(lang: Lang, page: Page) -> Route {
        Route { lang, page }
    }

    /// La page d'une adresse (sans le préfixe) ; `None` si elle ne commence pas par
    /// une langue.
    pub fn parse(path: &str) -> Option<Route> {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let lang = Lang::parse(parts.first()?)?;
        let page = match parts[1..] {
            [] => Page::Home,
            ["docs"] => Page::Docs,
            ["docs", slug] => match catalog::find(slug) {
                Some((_, page)) => Page::Doc(page.slug),
                None => Page::NotFound,
            },
            _ => Page::NotFound,
        };
        Some(Route { lang, page })
    }

    /// L'adresse de la page, sans le préfixe.
    pub fn path(&self) -> String {
        let l = self.lang.code();
        match self.page {
            Page::Home => format!("/{l}/"),
            Page::Docs => format!("/{l}/docs/"),
            Page::Doc(slug) => format!("/{l}/docs/{slug}/"),
            Page::NotFound => format!("/{l}/404/"),
        }
    }

    /// Le lien vers la page, derrière le préfixe.
    pub fn href(&self) -> String {
        link(&self.path())
    }

    /// La même page dans une autre langue.
    pub fn with_lang(&self, lang: Lang) -> Route {
        Route { lang, ..*self }
    }

    /// Toutes les pages du site, dans toutes les langues (ce qu'écrit l'export).
    pub fn all() -> Vec<Route> {
        let mut routes = Vec::new();
        for lang in Lang::ALL {
            routes.push(Route::new(lang, Page::Home));
            routes.push(Route::new(lang, Page::Docs));
            routes.extend(
                catalog::PAGES
                    .iter()
                    .map(|p| Route::new(lang, Page::Doc(p.slug))),
            );
        }
        routes
    }
}

/// L'accueil d'une langue, avec une ancre éventuelle (`performances`…).
pub fn home_href(lang: Lang, anchor: &str) -> String {
    let home = Route::new(lang, Page::Home).href();
    if anchor.is_empty() {
        home
    } else {
        format!("{home}#{anchor}")
    }
}

/// Le sommaire de la documentation d'une langue.
pub fn docs_href(lang: Lang) -> String {
    Route::new(lang, Page::Docs).href()
}

/// Une page de la documentation, avec une ancre éventuelle.
pub fn doc_href(lang: Lang, slug: &str, anchor: Option<&str>) -> String {
    let path = link(&format!("/{}/docs/{slug}/", lang.code()));
    match anchor {
        Some(anchor) if !anchor.is_empty() => format!("{path}#{anchor}"),
        _ => path,
    }
}

/// L'adresse où mène une ancre de l'ancien site (`#/docs/routing/jokers`,
/// `#performances`…), qui n'avait qu'une page : `None` pour une ancre ordinaire.
pub fn legacy_hash(lang: Lang, hash: &str) -> Option<String> {
    let rest = hash.trim_start_matches('#').strip_prefix("/docs")?;
    let mut parts = rest.trim_matches('/').splitn(2, '/');
    let slug = parts.next().unwrap_or_default();
    let anchor = parts.next().filter(|a| !a.is_empty());
    Some(match catalog::find(slug) {
        Some((_, page)) => doc_href(lang, page.slug, anchor),
        None => docs_href(lang),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_builds_urls() {
        let doc = Route::parse("/fr/docs/routing/").unwrap();
        assert_eq!(doc, Route::new(Lang::Fr, Page::Doc("routing")));
        assert_eq!(doc.path(), "/fr/docs/routing/");
        assert_eq!(doc.with_lang(Lang::Es).path(), "/es/docs/routing/");
        assert_eq!(Route::parse("/en").unwrap().page, Page::Home);
        assert_eq!(Route::parse("/en/").unwrap().page, Page::Home);
        assert_eq!(Route::parse("/es/docs").unwrap().page, Page::Docs);
        assert_eq!(Route::parse("/en/docs/nope/").unwrap().page, Page::NotFound);
        assert_eq!(Route::parse("/en/a/b/c").unwrap().page, Page::NotFound);
        assert_eq!(Route::parse("/de/"), None);
        assert_eq!(Route::parse("/"), None);
    }

    #[test]
    fn lists_every_page_once() {
        let all = Route::all();
        assert_eq!(all.len(), 3 * (2 + catalog::PAGES.len()));
        let mut paths: Vec<String> = all.iter().map(Route::path).collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), all.len());
    }

    #[test]
    fn normalizes_the_base_path() {
        assert_eq!(normalize_base(""), "");
        assert_eq!(normalize_base("/"), "");
        assert_eq!(normalize_base("Vitesse/"), "/Vitesse");
        assert_eq!(normalize_base(" /Vitesse "), "/Vitesse");
        // Le préfixe n'est pas fixé dans les tests unitaires.
        assert_eq!(link("/en/"), "/en/");
        assert_eq!(doc_href(Lang::Es, "faq", Some("x")), "/es/docs/faq/#x");
        assert_eq!(home_href(Lang::Fr, "performances"), "/fr/#performances");
    }

    #[test]
    fn translates_the_old_hash_urls() {
        assert_eq!(
            legacy_hash(Lang::En, "#/docs").as_deref(),
            Some("/en/docs/")
        );
        assert_eq!(
            legacy_hash(Lang::Fr, "#/docs/routing/jokers").as_deref(),
            Some("/fr/docs/routing/#jokers")
        );
        assert_eq!(
            legacy_hash(Lang::Es, "#/docs/nope").as_deref(),
            Some("/es/docs/")
        );
        assert_eq!(legacy_hash(Lang::En, "#performances"), None);
        assert_eq!(legacy_hash(Lang::En, ""), None);
    }
}
