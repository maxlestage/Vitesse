//! Content negotiation: `Accept` and `Accept-Language` (`req.accepts()` in
//! Express, actix-web's `Accept` header and its preferences).

use crate::util::mime_for;

/// An entry of an `Accept*` header, with its quality and position.
struct Entry<'a> {
    value: &'a str,
    q: f32,
    index: usize,
}

/// Splits an `Accept*` header into its entries (`q=0` included, as refusals).
fn entries<'a>(headers: impl Iterator<Item = &'a str>) -> Vec<Entry<'a>> {
    headers
        .flat_map(|h| h.split(','))
        .enumerate()
        .filter_map(|(index, part)| {
            let mut pieces = part.split(';');
            let value = pieces.next()?.trim();
            if value.is_empty() {
                return None;
            }
            let q = pieces
                .filter_map(|p| p.trim().strip_prefix("q="))
                .find_map(|q| q.trim().parse::<f32>().ok())
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
            Some(Entry { value, q, index })
        })
        .collect()
}

/// The MIME type of an offer: `json` → `application/json`, `text/*` as is.
fn mime_of(offer: &str) -> String {
    let offer = offer.trim();
    if offer.contains('/') {
        return offer.to_ascii_lowercase();
    }
    let mime = mime_for(std::path::Path::new(&format!("x.{offer}")));
    mime.split(';').next().unwrap_or(mime).trim().to_owned()
}

/// How well an `Accept` entry matches a MIME type: `None` if it doesn't,
/// otherwise its specificity (`*/*` < `text/*` < `text/html`).
fn media_match(accept: &str, mime: &str) -> Option<u8> {
    let accept = accept.to_ascii_lowercase();
    let (a_main, a_sub) = accept.split_once('/')?;
    let (m_main, m_sub) = mime.split_once('/')?;
    let main = a_main == "*" || a_main == m_main;
    let sub = a_sub == "*" || a_sub == m_sub || m_sub == "*";
    if !(main && sub) {
        return None;
    }
    Some(u8::from(a_main != "*") + u8::from(a_sub != "*"))
}

/// How well an `Accept-Language` entry matches an offered language.
fn language_match(accept: &str, offer: &str) -> Option<u8> {
    let primary = |tag: &str| tag.split('-').next().unwrap_or(tag).to_ascii_lowercase();
    if accept == "*" {
        Some(0)
    } else if accept.eq_ignore_ascii_case(offer) {
        Some(2)
    } else if primary(accept) == primary(offer) {
        Some(1)
    } else {
        None
    }
}

/// The preferred offer: the highest quality, then the most specific match,
/// then the first entry of the header, then the first offer (like the
/// `negotiator` package behind Express). Without any header, the first
/// offer.
fn best<'o, 'h>(
    headers: impl Iterator<Item = &'h str>,
    offers: &[&'o str],
    matches: impl Fn(&str, &str) -> Option<u8>,
    key: impl Fn(&str) -> String,
) -> Option<&'o str> {
    let entries = entries(headers);
    if entries.is_empty() {
        return offers.first().copied();
    }
    let mut best: Option<(f32, u8, usize, usize)> = None;
    let mut choice = None;
    for (i, offer) in offers.iter().enumerate() {
        let key = key(offer);
        // L'entrée la plus précise qui correspond décide de la qualité.
        let Some((s, entry)) = entries
            .iter()
            .filter_map(|e| matches(e.value, &key).map(|s| (s, e)))
            .max_by_key(|(s, e)| (*s, std::cmp::Reverse(e.index)))
        else {
            continue;
        };
        if entry.q <= 0.0 {
            continue;
        }
        let rank = (entry.q, s, entry.index, i);
        let better = match best {
            None => true,
            Some((q, bs, bi, bo)) => {
                rank.0 > q
                    || (rank.0 == q
                        && (s > bs
                            || (s == bs && (entry.index < bi || (entry.index == bi && i < bo)))))
            }
        };
        if better {
            best = Some(rank);
            choice = Some(*offer);
        }
    }
    choice
}

/// The preferred type among `offers`, for the `Accept` headers.
pub(crate) fn accepts<'o, 'h>(
    headers: impl Iterator<Item = &'h str>,
    offers: &[&'o str],
) -> Option<&'o str> {
    best(headers, offers, media_match, mime_of)
}

/// The preferred language among `offers`, for the `Accept-Language` headers.
pub(crate) fn accepts_language<'o, 'h>(
    headers: impl Iterator<Item = &'h str>,
    offers: &[&'o str],
) -> Option<&'o str> {
    best(headers, offers, language_match, |o| o.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick<'o>(accept: &str, offers: &[&'o str]) -> Option<&'o str> {
        accepts([accept].into_iter().filter(|a| !a.is_empty()), offers)
    }

    #[test]
    fn media_types_like_express() {
        let browser = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";
        assert_eq!(pick(browser, &["json", "html"]), Some("html"));
        assert_eq!(pick("application/json", &["html", "json"]), Some("json"));
        assert_eq!(pick("application/json", &["html"]), None);
        assert_eq!(pick("*/*", &["json", "html"]), Some("json"));
        assert_eq!(pick("", &["json", "html"]), Some("json"));
        assert_eq!(
            pick("text/*;q=0.5, application/json", &["text/plain", "json"]),
            Some("json")
        );
        assert_eq!(
            pick("text/*, text/plain;q=0", &["text/plain", "html"]),
            Some("html")
        );
        assert_eq!(pick("image/*", &["image/png", "html"]), Some("image/png"));
        assert_eq!(pick("TEXT/HTML", &["text/html"]), Some("text/html"));
        assert_eq!(
            pick("text/html;level=1;q=0.2, text/plain", &["html", "txt"]),
            Some("txt")
        );
    }

    #[test]
    fn languages() {
        let lang = |accept: &str, offers: &[&'static str]| {
            accepts_language([accept].into_iter().filter(|a| !a.is_empty()), offers)
        };
        assert_eq!(
            lang("fr-FR,fr;q=0.9,en;q=0.8", &["en", "fr", "es"]),
            Some("fr")
        );
        assert_eq!(lang("es-MX", &["en", "es"]), Some("es"));
        assert_eq!(lang("de", &["en", "fr"]), None);
        assert_eq!(lang("de, *;q=0.1", &["en", "fr"]), Some("en"));
        assert_eq!(lang("", &["en", "fr"]), Some("en"));
        assert_eq!(
            lang("en-GB, en-US;q=0.9", &["en-US", "en-GB"]),
            Some("en-GB")
        );
    }
}
