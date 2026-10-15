//! Request validation with clear, per-field errors (Laravel's
//! `$request->validate()`, Fastify's schemas, FastAPI's models).
//!
//! Implement [`Validate`] for the types you receive, then read the body with
//! [`Request::valid_json`](crate::Request::valid_json) (or
//! [`valid_form`](crate::Request::valid_form),
//! [`valid_query`](crate::Request::valid_query)): the value is deserialized,
//! then checked. When a rule fails, the client gets a `422 Unprocessable
//! Entity` listing the messages of every invalid field:
//!
//! ```json
//! {"error":"Validation failed","details":{"age":["must be at least 13"],"email":["must be a valid email address"]}}
//! ```
//!
//! ```
//! use vitesse::prelude::*;
//! use vitesse::validate::{Validate, Validator};
//!
//! #[derive(serde::Deserialize)]
//! struct SignUp {
//!     email: String,
//!     name: String,
//!     age: u8,
//!     website: Option<String>,
//! }
//!
//! impl Validate for SignUp {
//!     fn validate(&self, v: &mut Validator) {
//!         v.field("email", &self.email).email();
//!         v.field("name", &self.name).not_empty().max_length(50);
//!         v.field("age", &self.age).min(13);
//!         v.optional("website", &self.website).url();
//!     }
//! }
//!
//! let mut app = App::new();
//! app.post("/users", |req: Request| async move {
//!     let user: SignUp = req.valid_json().await?;
//!     Ok::<_, Error>((201, Json(json!({ "email": user.email }))))
//! });
//! ```
//!
//! Each field stops at its first failing rule, so a client gets one message
//! per field and per problem. Rules on an absent optional value
//! ([`Validator::optional`]) are skipped. [`Field::message`] replaces the
//! message of the rule just before it (to translate them, for instance), and
//! [`Field::check`] adds a rule of your own.
//!
//! Validation only runs where you ask for it: routes that don't call it
//! don't pay for it.

use std::collections::BTreeMap;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::{self, Display};

use crate::error::Error;

/// A type whose values can be checked (see the [module](self)).
pub trait Validate {
    /// Checks `self`, reporting each problem to `v`.
    fn validate(&self, v: &mut Validator);
}

impl<T: Validate + ?Sized> Validate for &T {
    fn validate(&self, v: &mut Validator) {
        (**self).validate(v);
    }
}

impl<T: Validate + ?Sized> Validate for Box<T> {
    fn validate(&self, v: &mut Validator) {
        (**self).validate(v);
    }
}

/// An absent value is valid; a present one is checked.
impl<T: Validate> Validate for Option<T> {
    fn validate(&self, v: &mut Validator) {
        if let Some(value) = self {
            value.validate(v);
        }
    }
}

/// Checks a value, as [`Request::valid_json`](crate::Request::valid_json)
/// does after deserializing it: `422` with the messages of every invalid
/// field, or `Ok(())`.
///
/// ```
/// use vitesse::validate::{self, Validate, Validator};
///
/// struct Tag(String);
/// impl Validate for Tag {
///     fn validate(&self, v: &mut Validator) {
///         v.field("tag", &self.0).min_length(2);
///     }
/// }
///
/// let err = validate::validate(&Tag("x".into())).unwrap_err();
/// assert_eq!(err.status(), 422);
/// assert_eq!(err.details().unwrap()["tag"][0], "must be at least 2 characters long");
/// ```
pub fn validate<T: Validate + ?Sized>(value: &T) -> Result<(), Error> {
    let mut v = Validator::new();
    value.validate(&mut v);
    v.into_result()
}

/// Collects the problems found while validating a value: field name →
/// messages.
#[derive(Debug, Default, Clone)]
pub struct Validator {
    /// `address.` or `items[2].` while validating a nested value.
    prefix: String,
    errors: BTreeMap<String, Vec<String>>,
}

