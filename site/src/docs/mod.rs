//! La documentation : sommaire, chargement et rendu des pages Markdown,
//! recherche, et les écrans qui les affichent.

pub mod fetch;
pub mod index;
pub mod render;
pub mod search;
mod view;

pub use view::{DocsHome, DocsPage};

#[cfg(test)]
mod tests {
    //! Vérifie toute la documentation du dépôt : chaque page existe dans les
    //! trois langues, son titre correspond au sommaire, et chaque lien
    //! interne mène à une page et à une ancre qui existent.

    use std::collections::{HashMap, HashSet};

    use super::index::PAGES;
    use super::render::render;
    use crate::i18n::Lang;

    fn ids(html: &str) -> HashSet<String> {
        html.split(" id=\"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .map(|id| id.replace("&amp;", "&"))
            .collect()
    }

    #[test]
    fn every_page_exists_and_every_link_resolves() {
        let mut problems = Vec::new();
        for lang in Lang::ALL {
            let mut pages = HashMap::new();
            for page in &PAGES {
                let path = format!("../docs/{}/{}.md", lang.code(), page.slug);
                match std::fs::read_to_string(&path) {
                    Ok(markdown) => {
                        pages.insert(page.slug, render(&markdown, lang, page.slug));
                    }
                    Err(e) => problems.push(format!("{path}: {e}")),
                }
            }
            let anchors: HashMap<&str, HashSet<String>> = pages
                .iter()
                .map(|(slug, r)| (*slug, ids(&r.html)))
                .collect();
            for page in &PAGES {
                let Some(rendered) = pages.get(page.slug) else {
                    continue;
                };
                let where_ = format!("{}/{}", lang.code(), page.slug);
                if rendered.title != page.title(lang) {
                    problems.push(format!(
                        "{where_}: titre « {} » au lieu de « {} »",
                        rendered.title,
                        page.title(lang)
                    ));
                }
                for link in rendered.html.split("href=\"#/docs").skip(1) {
                    let target = link.split('"').next().unwrap_or_default();
                    let mut parts = target.trim_start_matches('/').splitn(2, '/');
                    let slug = parts.next().unwrap_or_default();
                    if slug.is_empty() {
                        continue;
                    }
                    match anchors.get(slug) {
                        None => problems.push(format!("{where_}: lien vers « {slug} » inconnu")),
                        Some(ids) => {
                            if let Some(anchor) = parts.next()
                                && !ids.contains(anchor)
                            {
                                problems.push(format!(
                                    "{where_}: ancre « {slug}/{anchor} » introuvable"
                                ));
                            }
                        }
                    }
                }
            }
        }
        assert!(problems.is_empty(), "\n{}", problems.join("\n"));
    }
}
