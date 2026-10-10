//! Coloration syntaxique minimale pour Rust et JavaScript.

/// Les catégories de jetons (chacune a sa classe CSS).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Plain,
    Keyword,
    String,
    Comment,
    Number,
    Macro,
    Type,
    Function,
    Punct,
}

impl Kind {
    pub fn class(self) -> &'static str {
        match self {
            Kind::Plain => "t-plain",
            Kind::Keyword => "t-kw",
            Kind::String => "t-str",
            Kind::Comment => "t-com",
            Kind::Number => "t-num",
            Kind::Macro => "t-mac",
            Kind::Type => "t-type",
            Kind::Function => "t-fn",
            Kind::Punct => "t-punct",
        }
    }
}

const KEYWORDS: &[&str] = &[
    "use",
    "fn",
    "let",
    "mut",
    "async",
    "await",
    "move",
    "return",
    "if",
    "else",
    "match",
    "struct",
    "impl",
    "pub",
    "for",
    "in",
    "const",
    "true",
    "false",
    "Some",
    "None",
    "Ok",
    "Err",
    "self",
    "Self",
    "require",
    "new",
    "function",
    "var",
    "await",
    "typeof",
    "null",
    "undefined",
    "=>",
];

/// Découpe `code` en jetons colorés.
pub fn tokenize(code: &str) -> Vec<(Kind, String)> {
    let chars: Vec<char> = code.chars().collect();
    let mut out: Vec<(Kind, String)> = Vec::new();
    let mut push = |kind: Kind, text: String| match out.last_mut() {
        Some((k, t)) if *k == kind => t.push_str(&text),
        _ => out.push((kind, text)),
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // Commentaire de ligne.
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            let start = i;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            push(Kind::Comment, chars[start..i].iter().collect());
            continue;
        }
        // Chaînes.
        if c == '"' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(chars.len());
            push(Kind::String, chars[start..i].iter().collect());
            continue;
        }
        // Raw strings Rust r#"..."#
        if c == 'r' && chars.get(i + 1) == Some(&'#') && chars.get(i + 2) == Some(&'"') {
            let start = i;
            i += 3;
            while i + 1 < chars.len() && !(chars[i] == '"' && chars[i + 1] == '#') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            push(Kind::String, chars[start..i].iter().collect());
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            push(Kind::Number, chars[start..i].iter().collect());
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let kind = if chars.get(i) == Some(&'!') {
                i += 1;
                push(Kind::Macro, format!("{word}!"));
                continue;
            } else if KEYWORDS.contains(&word.as_str()) {
                Kind::Keyword
            } else if word.chars().next().is_some_and(char::is_uppercase) {
                Kind::Type
            } else if chars.get(i) == Some(&'(') {
                Kind::Function
            } else {
                Kind::Plain
            };
            push(kind, word);
            continue;
        }
        if c == '=' && chars.get(i + 1) == Some(&'>') {
            push(Kind::Keyword, "=>".into());
            i += 2;
            continue;
        }
        if c == '#' && chars.get(i + 1) == Some(&'[') {
            let start = i;
            while i < chars.len() && chars[i] != ']' {
                i += 1;
            }
            i = (i + 1).min(chars.len());
            push(Kind::Macro, chars[start..i].iter().collect());
            continue;
        }
        let kind = if "{}()[];,.:<>|&?=!+-*/".contains(c) {
            Kind::Punct
        } else {
            Kind::Plain
        };
        push(kind, c.to_string());
        i += 1;
    }
    out
}

/// Ne garde que les `limit` premiers caractères des jetons.
pub fn truncate(tokens: &[(Kind, String)], limit: usize) -> Vec<(Kind, String)> {
    let mut left = limit;
    let mut out = Vec::new();
    for (kind, text) in tokens {
        if left == 0 {
            break;
        }
        let n = text.chars().count();
        if n <= left {
            out.push((*kind, text.clone()));
            left -= n;
        } else {
            out.push((*kind, text.chars().take(left).collect()));
            left = 0;
        }
    }
    out
}
