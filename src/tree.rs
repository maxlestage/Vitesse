//! Segment-based routing tree (trie) with `:name` parameters, `*` wildcards,
//! optional parameters (`:name?`), several parameters within one segment
//! (`:from-:to`, `:name.:ext`) and constraints (`:id(\d+)`).
//!
//! Lookups make no allocation as long as the route has no parameters: the
//! path is walked segment by segment, with the priority
//! *static > constrained or composite segment > parameter > wildcard*,
//! backtracking when a branch fails. Regular expressions are only evaluated
//! for the parameters that have a constraint.

use std::fmt;

use regex_lite::Regex;

/// A segment of a route pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Segment<'a> {
    Static(&'a str),
    /// `:name`, `:name(regex)` or `:name?`.
    Param {
        name: &'a str,
        constraint: Option<&'a str>,
        optional: bool,
    },
    /// Text and parameters within a single segment: `:from-:to`,
    /// `:name.:ext`, `v:version`.
    Pattern(Vec<Part<'a>>),
    Wildcard(&'a str),
}

/// A piece of a [`Segment::Pattern`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Part<'a> {
    Lit(&'a str),
    Param {
        name: &'a str,
        constraint: Option<&'a str>,
    },
}

/// Splits a pattern such as `/users/:id/files/*path` into segments.
pub(crate) fn parse_pattern(pattern: &str) -> Result<Vec<Segment<'_>>, String> {
    let Some(body) = pattern.strip_prefix('/') else {
        return Err(format!("the path '{pattern}' must start with '/'"));
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
            return Err(format!("the path '{pattern}' contains an empty segment"));
        }
        let segment = parse_segment(part, pattern)?;
        if matches!(segment, Segment::Wildcard(_)) && i != last {
            return Err(format!(
                "the wildcard '{part}' must be the last segment of '{pattern}'"
            ));
        }
        segments.push(segment);
    }
    // Les paramètres optionnels ne peuvent être qu'à la fin : `/archive/:year?/:month?`.
    if let Some(first) = segments.iter().position(is_optional) {
        if !segments[first..].iter().all(is_optional) {
            return Err(format!(
                "in '{pattern}', only the last segments can be optional"
            ));
        }
    }
    for constraint in segments.iter().flat_map(constraints) {
        compile(constraint).map_err(|e| {
            format!("invalid regular expression '({constraint})' in '{pattern}': {e}")
        })?;
    }
    Ok(segments)
}

fn is_optional(segment: &Segment<'_>) -> bool {
    matches!(segment, Segment::Param { optional: true, .. })
}

/// The constraints of a segment.
fn constraints<'a>(segment: &Segment<'a>) -> Vec<&'a str> {
    match segment {
        Segment::Param {
            constraint: Some(c),
            ..
        } => vec![*c],
        Segment::Pattern(parts) => parts
            .iter()
            .filter_map(|p| match p {
                Part::Param { constraint, .. } => *constraint,
                Part::Lit(_) => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A constraint must match the whole value of the parameter.
fn compile(constraint: &str) -> Result<Regex, regex_lite::Error> {
    Regex::new(&format!("^(?:{constraint})$"))
}

fn parse_segment<'a>(part: &'a str, pattern: &str) -> Result<Segment<'a>, String> {
    if let Some(name) = part.strip_prefix('*') {
        if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(format!(
                "invalid wildcard '{part}' in '{pattern}' (only letters, digits and _ are allowed)"
            ));
        }
        return Ok(Segment::Wildcard(if name.is_empty() { "*" } else { name }));
    }
    if !part.contains(':') {
        return Ok(Segment::Static(part));
    }
    let mut parts = Vec::new();
    let mut optional = false;
    let mut rest = part;
    while !rest.is_empty() {
        if optional {
            return Err(format!(
                "in '{pattern}', only a whole segment can be optional ('/:id?')"
            ));
        }
        if let Some(after) = rest.strip_prefix(':') {
            let len = after
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .count();
            if len == 0 {
                return Err(format!(
                    "invalid parameter in '{pattern}': ':' must be followed by a name (letters, digits and _)"
                ));
            }
            let name = &after[..len];
            rest = &after[len..];
            let mut constraint = None;
            if rest.starts_with('(') {
                let end = closing_paren(rest)
                    .ok_or_else(|| format!("unclosed '(' after ':{name}' in '{pattern}'"))?;
                constraint = Some(&rest[1..end]);
                rest = &rest[end + 1..];
            }
            if let Some(after) = rest.strip_prefix('?') {
                optional = true;
                rest = after;
            }
            if matches!(parts.last(), Some(Part::Param { .. })) {
                return Err(format!(
                    "in '{pattern}', two parameters must be separated by text, as in ':from-:to'"
                ));
            }
            parts.push(Part::Param { name, constraint });
        } else {
            let end = rest.find(':').unwrap_or(rest.len());
            parts.push(Part::Lit(&rest[..end]));
            rest = &rest[end..];
        }
    }
    match parts.as_slice() {
        [Part::Param { name, constraint }] => Ok(Segment::Param {
            name,
            constraint: *constraint,
            optional,
        }),
        _ if optional => Err(format!(
            "in '{pattern}', only a whole segment can be optional ('/:id?')"
        )),
        _ => Ok(Segment::Pattern(parts)),
    }
}

