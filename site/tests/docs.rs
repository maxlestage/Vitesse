//! Vérifie toute la documentation du dépôt (`docs/`) : chaque page du sommaire
//! existe dans les trois langues, son titre correspond au sommaire, et chaque lien
//! interne mène à une page et à une ancre qui existent.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use vitesse_site::catalog::PAGES;
use vitesse_site::i18n::Lang;
use vitesse_site::server::markdown::render;

fn ids(html: &str) -> HashSet<String> {
    html.split(" id=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .map(|id| id.replace("&amp;", "&"))
        .collect()
}

fn hrefs(html: &str) -> Vec<String> {
    html.split("href=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .map(|h| h.replace("&amp;", "&"))
        .collect()
}

#[test]
fn every_page_exists_and_every_link_resolves() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs");
    let mut problems = Vec::new();
    for lang in Lang::ALL {
        let mut pages = HashMap::new();
        for page in &PAGES {
            let path = dir.join(lang.code()).join(format!("{}.md", page.slug));
            match std::fs::read_to_string(&path) {
                Ok(markdown) => {
                    pages.insert(page.slug, render(&markdown, lang));
                }
                Err(e) => problems.push(format!("{}: {e}", path.display())),
            }
        }
        let anchors: HashMap<&str, HashSet<String>> = pages
            .iter()
            .map(|(slug, r)| (*slug, ids(&r.html)))
            .collect();
        let prefix = format!("/{}/docs/", lang.code());
        for page in &PAGES {
            let Some(rendered) = pages.get(page.slug) else {
                continue;
            };
            let here = format!("{}/{}", lang.code(), page.slug);
            if rendered.title != page.title(lang) {
                problems.push(format!(
                    "{here}: titre « {} » au lieu de « {} »",
                    rendered.title,
                    page.title(lang)
                ));
            }
            if rendered.toc.is_empty() {
                problems.push(format!("{here}: aucune section"));
            }
            for href in hrefs(&rendered.html) {
                let (slug, anchor) = if let Some(anchor) = href.strip_prefix('#') {
                    (page.slug, Some(anchor.to_owned()))
                } else if let Some(rest) = href.strip_prefix(&prefix) {
                    let (path, anchor) = rest
                        .split_once('#')
                        .map_or((rest, None), |(p, a)| (p, Some(a.to_owned())));
                    match path.trim_end_matches('/') {
                        "" => continue,
                        slug => (slug, anchor),
                    }
                } else if href.starts_with('/') {
                    problems.push(format!("{here}: lien « {href} » hors de la langue"));
                    continue;
                } else {
                    continue;
                };
                match anchors.get(slug) {
                    None => problems.push(format!("{here}: lien vers « {slug} » inconnu")),
                    Some(ids) => {
                        if let Some(anchor) = anchor
                            && !ids.contains(&anchor)
                        {
                            problems.push(format!("{here}: ancre « {slug}#{anchor} » introuvable"));
                        }
                    }
                }
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}