impl Validator {
    /// An empty validator.
    pub fn new() -> Self {
        Validator::default()
    }

    /// Starts the rules of a field.
    pub fn field<'v, T: ?Sized>(&'v mut self, name: &str, value: &'v T) -> Field<'v, T> {
        let name = self.name(name);
        Field {
            validator: self,
            name,
            value: Some(value),
            failed: false,
            done: false,
        }
    }

    /// Starts the rules of an optional field: when it is `None`, its rules
    /// are skipped.
    pub fn optional<'v, T>(&'v mut self, name: &str, value: &'v Option<T>) -> Field<'v, T> {
        let name = self.name(name);
        Field {
            validator: self,
            name,
            value: value.as_ref(),
            failed: false,
            done: false,
        }
    }

    /// Starts the rules of a field that must be present: `None` fails with
    /// "is required".
    pub fn required<'v, T>(&'v mut self, name: &str, value: &'v Option<T>) -> Field<'v, T> {
        let mut field = self.optional(name, value);
        if value.is_none() {
            field.fail("is required".to_owned());
            field.done = true;
        }
        field
    }

    /// Validates a nested value, whose fields are reported as
    /// `name.field`.
    pub fn nested<T: Validate + ?Sized>(&mut self, name: &str, value: &T) -> &mut Self {
        let saved = self.prefix.len();
        self.prefix.push_str(name);
        self.prefix.push('.');
        value.validate(self);
        self.prefix.truncate(saved);
        self
    }

    /// Validates every item of a list, whose fields are reported as
    /// `name[0].field`.
    pub fn each<T: Validate>(&mut self, name: &str, items: &[T]) -> &mut Self {
        let saved = self.prefix.len();
        for (i, item) in items.iter().enumerate() {
            use std::fmt::Write;
            let _ = write!(self.prefix, "{name}[{i}].");
            item.validate(self);
            self.prefix.truncate(saved);
        }
        self
    }

    /// Reports a problem with a field yourself (a rule across several
    /// fields, a uniqueness check made beforehand…).
    pub fn error(&mut self, name: &str, message: impl Into<String>) -> &mut Self {
        let name = self.name(name);
        self.errors.entry(name).or_default().push(message.into());
        self
    }

    /// No problem was found.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// The problems found: field name → messages, sorted by name.
    pub fn errors(&self) -> &BTreeMap<String, Vec<String>> {
        &self.errors
    }

    /// `Ok(())`, or a `422 Unprocessable Entity` whose
    /// [`details`](Error::details) are the messages of each field.
    pub fn into_result(self) -> Result<(), Error> {
        if self.errors.is_empty() {
            return Ok(());
        }
        let details: serde_json::Map<String, serde_json::Value> = self
            .errors
            .into_iter()
            .map(|(name, messages)| (name, messages.into()))
            .collect();
        Err(Error::unprocessable("Validation failed").with_details(details))
    }

    fn name(&self, name: &str) -> String {
        let mut full = String::with_capacity(self.prefix.len() + name.len());
        full.push_str(&self.prefix);
        full.push_str(name);
        full
    }
}

/// The rules of one field, chained: `v.field("name", &self.name).not_empty().max_length(50)`.
///
/// A field stops at its first failing rule.
pub struct Field<'v, T: ?Sized> {
    validator: &'v mut Validator,
    name: String,
    /// `None` for an absent optional value: the rules are skipped.
    value: Option<&'v T>,
    /// The last rule failed (for [`Field::message`]).
    failed: bool,
    /// A rule already failed: the next ones are skipped.
    done: bool,
}

impl<T: ?Sized> fmt::Debug for Field<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Field")
            .field("name", &self.name)
            .field("present", &self.value.is_some())
            .field("failed", &self.done)
            .finish()
    }
}

impl<'v, T: ?Sized> Field<'v, T> {
    fn fail(&mut self, message: String) {
        self.validator
            .errors
            .entry(self.name.clone())
            .or_default()
            .push(message);
        self.failed = true;
        self.done = true;
    }

