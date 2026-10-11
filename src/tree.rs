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

/// The path of a named route, prepared at startup: its literal parts and
/// its parameters, with their constraints compiled (see
/// [`Request::path_for`](crate::Request::path_for)).
pub(crate) struct UrlTemplate {
    segments: Vec<UrlSegment>,
}

enum UrlSegment {
    Static(Box<str>),
    Param {
        name: Box<str>,
        constraint: Option<Regex>,
        optional: bool,
    },
    Composite(Vec<UrlPart>),
    Wildcard,
}

enum UrlPart {
    Lit(Box<str>),
    Param {
        name: Box<str>,
        constraint: Option<Regex>,
    },
}

fn compiled(constraint: Option<&str>) -> Option<Regex> {
    constraint.map(|c| compile(c).expect("constraints are checked by parse_pattern"))
}

impl UrlTemplate {
    pub(crate) fn new(pattern: &str) -> Result<UrlTemplate, String> {
        let segments = parse_pattern(pattern)?
            .into_iter()
            .map(|segment| match segment {
                Segment::Static(s) => UrlSegment::Static(s.into()),
                Segment::Param {
                    name,
                    constraint,
                    optional,
                } => UrlSegment::Param {
                    name: name.into(),
                    constraint: compiled(constraint),
                    optional,
                },
                Segment::Pattern(parts) => UrlSegment::Composite(
                    parts
                        .into_iter()
                        .map(|part| match part {
                            Part::Lit(lit) => UrlPart::Lit(lit.into()),
                            Part::Param { name, constraint } => UrlPart::Param {
                                name: name.into(),
                                constraint: compiled(constraint),
                            },
                        })
                        .collect(),
                ),
                Segment::Wildcard(_) => UrlSegment::Wildcard,
            })
            .collect();
        Ok(UrlTemplate { segments })
    }

