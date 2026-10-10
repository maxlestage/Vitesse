//! La recherche plein texte : dans les titres et résumés du sommaire, puis
//! dans le contenu des pages déjà chargées. Insensible à la casse et aux
//! accents.

use crate::i18n::Lang;

use super::fetch;
use super::index::PAGES;
use super::render::{Slugger, plain_line};

pub struct Hit {
    pub slug: &'static str,
    pub title: &'static str,
    pub anchor: Option<String>,
    pub section: Option<String>,
    /// Avant, correspondance, après.
    pub snippet: Option<(String, String, String)>,
    score: u32,
}

/// Minuscules sans accents, caractère pour caractère (les positions restent
/// alignées avec le texte d'origine).
pub fn fold(text: &str) -> Vec<char> {
    text.chars()
        .map(|c| {
            let c = c.to_lowercase().next().unwrap_or(c);
            match c {
                'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'æ' => 'a',
                'ç' => 'c',
                'è' | 'é' | 'ê' | 'ë' => 'e',
                'ì' | 'í' | 'î' | 'ï' => 'i',
                'ñ' => 'n',
                'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'œ' => 'o',
                'ù' | 'ú' | 'û' | 'ü' => 'u',
                'ý' | 'ÿ' => 'y',
                '’' => '\'',
                c => c,
            }
        })
        .collect()
}

fn find(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| haystack[i..i + needle.len()] == *needle)
}

fn snippet(text: &str, at: usize, len: usize) -> (String, String, String) {
    let chars: Vec<char> = text.chars().collect();
    let start = at.saturating_sub(48);
    let end = (at + len + 72).min(chars.len());
    let mut before: String = chars[start..at].iter().collect();
    if start > 0 {
        before.insert(0, '…');
    }
    let matched: String = chars[at..at + len].iter().collect();
    let mut after: String = chars[at + len..end].iter().collect();
    if end < chars.len() {
        after.push('…');
    }
    (before, matched, after)
}

pub fn search(lang: Lang, query: &str) -> Vec<Hit> {
    let needle = fold(query.trim());
    if needle.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for page in &PAGES {
        let title = page.title(lang);
        let mut best = None;
        if let Some(i) = find(&fold(title), &needle) {
            best = Some(if i == 0 { 120 } else { 100 });
        } else if find(&fold(page.summary(lang)), &needle).is_some() {
            best = Some(60);
        }
        if let Some(score) = best {
            hits.push(Hit {
                slug: page.slug,
                title,
                anchor: None,
                section: None,
                snippet: None,
                score,
            });
        }
        let Some(markdown) = fetch::cached(lang, page.slug) else {
            continue;
        };
        let mut slugger = Slugger::default();
        let mut section: Option<(String, String)> = None;
        let mut in_code = false;
        let mut found = 0;
        for line in markdown.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") {
                in_code = !in_code;
                continue;
            }
            let heading = !in_code && trimmed.starts_with("##");
            if heading {
                let text = plain_line(trimmed);
                section = Some((slugger.slug(&text), text));
            } else if !in_code && trimmed.starts_with("# ") {
                continue;
            }
            if found >= 2 {
                continue;
            }
            let text = if in_code {
                trimmed.to_owned()
            } else {
                plain_line(trimmed)
            };
            let Some(at) = find(&fold(&text), &needle) else {
                continue;
            };
            found += 1;
            hits.push(Hit {
                slug: page.slug,
                title,
                anchor: section.as_ref().map(|(id, _)| id.clone()),
                section: section.as_ref().map(|(_, t)| t.clone()),
                snippet: (!heading).then(|| snippet(&text, at, needle.len())),
                score: if heading { 50 } else { 30 },
            });
        }
    }
    hits.sort_by_key(|h| std::cmp::Reverse(h.score));
    hits.truncate(12);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_accents_and_case() {
        let f: String = fold("Déployer ÉTAT Ñandú").into_iter().collect();
        assert_eq!(f, "deployer etat nandu");
    }

    #[test]
    fn finds_titles_without_accents() {
        let hits = search(Lang::Fr, "etat partage");
        assert_eq!(hits.first().map(|h| h.slug), Some("state"));
        let hits = search(Lang::Es, "movil");
        assert_eq!(hits.first().map(|h| h.slug), Some("heroku-mobile"));
        assert!(search(Lang::En, "   ").is_empty());
    }

    #[test]
    fn snippets_keep_original_text() {
        let (before, matched, after) = snippet("Le routage d'Express", 3, 7);
        assert_eq!(
            (before.as_str(), matched.as_str(), after.as_str()),
            ("Le ", "routage", " d'Express")
        );
    }
}
