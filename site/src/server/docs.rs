//! La documentation : les pages `docs/{en,fr,es}/{page}.md` du dépôt, lues et rendues
//! une fois au démarrage, et l'index de recherche de chaque langue.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::markdown::{Slugger, TocEntry, plain_line, render};
use crate::catalog::PAGES;
use crate::i18n::Lang;
use crate::routes::doc_href;
use crate::search::Entry;

/// Une page rendue.
pub struct Doc {
    pub title: String,
    pub html: String,
    pub toc: Vec<TocEntry>,
    /// Le texte anglais, faute de traduction.
    pub fallback: bool,
}

pub struct Docs {
    pages: HashMap<(Lang, &'static str), Doc>,
    index: HashMap<Lang, Vec<Entry>>,
    /// Empreinte de toutes les pages, pour la version du service worker.
    pub digest: String,
}

/// Le dossier des pages : `DOCS_DIR`, sinon `../docs` à côté du site, sinon `docs/`
/// dans le dossier courant.
pub fn docs_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("DOCS_DIR") {
        return PathBuf::from(dir);
    }
    let beside = super::site_dir().join("../docs");
    if beside.is_dir() {
        return beside;
    }
    PathBuf::from("docs")
}

impl Docs {
    /// Lit et rend toutes les pages (appeler [`crate::routes::set_base`] avant : les
    /// liens en dépendent).
    pub fn load(dir: &Path) -> Docs {
        let mut sources: HashMap<(Lang, &'static str), String> = HashMap::new();
        let mut bytes = Vec::new();
        for lang in Lang::ALL {
            for page in &PAGES {
                let file = dir.join(lang.code()).join(format!("{}.md", page.slug));
                if let Ok(markdown) = std::fs::read_to_string(&file) {
                    bytes.extend_from_slice(markdown.as_bytes());
                    sources.insert((lang, page.slug), markdown);
                }
            }
        }
        let mut pages = HashMap::new();
        let mut index = HashMap::new();
        for lang in Lang::ALL {
            let mut entries = Vec::new();
            for page in &PAGES {
                let (markdown, fallback) = match sources.get(&(lang, page.slug)) {
                    Some(md) => (md, false),
                    None => match sources.get(&(Lang::En, page.slug)) {
                        Some(md) => (md, true),
                        None => continue,
                    },
                };
                // Le texte anglais garde les liens de la langue de la page.
                let rendered = render(markdown, lang);
                let title = if fallback || rendered.title.is_empty() {
                    page.title(lang).to_owned()
                } else {
                    rendered.title
                };
                entries.extend(index_entries(
                    lang,
                    page.slug,
                    &title,
                    page.summary(lang),
                    markdown,
                ));
                pages.insert(
                    (lang, page.slug),
                    Doc {
                        title,
                        html: rendered.html,
                        toc: rendered.toc,
                        fallback,
                    },
                );
            }
            index.insert(lang, entries);
        }
        Docs {
            pages,
            index,
            digest: super::version(&bytes),
        }
    }

    pub fn get(&self, lang: Lang, slug: &str) -> Option<&Doc> {
        let (_, page) = crate::catalog::find(slug)?;
        self.pages.get(&(lang, page.slug))
    }

    /// Le nombre de pages vraiment écrites dans `lang`.
    pub fn count(&self, lang: Lang) -> usize {
        self.pages
            .iter()
            .filter(|((l, _), doc)| *l == lang && !doc.fallback)
            .count()
    }

    /// L'index de recherche d'une langue : chaque page, puis chacune de ses sections.
    pub fn search_index(&self, lang: Lang) -> &[Entry] {
        self.index.get(&lang).map_or(&[], Vec::as_slice)
    }
}

/// Les entrées de recherche d'une page : la page (titre et résumé), son
/// introduction, puis une entrée par section (`##` et plus), avec son texte brut.
fn index_entries(
    lang: Lang,
    slug: &'static str,
    title: &str,
    summary: &str,
    markdown: &str,
) -> Vec<Entry> {
    let mut entries = vec![Entry {
        page: true,
        href: doc_href(lang, slug, None),
        title: title.to_owned(),
        section: String::new(),
        text: summary.to_owned(),
    }];
    let mut current = Entry {
        page: false,
        href: doc_href(lang, slug, None),
        title: title.to_owned(),
        section: String::new(),
        text: String::new(),
    };
    let mut slugger = Slugger::default();
    let mut in_code = false;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        let text = if in_code {
            trimmed.split_whitespace().collect::<Vec<_>>().join(" ")
        } else if trimmed.starts_with("##") {
            let heading = plain_line(trimmed);
            let anchor = slugger.slug(&heading);
            let next = Entry {
                page: false,
                href: doc_href(lang, slug, Some(&anchor)),
                title: title.to_owned(),
                section: heading,
                text: String::new(),
            };
            let done = std::mem::replace(&mut current, next);
            if !done.text.is_empty() || !done.section.is_empty() {
                entries.push(done);
            }
            continue;
        } else if trimmed.starts_with("# ")
            || (trimmed.starts_with('|') && trimmed.chars().all(|c| "|-: ".contains(c)))
        {
            // Le titre de la page, et les lignes de séparation des tableaux.
            continue;
        } else {
            plain_line(trimmed)
        };
        if !text.is_empty() {
            if !current.text.is_empty() {
                current.text.push(' ');
            }
            current.text.push_str(&text);
        }
    }
    if !current.text.is_empty() || !current.section.is_empty() {
        entries.push(current);
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_pages_and_sections() {
        let md = "# Routage\n\nIntro du **routage**.\n\n## Jokers\n\nUn `*chemin`.\n\n```rust\napp.get(\"/*path\", h);\n```\n\n## Jokers\n\nEncore.\n";
        let entries = index_entries(Lang::Fr, "routing", "Routage", "Résumé.", md);
        assert_eq!(entries.len(), 4);
        assert!(entries[0].page);
        assert_eq!(entries[0].text, "Résumé.");
        assert_eq!(entries[1].section, "");
        assert_eq!(entries[1].text, "Intro du routage.");
        assert_eq!(entries[2].href, "/fr/docs/routing/#jokers");
        assert_eq!(entries[2].text, "Un chemin. app.get(\"/*path\", h);");
        // Les titres en double ont le même identifiant que dans la page.
        assert_eq!(entries[3].href, "/fr/docs/routing/#jokers-1");
    }
}