/// The position of the `)` that closes the `(` at the start of `s`.
fn closing_paren(s: &str) -> Option<usize> {
    let mut depth = 0;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// The routes a pattern with optional parameters stands for: `/a/:b?/:c?`
/// gives `/a`, `/a/:b` and `/a/:b/:c`.
pub(crate) fn variants<'a>(segments: &[Segment<'a>]) -> Vec<Vec<Segment<'a>>> {
    let required = segments
        .iter()
        .position(is_optional)
        .unwrap_or(segments.len());
    (required..=segments.len())
        .map(|n| {
            segments[..n]
                .iter()
                .cloned()
                .map(|s| match s {
                    Segment::Param {
                        name, constraint, ..
                    } => Segment::Param {
                        name,
                        constraint,
                        optional: false,
                    },
                    s => s,
                })
                .collect()
        })
        .collect()
}

/// The names of the parameters of a pattern, in order.
pub(crate) fn param_names(segments: &[Segment<'_>]) -> Box<[Box<str>]> {
    let mut names = Vec::new();
    for segment in segments {
        match segment {
            Segment::Param { name, .. } | Segment::Wildcard(name) => names.push(Box::from(*name)),
            Segment::Pattern(parts) => {
                for part in parts {
                    if let Part::Param { name, .. } = part {
                        names.push(Box::from(*name));
                    }
                }
            }
            Segment::Static(_) => {}
        }
    }
    names.into_boxed_slice()
}

/// Builds the path of a pattern from the values of its parameters, in
/// order (see [`Request::path_for`](crate::Request::path_for)).
pub(crate) fn build_path(pattern: &str, values: &[&dyn fmt::Display]) -> Result<String, String> {
    let segments = parse_pattern(pattern)?;
    let mut values = values.iter().map(|v| v.to_string());
    let mut out = String::new();
    let check = |name: &str, constraint: Option<&str>, value: &str| match constraint {
        Some(c) if !compile(c).is_ok_and(|re| re.is_match(value)) => Err(format!(
            "'{value}' does not satisfy the constraint of ':{name}({c})'"
        )),
        _ => Ok(()),
    };
    for segment in &segments {
        match segment {
            Segment::Static(s) => {
                out.push('/');
                out.push_str(s);
            }
            Segment::Param {
                name,
                constraint,
                optional,
            } => {
                let Some(value) = values.next() else {
                    if *optional {
                        break;
                    }
                    return Err(format!("missing a value for ':{name}'"));
                };
                check(name, *constraint, &value)?;
                out.push('/');
                encode_into(&mut out, &value, false);
            }
            Segment::Pattern(parts) => {
                out.push('/');
                for part in parts {
                    match part {
                        Part::Lit(lit) => out.push_str(lit),
                        Part::Param { name, constraint } => {
                            let value = values
                                .next()
                                .ok_or_else(|| format!("missing a value for ':{name}'"))?;
                            if value.is_empty() {
                                return Err(format!("':{name}' cannot be empty"));
                            }
                            check(name, *constraint, &value)?;
                            encode_into(&mut out, &value, false);
                        }
                    }
                }
            }
            Segment::Wildcard(_) => {
                if let Some(value) = values.next() {
                    let value = value.trim_start_matches('/');
                    if !value.is_empty() {
                        out.push('/');
                        encode_into(&mut out, value, true);
                    }
                }
            }
        }
    }
    if values.next().is_some() {
        return Err("too many values".into());
    }
    if out.is_empty() {
        out.push('/');
    }
    Ok(out)
}

/// Percent-encodes a parameter value for a path (keeping `/` for a
/// wildcard).
fn encode_into(out: &mut String, value: &str, keep_slashes: bool) {
    for b in value.bytes() {
        let keep = b.is_ascii_alphanumeric()
            || b"-._~!$&'()*+,;=:@".contains(&b)
            || (keep_slashes && b == b'/');
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
}

/// Positions (start, end) of the parameter values in the path.
pub(crate) type Captures = Vec<(usize, usize)>;

pub(crate) struct Tree<T> {
    root: Node,
    values: Vec<T>,
    case_insensitive: bool,
}

#[derive(Default)]
struct Node {
    statics: Vec<(Box<str>, Node)>,
    /// Constrained parameters and composite segments, in registration order.
    patterns: Vec<(Matcher, Node)>,
    param: Option<Box<Node>>,
    wildcard: Option<usize>,
    value: Option<usize>,
}

/// Matches a constrained parameter or a composite segment.
struct Matcher {
    parts: Box<[Piece]>,
}

enum Piece {
    Lit(Box<str>),
    Param(Option<Regex>),
}

impl Matcher {
    fn new(parts: &[Part<'_>]) -> Matcher {
        let parts = parts
            .iter()
            .map(|p| match p {
                Part::Lit(lit) => Piece::Lit(Box::from(*lit)),
                Part::Param { constraint, .. } => Piece::Param(
                    constraint
                        .map(|c| compile(c).expect("constraints are checked by parse_pattern")),
                ),
            })
            .collect();
        Matcher { parts }
    }

    /// Same structure (the names of the parameters don't matter).
    fn same(&self, parts: &[Part<'_>]) -> bool {
        self.parts.len() == parts.len()
            && self
                .parts
                .iter()
                .zip(parts)
                .all(|(piece, part)| match (piece, part) {
                    (Piece::Lit(a), Part::Lit(b)) => **a == **b,
                    (Piece::Param(re), Part::Param { constraint, .. }) => {
                        re.as_ref().map(|r| r.as_str().to_owned())
                            == constraint.map(|c| format!("^(?:{c})$"))
                    }
                    _ => false,
                })
    }

    fn matches(&self, segment: &str, offset: usize, captures: &mut Captures, ci: bool) -> bool {
        match_pieces(&self.parts, segment, 0, offset, captures, ci)
    }
}

/// Matches `pieces` against `segment[pos..]`. A parameter takes as few
/// characters as possible (at least one), like Express: `:name.:ext`
/// splits `archive.tar.gz` into `archive` and `tar.gz`.
fn match_pieces(
    pieces: &[Piece],
    segment: &str,
    pos: usize,
    offset: usize,
    captures: &mut Captures,
    ci: bool,
) -> bool {
    let Some((first, rest)) = pieces.split_first() else {
        return pos == segment.len();
    };
    match first {
        Piece::Lit(lit) => {
            let tail = &segment.as_bytes()[pos..];
            let found = tail.len() >= lit.len()
                && if ci {
                    tail[..lit.len()].eq_ignore_ascii_case(lit.as_bytes())
                } else {
                    tail[..lit.len()] == *lit.as_bytes()
                };
            found && match_pieces(rest, segment, pos + lit.len(), offset, captures, ci)
        }
        Piece::Param(constraint) => {
            let start = if rest.is_empty() {
                segment.len()
            } else {
                pos + 1
            };
            for end in (start..=segment.len()).filter(|&e| segment.is_char_boundary(e)) {
                if end == pos {
                    break;
                }
                if constraint
                    .as_ref()
                    .is_some_and(|re| !re.is_match(&segment[pos..end]))
                {
                    continue;
                }
                captures.push((offset + pos, offset + end));
                if match_pieces(rest, segment, end, offset, captures, ci) {
                    return true;
                }
                captures.pop();
            }
            false
        }
    }
}

impl<T> Tree<T> {
    pub(crate) fn new() -> Self {
        Tree {
            root: Node::default(),
            values: Vec::new(),
            case_insensitive: false,
        }
    }

    /// Matches static segments without regard to ASCII case (`/Users` is
    /// `/users`).
    pub(crate) fn set_case_insensitive(&mut self, insensitive: bool) {
        self.case_insensitive = insensitive;
    }

    pub(crate) fn is_case_insensitive(&self) -> bool {
        self.case_insensitive
    }

    /// Returns the value associated with the pattern, creating it if needed.
    pub(crate) fn entry(
        &mut self,
        segments: &[Segment<'_>],
        default: impl FnOnce() -> T,
    ) -> &mut T {
        let Tree { root, values, .. } = self;
        let mut node = root;
        for seg in segments {
            match seg {
                Segment::Static(s) => {
                    let pos = match node.statics.iter().position(|(k, _)| **k == **s) {
                        Some(p) => p,
                        None => {
                            node.statics.push((Box::from(*s), Node::default()));
                            node.statics.len() - 1
                        }
                    };
                    node = &mut node.statics[pos].1;
                }
                Segment::Param {
                    constraint: None, ..
                } => {
                    node = node.param.get_or_insert_with(Default::default);
                }
                Segment::Param {
                    name,
                    constraint: Some(c),
                    ..
                } => {
                    node = node.pattern(&[Part::Param {
                        name,
                        constraint: Some(c),
                    }]);
                }
                Segment::Pattern(parts) => node = node.pattern(parts),
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

    /// Looks up the route matching `path` (which starts with `/`). The
    /// positions of the parameters are written into `captures`, a buffer
    /// provided by the caller to avoid any allocation.
    #[inline]
    pub(crate) fn find(&self, path: &str, captures: &mut Captures) -> Option<&T> {
        captures.clear();
        // `/users/` est traité comme `/users` (mode non strict, comme Express).
        let path = if path.len() > 1 {
            path.strip_suffix('/').unwrap_or(path)
        } else {
            path
        };
        let rest = path.strip_prefix('/')?;
        let rest = if rest.is_empty() { None } else { Some(rest) };
        let idx = self.root.find(rest, 1, captures, self.case_insensitive)?;
        Some(&self.values[idx])
    }
}

impl Node {
    /// The child for a constrained or composite segment.
    fn pattern(&mut self, parts: &[Part<'_>]) -> &mut Node {
        let pos = match self.patterns.iter().position(|(m, _)| m.same(parts)) {
            Some(p) => p,
            None => {
                self.patterns.push((Matcher::new(parts), Node::default()));
                self.patterns.len() - 1
            }
        };
        &mut self.patterns[pos].1
    }

    fn find(
        &self,
        rest: Option<&str>,
        offset: usize,
        captures: &mut Captures,
        ci: bool,
    ) -> Option<usize> {
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

        let child = if ci {
            self.statics
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(seg))
        } else {
            self.statics.iter().find(|(k, _)| **k == *seg)
        };
        if let Some((_, child)) = child {
            if let Some(v) = child.find(next, next_offset, captures, ci) {
                return Some(v);
            }
        }

        for (matcher, child) in &self.patterns {
            let mark = captures.len();
            if matcher.matches(seg, offset, captures, ci) {
                if let Some(v) = child.find(next, next_offset, captures, ci) {
                    return Some(v);
                }
            }
            captures.truncate(mark);
        }

        if let Some(child) = &self.param {
            if !seg.is_empty() {
                captures.push((offset, offset + seg.len()));
                if let Some(v) = child.find(next, next_offset, captures, ci) {
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
            for variant in variants(&segs) {
                *t.entry(&variant, || "") = r;
            }
        }
        t
    }

    fn lookup<'a>(t: &Tree<&'static str>, path: &'a str) -> Option<(&'static str, Vec<&'a str>)> {
        let mut caps = Vec::new();
        t.find(path, &mut caps)
            .map(|v| (*v, caps.into_iter().map(|(a, b)| &path[a..b]).collect()))
    }

    fn param(name: &str) -> Segment<'_> {
        Segment::Param {
            name,
            constraint: None,
            optional: false,
        }
    }

    #[test]
    fn patterns() {
        assert_eq!(parse_pattern("/").unwrap(), vec![]);
        assert_eq!(
            parse_pattern("/a/:id/*rest").unwrap(),
            vec![Segment::Static("a"), param("id"), Segment::Wildcard("rest")]
        );
        assert_eq!(parse_pattern("/a/").unwrap(), vec![Segment::Static("a")]);
        assert!(parse_pattern("a").is_err());
        assert!(parse_pattern("/a//b").is_err());
        assert!(parse_pattern("/*x/b").is_err());
        assert!(parse_pattern("/:").is_err());
        assert!(parse_pattern("/:a:b").is_err());
        assert!(parse_pattern("/:id(\\d+").is_err());
        assert!(parse_pattern("/:id([)").is_err());
        assert!(parse_pattern("/:a?/b").is_err());
        assert!(parse_pattern("/x-:a?").is_err());
        assert!(parse_pattern("/*a-b").is_err());
        assert_eq!(
            parse_pattern("/:from-:to").unwrap(),
            vec![Segment::Pattern(vec![
                Part::Param {
                    name: "from",
                    constraint: None
                },
                Part::Lit("-"),
                Part::Param {
                    name: "to",
                    constraint: None
                },
            ])]
        );
        assert_eq!(
            parse_pattern("/:id(\\d+)?").unwrap(),
            vec![Segment::Param {
                name: "id",
                constraint: Some("\\d+"),
                optional: true
            }]
        );
        assert_eq!(
            param_names(&parse_pattern("/v:version/:from-:to/*rest").unwrap()),
            ["version", "from", "to", "rest"].map(Box::from).into()
        );
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

    #[test]
    fn several_parameters_in_a_segment() {
        let t = tree(&[
            "/flights/:from-:to",
            "/files/:name.:ext",
            "/api/v:version/users",
            "/img/:id.png",
            "/img/:name",
        ]);
        assert_eq!(
            lookup(&t, "/flights/LAX-SFO"),
            Some(("/flights/:from-:to", vec!["LAX", "SFO"]))
        );
        // Le premier paramètre prend le moins possible, comme Express.
        assert_eq!(
            lookup(&t, "/files/archive.tar.gz"),
            Some(("/files/:name.:ext", vec!["archive", "tar.gz"]))
        );
        assert_eq!(lookup(&t, "/files/noext"), None);
        assert_eq!(lookup(&t, "/files/.env"), None);
        assert_eq!(
            lookup(&t, "/api/v2/users"),
            Some(("/api/v:version/users", vec!["2"]))
        );
        assert_eq!(lookup(&t, "/api/2/users"), None);
        // Le segment composé passe avant le paramètre simple.
        assert_eq!(
            lookup(&t, "/img/42.png"),
            Some(("/img/:id.png", vec!["42"]))
        );
        assert_eq!(
            lookup(&t, "/img/42.jpg"),
            Some(("/img/:name", vec!["42.jpg"]))
        );
        assert_eq!(
            lookup(&t, "/flights/été-hiver"),
            Some(("/flights/:from-:to", vec!["été", "hiver"]))
        );
    }

    #[test]
    fn constraints() {
        let t = tree(&[
            "/users/:id(\\d+)",
            "/users/:name",
            "/posts/:year(\\d{4})-:slug",
            "/v/:lang(en|fr)/x",
        ]);
        assert_eq!(
            lookup(&t, "/users/42"),
            Some(("/users/:id(\\d+)", vec!["42"]))
        );
        assert_eq!(
            lookup(&t, "/users/ada"),
            Some(("/users/:name", vec!["ada"]))
        );
        assert_eq!(
            lookup(&t, "/posts/2024-hello-world"),
            Some(("/posts/:year(\\d{4})-:slug", vec!["2024", "hello-world"]))
        );
        assert_eq!(lookup(&t, "/posts/24-hello"), None);
        assert_eq!(
            lookup(&t, "/v/fr/x"),
            Some(("/v/:lang(en|fr)/x", vec!["fr"]))
        );
        // La contrainte porte sur toute la valeur.
        assert_eq!(lookup(&t, "/v/frr/x"), None);
    }

    #[test]
    fn optional_parameters() {
        let t = tree(&["/archive/:year?/:month?", "/users/:id(\\d+)?"]);
        assert_eq!(
            lookup(&t, "/archive"),
            Some(("/archive/:year?/:month?", vec![]))
        );
        assert_eq!(
            lookup(&t, "/archive/2024"),
            Some(("/archive/:year?/:month?", vec!["2024"]))
        );
        assert_eq!(
            lookup(&t, "/archive/2024/05"),
            Some(("/archive/:year?/:month?", vec!["2024", "05"]))
        );
        assert_eq!(lookup(&t, "/users"), Some(("/users/:id(\\d+)?", vec![])));
        assert_eq!(
            lookup(&t, "/users/7"),
            Some(("/users/:id(\\d+)?", vec!["7"]))
        );
        assert_eq!(lookup(&t, "/users/x"), None);
        assert_eq!(variants(&parse_pattern("/a/:b?/:c?").unwrap()).len(), 3);
    }

    #[test]
    fn paths_from_patterns() {
        let build = |p: &str, v: &[&dyn fmt::Display]| build_path(p, v);
        assert_eq!(build("/", &[]).unwrap(), "/");
        assert_eq!(build("/users/:id", &[&42]).unwrap(), "/users/42");
        assert_eq!(
            build("/flights/:from-:to", &[&"LAX", &"SFO"]).unwrap(),
            "/flights/LAX-SFO"
        );
        assert_eq!(
            build("/q/:term", &[&"a b/c?é"]).unwrap(),
            "/q/a%20b%2Fc%3F%C3%A9"
        );
        assert_eq!(
            build("/files/*path", &[&"a b/c.txt"]).unwrap(),
            "/files/a%20b/c.txt"
        );
        assert_eq!(build("/files/*path", &[]).unwrap(), "/files");
        assert_eq!(build("/archive/:y?/:m?", &[]).unwrap(), "/archive");
        assert_eq!(
            build("/archive/:y?/:m?", &[&2024]).unwrap(),
            "/archive/2024"
        );
        assert_eq!(build("/users/:id(\\d+)", &[&7]).unwrap(), "/users/7");
        assert!(build("/users/:id(\\d+)", &[&"ada"]).is_err());
        assert!(build("/users/:id", &[]).is_err());
        assert!(build("/users/:id", &[&1, &2]).is_err());
    }

    #[test]
    fn case_insensitive() {
        let mut t = tree(&["/Users/:id", "/files/:name.TXT"]);
        assert_eq!(lookup(&t, "/users/1"), None);
        t.set_case_insensitive(true);
        assert_eq!(lookup(&t, "/users/1"), Some(("/Users/:id", vec!["1"])));
        assert_eq!(lookup(&t, "/USERS/1"), Some(("/Users/:id", vec!["1"])));
        assert_eq!(
            lookup(&t, "/FILES/notes.txt"),
            Some(("/files/:name.TXT", vec!["notes"]))
        );
    }
}