    /// The path, with the values of the parameters in order.
    pub(crate) fn build(&self, values: &[&dyn fmt::Display]) -> Result<String, String> {
        let mut values = values.iter().map(|v| v.to_string());
        let mut out = String::new();
        let check = |name: &str, constraint: &Option<Regex>, value: &str| {
            if value.is_empty() {
                return Err(format!("':{name}' cannot be empty"));
            }
            match constraint {
                Some(re) if !re.is_match(value) => Err(format!(
                    "'{value}' does not satisfy the constraint of ':{name}'"
                )),
                _ => Ok(()),
            }
        };
        for segment in &self.segments {
            match segment {
                UrlSegment::Static(s) => {
                    out.push('/');
                    out.push_str(s);
                }
                UrlSegment::Param {
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
                    check(name, constraint, &value)?;
                    out.push('/');
                    encode_into(&mut out, &value, false);
                }
                UrlSegment::Composite(parts) => {
                    out.push('/');
                    for part in parts {
                        match part {
                            UrlPart::Lit(lit) => out.push_str(lit),
                            UrlPart::Param { name, constraint } => {
                                let value = values
                                    .next()
                                    .ok_or_else(|| format!("missing a value for ':{name}'"))?;
                                check(name, constraint, &value)?;
                                encode_into(&mut out, &value, false);
                            }
                        }
                    }
                }
                UrlSegment::Wildcard => {
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
}

/// Whether the mount prefix `parent` covers the mount prefix `child`: every
/// path under `child` is also under `parent`. A segment is covered by the
/// same text, by a parameter without constraint, or, for a fixed segment,
/// by a constrained or composite segment that matches it.
pub(crate) fn covers(parent: &str, child: &str) -> bool {
    let (Ok(parents), Ok(children)) = (parse_pattern(parent), parse_pattern(child)) else {
        return false;
    };
    let mut children = children.iter();
    for expected in &parents {
        let Some(segment) = children.next() else {
            // `child` est plus court : il n'est couvert que si le reste est optionnel.
            return matches!(expected, Segment::Wildcard(_)) || is_optional(expected);
        };
        let covered = match (expected, segment) {
            (Segment::Wildcard(_), _) => return true,
            (
                Segment::Param {
                    constraint: None, ..
                },
                _,
            ) => true,
            (
                Segment::Param {
                    name, constraint, ..
                },
                Segment::Static(text),
            ) => Matcher::new(&[Part::Param {
                name,
                constraint: *constraint,
            }])
            .matches(text, 0, &mut Vec::new(), false),
            (Segment::Pattern(parts), Segment::Static(text)) => {
                Matcher::new(parts).matches(text, 0, &mut Vec::new(), false)
            }
            (a, b) => same_segment(a, b),
        };
        if !covered {
            return false;
        }
    }
    true
}

/// The same segment, whatever the names of its parameters.
fn same_segment(a: &Segment<'_>, b: &Segment<'_>) -> bool {
    match (a, b) {
        (Segment::Static(a), Segment::Static(b)) => a == b,
        (
            Segment::Param {
                constraint: a,
                optional: oa,
                ..
            },
            Segment::Param {
                constraint: b,
                optional: ob,
                ..
            },
        ) => a == b && oa == ob,
        (Segment::Pattern(a), Segment::Pattern(b)) => key_of(a) == key_of(b),
        (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
        _ => false,
    }
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

/// Matches a constrained parameter or a composite segment, with a single
/// anchored regular expression: regex-lite runs in linear time, whatever
/// the path (no catastrophic backtracking).
struct Matcher {
    /// The structure of the segment, to merge identical ones (the names of
    /// the parameters don't matter).
    key: String,
    sensitive: Regex,
    /// The same, with the literal parts compared regardless of ASCII case.
    insensitive: Regex,
    /// The capture groups of the parameters, in order.
    groups: Box<[usize]>,
}

/// The structure of a segment: its literals and constraints.
fn key_of(parts: &[Part<'_>]) -> String {
    let mut key = String::new();
    for part in parts {
        match part {
            Part::Lit(lit) => {
                key.push('L');
                key.push_str(lit);
            }
            Part::Param { constraint, .. } => {
                key.push('P');
                key.push_str(constraint.unwrap_or(""));
            }
        }
        key.push('\0');
    }
    key
}

/// `^text(?P<vitesse_p0>(?:constraint))text(?P<vitesse_p1>.+?)$`: a parameter without
/// constraint takes as few characters as possible (at least one), like
/// Express: `:name.:ext` splits `archive.tar.gz` into `archive` and `tar.gz`.
fn segment_regex(parts: &[Part<'_>], case_insensitive: bool) -> Regex {
    let mut re = String::from("^");
    let mut n = 0;
    for part in parts {
        match part {
            Part::Lit(lit) if case_insensitive => {
                re.push_str("(?i:");
                re.push_str(&regex_lite::escape(lit));
                re.push(')');
            }
            Part::Lit(lit) => re.push_str(&regex_lite::escape(lit)),
            Part::Param { constraint, .. } => {
                match constraint {
                    Some(c) => re.push_str(&format!("(?P<vitesse_p{n}>(?:{c}))")),
                    None => re.push_str(&format!("(?P<vitesse_p{n}>.+?)")),
                }
                n += 1;
            }
        }
    }
    re.push('$');
    Regex::new(&re).expect("constraints are checked by parse_pattern")
}

impl Matcher {
    fn new(parts: &[Part<'_>]) -> Matcher {
        let sensitive = segment_regex(parts, false);
        let groups = sensitive
            .capture_names()
            .enumerate()
            .filter(|(_, name)| name.is_some_and(|n| n.starts_with("vitesse_p")))
            .map(|(i, _)| i)
            .collect();
        Matcher {
            key: key_of(parts),
            insensitive: segment_regex(parts, true),
            sensitive,
            groups,
        }
    }

    fn same(&self, parts: &[Part<'_>]) -> bool {
        self.key == key_of(parts)
    }

    fn matches(&self, segment: &str, offset: usize, captures: &mut Captures, ci: bool) -> bool {
        let re = if ci {
            &self.insensitive
        } else {
            &self.sensitive
        };
        let Some(found) = re.captures(segment) else {
            return false;
        };
        let mark = captures.len();
        for &group in &*self.groups {
            match found.get(group) {
                Some(m) if !m.is_empty() => captures.push((offset + m.start(), offset + m.end())),
                _ => {
                    captures.truncate(mark);
                    return false;
                }
            }
        }
        true
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

        if ci {
            // Sans la casse, plusieurs enfants peuvent convenir (`/Users`, `/users`).
            let matching = self
                .statics
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case(seg));
            for (_, child) in matching {
                if let Some(v) = child.find(next, next_offset, captures, ci) {
                    return Some(v);
                }
            }
        } else if let Some((_, child)) = self.statics.iter().find(|(k, _)| **k == *seg) {
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
        let build = |p: &str, v: &[&dyn fmt::Display]| UrlTemplate::new(p).unwrap().build(v);
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
        assert!(build("/users/:id", &[&""]).is_err());
    }

    #[test]
    fn matching_takes_linear_time() {
        let t = tree(&[
            "/posts/:id(\\d+)-:slug",
            "/a/:b-:c-:d-:e",
            "/files/:name.:ext",
        ]);
        let start = std::time::Instant::now();
        assert_eq!(lookup(&t, &format!("/posts/{}", "1".repeat(20_000))), None);
        assert_eq!(lookup(&t, &format!("/files/{}", "a".repeat(20_000))), None);
        let dashes = format!("/a/{}", "-".repeat(20_000));
        assert_eq!(lookup(&t, &dashes).unwrap().1.len(), 4);
        assert!(
            start.elapsed() < std::time::Duration::from_millis(500),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn case_insensitive_branches() {
        let mut t = tree(&["/Users/new", "/users/:id"]);
        t.set_case_insensitive(true);
        assert_eq!(lookup(&t, "/users/new"), Some(("/Users/new", vec![])));
        assert_eq!(lookup(&t, "/users/42"), Some(("/users/:id", vec!["42"])));
    }

    #[test]
    fn prefixes_covering_prefixes() {
        assert!(covers("/", "/api"));
        assert!(covers("/api", "/api/admin"));
        assert!(covers("/api/:v", "/api/admin"));
        assert!(covers("/api/:v", "/api/:version/x"));
        assert!(!covers("/api/v:version", "/api/admin"));
        assert!(covers("/api/v:version", "/api/v2/admin"));
        assert!(!covers("/docs/:lang(en|fr)", "/docs/de"));
        assert!(covers("/docs/:lang(en|fr)", "/docs/fr/x"));
        assert!(covers("/docs/:lang(en|fr)", "/docs/:l(en|fr)/x"));
        assert!(!covers("/docs/:lang(en|fr)", "/docs/:l/x"));
        assert!(!covers("/api/admin", "/api"));
        assert!(covers("/docs/:page?", "/docs"));
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
