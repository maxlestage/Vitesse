//! Arbre de routage par segments (trie) avec paramètres `:nom` et joker `*`.
//!
//! La recherche ne fait aucune allocation tant que la route n'a pas de
//! paramètre : on parcourt le chemin segment par segment, avec la priorité
//! *statique > paramètre > joker* et un retour arrière si une branche échoue.

/// Un segment de motif de route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Segment<'a> {
    Static(&'a str),
    Param(&'a str),
    Wildcard(&'a str),
}

/// Découpe un motif comme `/users/:id/files/*path` en segments.
pub(crate) fn parse_pattern(pattern: &str) -> Result<Vec<Segment<'_>>, String> {
    let Some(body) = pattern.strip_prefix('/') else {
        return Err(format!("le chemin « {pattern} » doit commencer par '/'"));
    };
    let body = body.strip_suffix('/').unwrap_or(body);
    if body.is_empty() {
        return Ok(Vec::new());
    }
    let parts: Vec<&str> = body.split('/').collect();
    let last = parts.len() - 1;
    let mut segments = Vec::with_capacity(parts.len());
    for (i, part) in parts.into_iter().enumerate() {
        if part.is_empty() {
            return Err(format!("le chemin « {pattern} » contient un segment vide"));
        }
        if let Some(name) = part.strip_prefix(':') {
            if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                return Err(format!(
                    "paramètre invalide « {part} » dans « {pattern} » (lettres, chiffres et _ uniquement)"
                ));
            }
            segments.push(Segment::Param(name));
        } else if let Some(name) = part.strip_prefix('*') {
            if i != last {
                return Err(format!(
                    "le joker « {part} » doit être le dernier segment de « {pattern} »"
                ));
            }
            segments.push(Segment::Wildcard(if name.is_empty() { "*" } else { name }));
        } else {
            segments.push(Segment::Static(part));
        }
    }
    Ok(segments)
}