    /// Runs a rule: `test` gets the value, and returns the message of the
    /// problem, if any.
    fn rule(mut self, test: impl FnOnce(&T) -> Option<String>) -> Self {
        self.failed = false;
        if self.done {
            return self;
        }
        if let Some(message) = self.value.and_then(test) {
            self.fail(message);
        }
        self
    }

    /// A rule of your own: `test` returns `true` when the value is valid.
    ///
    /// ```
    /// # use vitesse::validate::{Validate, Validator};
    /// # struct Account { username: String }
    /// impl Validate for Account {
    ///     fn validate(&self, v: &mut Validator) {
    ///         v.field("username", &self.username)
    ///             .check(|u| !u.contains(' '), "must not contain spaces");
    ///     }
    /// }
    /// ```
    pub fn check(self, test: impl FnOnce(&T) -> bool, message: impl Into<String>) -> Self {
        self.rule(|value| (!test(value)).then(|| message.into()))
    }

    /// Replaces the message of the rule just before, if it failed (to
    /// translate it, or to be more specific).
    ///
    /// ```
    /// # use vitesse::validate::{Validate, Validator};
    /// # struct Signup { email: String }
    /// impl Validate for Signup {
    ///     fn validate(&self, v: &mut Validator) {
    ///         v.field("email", &self.email)
    ///             .email()
    ///             .message("doit être une adresse e-mail valide");
    ///     }
    /// }
    /// ```
    pub fn message(self, message: impl Into<String>) -> Self {
        if self.failed {
            if let Some(last) = self
                .validator
                .errors
                .get_mut(&self.name)
                .and_then(|m| m.last_mut())
            {
                *last = message.into();
            }
        }
        self
    }

    /// The field is valid so far.
    pub fn is_valid(&self) -> bool {
        !self.done
    }
}

impl<T: Length + ?Sized> Field<'_, T> {
    /// Not empty (a string, a list, a map…).
    pub fn not_empty(self) -> Self {
        self.rule(|value| (value.length() == 0).then(|| "must not be empty".to_owned()))
    }

    /// At least `min` characters (for a string, counted in Unicode
    /// characters) or items.
    pub fn min_length(self, min: usize) -> Self {
        self.rule(|value| {
            (value.length() < min).then(|| match T::UNIT {
                Unit::Chars => format!("must be at least {min} characters long"),
                Unit::Items => format!("must have at least {min} items"),
            })
        })
    }

    /// At most `max` characters or items.
    pub fn max_length(self, max: usize) -> Self {
        self.rule(|value| {
            (value.length() > max).then(|| match T::UNIT {
                Unit::Chars => format!("must be at most {max} characters long"),
                Unit::Items => format!("must have at most {max} items"),
            })
        })
    }
}

impl<T: PartialOrd + Display> Field<'_, T> {
    /// At least `min`.
    pub fn min(self, min: T) -> Self {
        self.rule(|value| (*value < min).then(|| format!("must be at least {min}")))
    }

    /// At most `max`.
    pub fn max(self, max: T) -> Self {
        self.rule(|value| (*value > max).then(|| format!("must be at most {max}")))
    }

    /// Between `min` and `max`, both included.
    pub fn between(self, min: T, max: T) -> Self {
        self.rule(|value| {
            (*value < min || *value > max).then(|| format!("must be between {min} and {max}"))
        })
    }
}

impl<T: ?Sized> Field<'_, T> {
    /// One of the given values.
    ///
    /// ```
    /// # use vitesse::validate::{Validate, Validator};
    /// # struct Order { size: String }
    /// impl Validate for Order {
    ///     fn validate(&self, v: &mut Validator) {
    ///         v.field("size", &self.size).one_of(&["S", "M", "L"]);
    ///     }
    /// }
    /// ```
    pub fn one_of<U: Display>(self, choices: &[U]) -> Self
    where
        T: PartialEq<U>,
    {
        self.rule(|value| {
            if choices.iter().any(|choice| *value == *choice) {
                return None;
            }
            let list: Vec<String> = choices.iter().map(ToString::to_string).collect();
            Some(format!("must be one of: {}", list.join(", ")))
        })
    }
}

