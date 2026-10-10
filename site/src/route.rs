//! La navigation par ancre (`#/docs/...`), qui fonctionne sur GitHub Pages
//! sans configuration serveur. Les autres ancres (`#performances`…) restent
//! des sections de la page d'accueil.

use crate::docs::index;
use crate::dom;

#[derive(Clone, PartialEq, Debug)]
pub enum Route {
    Home,
    DocsHome,
    Doc {
        slug: &'static str,
        anchor: Option<String>,
    },
}

impl Route {
    pub fn is_docs(&self) -> bool {
        !matches!(self, Route::Home)
    }
}

pub fn parse(hash: &str) -> Route {
    let hash = hash.trim_start_matches('#');
    let Some(rest) = hash.strip_prefix("/docs") else {
        return Route::Home;
    };
    let mut parts = rest.trim_matches('/').splitn(2, '/');
    let slug = parts.next().unwrap_or_default();
    let anchor = parts.next().filter(|a| !a.is_empty()).map(|a| {
        js_sys::decode_uri_component(a)
            .ok()
            .and_then(|s| s.as_string())
            .unwrap_or_else(|| a.to_owned())
    });
    match index::find(slug) {
        Some((_, page)) => Route::Doc {
            slug: page.slug,
            anchor,
        },
        None => Route::DocsHome,
    }
}

pub fn current() -> Route {
    parse(&dom::window().location().hash().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hashes() {
        assert_eq!(parse(""), Route::Home);
        assert_eq!(parse("#performances"), Route::Home);
        assert_eq!(parse("#/docs"), Route::DocsHome);
        assert_eq!(parse("#/docs/"), Route::DocsHome);
        assert_eq!(parse("#/docs/nope"), Route::DocsHome);
        assert_eq!(
            parse("#/docs/routing"),
            Route::Doc {
                slug: "routing",
                anchor: None
            }
        );
    }
}