/// Les noms des paramètres d'un motif, dans l'ordre.
pub(crate) fn param_names(segments: &[Segment<'_>]) -> Box<[Box<str>]> {
    segments
        .iter()
        .filter_map(|s| match s {
            Segment::Param(n) | Segment::Wildcard(n) => Some(Box::from(*n)),
            Segment::Static(_) => None,
        })
        .collect()
}

/// Positions (début, fin) des valeurs de paramètres dans le chemin.
pub(crate) type Captures = Vec<(usize, usize)>;

pub(crate) struct Tree<T> {
    root: Node,
    values: Vec<T>,
}

#[derive(Default)]
struct Node {
    statics: Vec<(Box<str>, Node)>,
    param: Option<Box<Node>>,
    wildcard: Option<usize>,
    value: Option<usize>,
}

impl<T> Tree<T> {
    pub(crate) fn new() -> Self {
        Tree {
            root: Node::default(),
            values: Vec::new(),
        }
    }

    /// Renvoie la valeur associée au motif, en la créant au besoin.
    pub(crate) fn entry(
        &mut self,
        segments: &[Segment<'_>],
        default: impl FnOnce() -> T,
    ) -> &mut T {
        let Tree { root, values } = self;
        let mut node = root;
        for seg in segments {
            match *seg {
                Segment::Static(s) => {
                    let pos = match node.statics.iter().position(|(k, _)| **k == *s) {
                        Some(p) => p,
                        None => {
                            node.statics.push((Box::from(s), Node::default()));
                            node.statics.len() - 1
                        }
                    };
                    node = &mut node.statics[pos].1;
                }
                Segment::Param(_) => {
                    node = node.param.get_or_insert_with(Default::default);
                }
                Segment::Wildcard(_) => {
                    let idx = *node.wildcard.get_or_insert_with(|| {
                        values.push(default());
                        values.len() - 1
                    });
                    return &mut values[idx];
                }
            }
        }
        let idx = *node.value.get_or_insert_with(|| {
            values.push(default());
            values.len() - 1
        });
        &mut values[idx]
    }

    /// Cherche la route correspondant au chemin `path` (qui commence par `/`).
    #[inline]
    pub(crate) fn find(&self, path: &str) -> Option<(&T, Captures)> {
        // `/users/` est traité comme `/users` (mode non strict, comme Express).
        let path = if path.len() > 1 {
            path.strip_suffix('/').unwrap_or(path)
        } else {
            path
        };
        let rest = path.strip_prefix('/')?;
        let rest = if rest.is_empty() { None } else { Some(rest) };
        let mut captures = Vec::new();
        let idx = self.root.find(rest, 1, &mut captures)?;
        Some((&self.values[idx], captures))
    }
}

impl Node {
    fn find(&self, rest: Option<&str>, offset: usize, captures: &mut Captures) -> Option<usize> {
        let Some(rest) = rest else {
            if self.value.is_some() {
                return self.value;
            }
            // `/files/*path` accepte aussi `/files` (joker vide).
            if let Some(w) = self.wildcard {
                captures.push((offset, offset));
                return Some(w);
            }
            return None;
        };

        let (seg, next, next_offset) = match rest.find('/') {
            Some(i) => (&rest[..i], Some(&rest[i + 1..]), offset + i + 1),
            None => (rest, None, offset + rest.len()),
        };

        if let Some((_, child)) = self.statics.iter().find(|(k, _)| **k == *seg) {
            if let Some(v) = child.find(next, next_offset, captures) {
                return Some(v);
            }
        }

        if let Some(child) = &self.param {
            if !seg.is_empty() {
                captures.push((offset, offset + seg.len()));
                if let Some(v) = child.find(next, next_offset, captures) {
                    return Some(v);
                }
                captures.pop();
            }
        }

        if let Some(w) = self.wildcard {
            captures.push((offset, offset + rest.len()));
            return Some(w);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(routes: &[&str]) -> Tree<&'static str> {
        let mut t = Tree::new();
        for r in routes {
            let r: &'static str = Box::leak(r.to_string().into_boxed_str());
            let segs = parse_pattern(r).unwrap();
            *t.entry(&segs, || "") = r;
        }
        t
    }

    fn lookup<'a>(t: &Tree<&'static str>, path: &'a str) -> Option<(&'static str, Vec<&'a str>)> {
        t.find(path)
            .map(|(v, caps)| (*v, caps.into_iter().map(|(a, b)| &path[a..b]).collect()))
    }

    #[test]
    fn patterns() {
        assert_eq!(parse_pattern("/").unwrap(), vec![]);
        assert_eq!(
            parse_pattern("/a/:id/*rest").unwrap(),
            vec![
                Segment::Static("a"),
                Segment::Param("id"),
                Segment::Wildcard("rest")
            ]
        );
        assert_eq!(parse_pattern("/a/").unwrap(), vec![Segment::Static("a")]);
        assert!(parse_pattern("a").is_err());
        assert!(parse_pattern("/a//b").is_err());
        assert!(parse_pattern("/*x/b").is_err());
        assert!(parse_pattern("/:").is_err());
        assert!(parse_pattern("/:a-b").is_err());
    }

    #[test]
    fn matching() {
        let t = tree(&[
            "/",
            "/users",
            "/users/new",
            "/users/:id",
            "/users/:id/posts/:post",
            "/files/*path",
            "/a/:x/c",
            "/a/b/d",
        ]);
        assert_eq!(lookup(&t, "/"), Some(("/", vec![])));
        assert_eq!(lookup(&t, "/users"), Some(("/users", vec![])));
        assert_eq!(lookup(&t, "/users/"), Some(("/users", vec![])));
        assert_eq!(lookup(&t, "/users/new"), Some(("/users/new", vec![])));
        assert_eq!(lookup(&t, "/users/42"), Some(("/users/:id", vec!["42"])));
        assert_eq!(
            lookup(&t, "/users/42/posts/7"),
            Some(("/users/:id/posts/:post", vec!["42", "7"]))
        );
        assert_eq!(lookup(&t, "/users/42/posts"), None);
        assert_eq!(
            lookup(&t, "/files/a/b/c.txt"),
            Some(("/files/*path", vec!["a/b/c.txt"]))
        );
        assert_eq!(lookup(&t, "/files"), Some(("/files/*path", vec![""])));
        // Retour arrière : `b` est statique mais seul `/a/:x/c` correspond.
        assert_eq!(lookup(&t, "/a/b/c"), Some(("/a/:x/c", vec!["b"])));
        assert_eq!(lookup(&t, "/a/b/d"), Some(("/a/b/d", vec![])));
        assert_eq!(lookup(&t, "/nope"), None);
        assert_eq!(lookup(&t, "/users//x"), None);
    }

    #[test]
    fn root_wildcard() {
        let t = tree(&["/*", "/api/:v"]);
        assert_eq!(lookup(&t, "/"), Some(("/*", vec![""])));
        assert_eq!(lookup(&t, "/x/y"), Some(("/*", vec!["x/y"])));
        assert_eq!(lookup(&t, "/api/1"), Some(("/api/:v", vec!["1"])));
        assert_eq!(lookup(&t, "/api/1/2"), Some(("/*", vec!["api/1/2"])));
    }
}