impl<T: AsRef<str> + ?Sized> Field<'_, T> {
    /// A plausible email address (`name@domain.tld`, without spaces).
    pub fn email(self) -> Self {
        self.rule(|value| {
            (!is_email(value.as_ref())).then(|| "must be a valid email address".to_owned())
        })
    }

    /// An `http://` or `https://` URL with a host.
    pub fn url(self) -> Self {
        self.rule(|value| (!is_url(value.as_ref())).then(|| "must be a valid URL".to_owned()))
    }

    /// Only ASCII letters and digits.
    pub fn alphanumeric(self) -> Self {
        self.rule(|value| {
            let s = value.as_ref();
            (!s.bytes().all(|b| b.is_ascii_alphanumeric()))
                .then(|| "must only contain letters and digits".to_owned())
        })
    }
}

fn is_email(s: &str) -> bool {
    let Some((local, domain)) = s.rsplit_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && s.len() <= 254
        && !s.chars().any(|c| c.is_whitespace() || c.is_control())
        && !local.contains('@')
        && domain.contains('.')
        && domain
            .split('.')
            .all(|label| !label.is_empty() && !label.starts_with('-') && !label.ends_with('-'))
}

fn is_url(s: &str) -> bool {
    let Some(rest) = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
    else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    !host.is_empty()
        && !host.starts_with(':')
        && !s.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// What [`Length`] counts, for the messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// Characters of a string.
    Chars,
    /// Items of a collection.
    Items,
}

/// A value with a length, for [`Field::not_empty`], [`Field::min_length`]
/// and [`Field::max_length`].
pub trait Length {
    /// What the length counts.
    const UNIT: Unit;
    /// The length: Unicode characters for strings, items for collections.
    fn length(&self) -> usize;
}

impl Length for str {
    const UNIT: Unit = Unit::Chars;
    fn length(&self) -> usize {
        self.chars().count()
    }
}

impl Length for String {
    const UNIT: Unit = Unit::Chars;
    fn length(&self) -> usize {
        self.chars().count()
    }
}

impl<T> Length for [T] {
    const UNIT: Unit = Unit::Items;
    fn length(&self) -> usize {
        self.len()
    }
}

impl<T> Length for Vec<T> {
    const UNIT: Unit = Unit::Items;
    fn length(&self) -> usize {
        self.len()
    }
}

impl<K, V, S> Length for HashMap<K, V, S> {
    const UNIT: Unit = Unit::Items;
    fn length(&self) -> usize {
        self.len()
    }
}

impl<K, V> Length for BTreeMap<K, V> {
    const UNIT: Unit = Unit::Items;
    fn length(&self) -> usize {
        self.len()
    }
}

impl<T, S> Length for HashSet<T, S> {
    const UNIT: Unit = Unit::Items;
    fn length(&self) -> usize {
        self.len()
    }
}

impl<T> Length for BTreeSet<T> {
    const UNIT: Unit = Unit::Items;
    fn length(&self) -> usize {
        self.len()
    }
}

