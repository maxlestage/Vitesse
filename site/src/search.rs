//! La recherche dans la documentation, partagée par les deux côtés : le serveur
//! écrit un index par langue (un petit fichier texte, pour que le site marche
//! aussi en fichiers statiques), le navigateur le télécharge une fois et cherche
//! dedans à chaque touche. Insensible à la casse et aux accents.

/// Une cible de recherche : une page (son titre et son résumé) ou une section
/// d'une page (son titre et son texte).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Une page entière (titre et résumé), classée avant ses sections.
    pub page: bool,
    pub href: String,
    /// Le titre de la page.
    pub title: String,
    /// Le titre de la section ; vide pour la page et pour son introduction.
    pub section: String,
    /// Le résumé de la page, ou le texte brut de la section.
    pub text: String,
}

/// Au plus autant de résultats.
pub const MAX: usize = 12;

impl Entry {
    /// Une ligne de l'index : `p|s \t href \t titre \t section \t texte`.
    pub fn line(&self) -> String {
        let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
        format!(
            "{}\t{}\t{}\t{}\t{}\n",
            if self.page { "p" } else { "s" },
            clean(&self.href),
            clean(&self.title),
            clean(&self.section),
            clean(&self.text),
        )
    }

    pub fn parse(line: &str) -> Option<Entry> {
        let mut f = line.split('\t');
        Some(Entry {
            page: f.next()? == "p",
            href: f.next()?.to_owned(),
            title: f.next()?.to_owned(),
            section: f.next()?.to_owned(),
            text: f.next()?.to_owned(),
        })
    }
}

/// Un résultat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub href: String,
    pub title: String,
    pub section: Option<String>,
    /// Avant, correspondance, après.
    pub snippet: Option<(String, String, String)>,
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

/// Le passage autour de la correspondance.
pub fn snippet(text: &str, at: usize, len: usize) -> (String, String, String) {
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

/// Les résultats de `query`, les meilleurs d'abord : titres de pages, résumés,
/// titres de sections, puis le texte (deux passages au plus par page).
pub fn search(entries: &[Entry], query: &str) -> Vec<Hit> {
    let needle = fold(query.trim());
    if needle.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(u32, Hit)> = Vec::new();
    let mut per_page: Vec<(&str, usize)> = Vec::new();
    for entry in entries {
        if entry.page {
            let score = match find(&fold(&entry.title), &needle) {
                Some(0) => 120,
                Some(_) => 100,
                None if find(&fold(&entry.text), &needle).is_some() => 60,
                None => continue,
            };
            hits.push((
                score,
                Hit {
                    href: entry.href.clone(),
                    title: entry.title.clone(),
                    section: None,
                    snippet: None,
                },
            ));
            continue;
        }
        let found = if !entry.section.is_empty() && find(&fold(&entry.section), &needle).is_some() {
            Some((50, None))
        } else {
            find(&fold(&entry.text), &needle)
                .map(|at| (30, Some(snippet(&entry.text, at, needle.len()))))
        };
        let Some((score, snippet)) = found else {
            continue;
        };
        let page = entry.href.split('#').next().unwrap_or_default();
        let count = match per_page.iter_mut().find(|(p, _)| *p == page) {
            Some((_, n)) => n,
            None => {
                per_page.push((page, 0));
                &mut per_page.last_mut().expect("just pushed").1
            }
        };
        if *count >= 2 {
            continue;
        }
        *count += 1;
        hits.push((
            score,
            Hit {
                href: entry.href.clone(),
                title: entry.title.clone(),
                section: (!entry.section.is_empty()).then(|| entry.section.clone()),
                snippet,
            },
        ));
    }
    // Stable : à score égal, l'ordre de lecture.
    hits.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    hits.into_iter().take(MAX).map(|(_, hit)| hit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(href: &str, title: &str, section: &str, text: &str) -> Entry {
        Entry {
            page: section.is_empty(),
            href: href.into(),
            title: title.into(),
            section: section.into(),
            text: text.into(),
        }
    }

    fn index() -> Vec<Entry> {
        vec![
            entry("/fr/docs/state/", "État partagé", "", "Partager une base."),
            entry(
                "/fr/docs/routing/",
                "Routage",
                "",
                "Méthodes et paramètres.",
            ),
            entry(
                "/fr/docs/routing/#jokers",
                "Routage",
                "Jokers",
                "Un joker capture la fin du chemin, comme l'état d'une page.",
            ),
            entry(
                "/fr/docs/routing/#priorite",
                "Routage",
                "Priorité",
                "L'état statique passe avant.",
            ),
            entry(
                "/fr/docs/routing/#autre",
                "Routage",
                "Autre",
                "Encore un état.",
            ),
        ]
    }

    #[test]
    fn folds_accents_and_case() {
        let f: String = fold("Déployer ÉTAT Ñandú").into_iter().collect();
        assert_eq!(f, "deployer etat nandu");
    }

    #[test]
    fn ranks_titles_then_sections_then_text() {
        let hits = search(&index(), "etat");
        assert_eq!(hits[0].href, "/fr/docs/state/");
        assert_eq!(hits[0].section, None);
        // Deux passages au plus par page.
        assert_eq!(hits.len(), 3);
        let (before, matched, _) = hits[1].snippet.clone().unwrap();
        assert_eq!(matched, "état");
        assert!(before.ends_with("comme l'"));
        assert_eq!(
            search(&index(), "jokers")[0].section.as_deref(),
            Some("Jokers")
        );
        assert!(search(&index(), "   ").is_empty());
        assert!(search(&index(), "nothing").is_empty());
    }

    #[test]
    fn round_trips_lines() {
        let e = entry("/en/docs/x/#a", "Deploy\tto Heroku", "Sec", "a\nb");
        let back = Entry::parse(e.line().trim_end_matches('\n')).unwrap();
        assert_eq!(back.title, "Deploy to Heroku");
        assert_eq!(back.text, "a b");
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