impl<T: Length + ?Sized> Length for &T {
    const UNIT: Unit = T::UNIT;
    fn length(&self) -> usize {
        (**self).length()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Address {
        city: String,
    }

    impl Validate for Address {
        fn validate(&self, v: &mut Validator) {
            v.field("city", &self.city).not_empty();
        }
    }

    struct Form {
        name: String,
        age: u32,
        email: Option<String>,
        nickname: Option<String>,
        tags: Vec<String>,
        address: Address,
        addresses: Vec<Address>,
    }

    impl Validate for Form {
        fn validate(&self, v: &mut Validator) {
            v.field("name", &self.name)
                .not_empty()
                .message("le nom est obligatoire")
                .max_length(3);
            v.field("age", &self.age).between(18, 99);
            v.required("email", &self.email).email();
            v.optional("nickname", &self.nickname).min_length(2);
            v.field("tags", &self.tags).max_length(1);
            v.nested("address", &self.address);
            v.each("addresses", &self.addresses);
        }
    }

    #[test]
    fn collects_errors_per_field() {
        let form = Form {
            name: String::new(),
            age: 12,
            email: None,
            nickname: None,
            tags: vec!["a".into(), "b".into()],
            address: Address {
                city: String::new(),
            },
            addresses: vec![
                Address {
                    city: "Paris".into(),
                },
                Address {
                    city: String::new(),
                },
            ],
        };
        let err = validate(&form).unwrap_err();
        assert_eq!(err.status(), 422);
        let details = err.details().unwrap();
        assert_eq!(
            details["name"],
            serde_json::json!(["le nom est obligatoire"])
        );
        assert_eq!(details["age"][0], "must be between 18 and 99");
        assert_eq!(details["email"][0], "is required");
        assert!(details.get("nickname").is_none());
        assert_eq!(details["tags"][0], "must have at most 1 items");
        assert_eq!(details["address.city"][0], "must not be empty");
        assert_eq!(details["addresses[1].city"][0], "must not be empty");
        assert!(details.get("addresses[0].city").is_none());
    }

    #[test]
    fn valid_values_pass() {
        let form = Form {
            name: "Ada".into(),
            age: 36,
            email: Some("ada@example.com".into()),
            nickname: Some("al".into()),
            tags: vec![],
            address: Address {
                city: "London".into(),
            },
            addresses: vec![],
        };
        assert!(validate(&form).is_ok());
    }

    #[test]
    fn first_failure_stops_the_field() {
        let mut v = Validator::new();
        v.field("name", "").not_empty().min_length(3).max_length(0);
        assert_eq!(v.errors()["name"], vec!["must not be empty"]);
        // `message` only rewrites a rule that failed.
        let mut v = Validator::new();
        v.field("name", "abc").not_empty().message("nope");
        assert!(v.is_valid());
    }

    #[test]
    fn emails_and_urls() {
        for ok in ["a@b.co", "first.last+tag@sub.example.org"] {
            assert!(is_email(ok), "{ok}");
        }
        for bad in [
            "", "a", "a@b", "@b.co", "a b@c.de", "a@.co", "a@b..co", "a@-b.co",
        ] {
            assert!(!is_email(bad), "{bad}");
        }
        for ok in ["https://example.com", "http://localhost:3000/x?y#z"] {
            assert!(is_url(ok), "{ok}");
        }
        for bad in [
            "example.com",
            "ftp://x.y",
            "https://",
            "https:// x.com",
            "http://:80",
        ] {
            assert!(!is_url(bad), "{bad}");
        }
    }

    #[test]
    fn one_of_and_custom_rules() {
        let mut v = Validator::new();
        v.field("size", &String::from("XL"))
            .one_of(&["S", "M", "L"]);
        v.field("n", &3).check(|n| n % 2 == 0, "must be even");
        v.error("password_confirmation", "does not match");
        let errors = v.errors();
        assert_eq!(errors["size"], vec!["must be one of: S, M, L"]);
        assert_eq!(errors["n"], vec!["must be even"]);
        assert_eq!(errors["password_confirmation"], vec!["does not match"]);
        let mut v = Validator::new();
        v.field("word", "été").min_length(4);
        v.field("list", &[1, 2][..]).min_length(3);
        assert_eq!(
            v.errors()["word"],
            vec!["must be at least 4 characters long"]
        );
        assert_eq!(v.errors()["list"], vec!["must have at least 3 items"]);
    }
}
