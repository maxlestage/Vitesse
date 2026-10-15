//! Typed routes: a pattern and the types of its parameters, declared once,
//! to register the route, read its parameters and build its URLs
//! (Laravel's `route('user', $id)`, Phoenix's verified routes, axum's
//! `TypedPath`).

use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::OnceLock;

use crate::error::Error;
use crate::request::Request;
use crate::tree::{UrlTemplate, param_names, parse_pattern};

/// The parameters of a [`TypedRoute`]: a tuple of types that print
/// themselves (`Display`) and parse themselves (`FromStr`), in the order of
/// the pattern: `()`, `(u32,)`, `(String, u32)`… up to six.
pub trait RouteParams: Sized {
    /// The number of parameters.
    const COUNT: usize;

    /// The values, in order, to build a URL.
    fn values(&self) -> Vec<&dyn fmt::Display>;

    /// Parses the values, in order; the error names the parameter.
    fn parse(values: &[&str]) -> Result<Self, usize>;
}

macro_rules! route_params {
    ($count:expr; $($name:ident $index:tt),*) => {
        impl<$($name),*> RouteParams for ($($name,)*)
        where
            $($name: fmt::Display + FromStr,)*
        {
            const COUNT: usize = $count;

            fn values(&self) -> Vec<&dyn fmt::Display> {
                vec![$(&self.$index as &dyn fmt::Display),*]
            }

            #[allow(unused_variables)]
            fn parse(values: &[&str]) -> Result<Self, usize> {
                Ok(($(
                    values
                        .get($index)
                        .and_then(|v| v.parse::<$name>().ok())
                        .ok_or::<usize>($index)?,
                )*))
            }
        }
    };
}

impl RouteParams for () {
    const COUNT: usize = 0;

    fn values(&self) -> Vec<&dyn fmt::Display> {
        Vec::new()
    }

    fn parse(_: &[&str]) -> Result<Self, usize> {
        Ok(())
    }
}

route_params!(1; A 0);
route_params!(2; A 0, B 1);
route_params!(3; A 0, B 1, C 2);
route_params!(4; A 0, B 1, C 2, D 3);
route_params!(5; A 0, B 1, C 2, D 3, E 4);
route_params!(6; A 0, B 1, C 2, D 3, E 4, F 5);

/// The compiled pattern and the names of its parameters, or why the
/// pattern does not fit the parameters.
type Compiled = Result<(UrlTemplate, Box<[Box<str>]>), String>;

/// A route pattern with the types of its parameters, declared once as a
/// `static`, then used to register the route, read its parameters with
/// their types, and build its URLs: no name to repeat, no parameter to
/// forget, and a compile error if a link passes the wrong types.
///
/// ```
/// use vitesse::prelude::*;
/// use vitesse::TypedRoute;
///
/// static POST: TypedRoute<(String, u32)> = TypedRoute::new("/blog/:slug/comments/:id(\\d+)");
///
/// let mut app = App::new();
/// app.get(POST.pattern(), |req: Request| async move {
///     let (slug, id) = POST.params(&req)?; // (String, u32)
///     Ok::<_, Error>(format!("comment {id} of {slug}"))
/// });
/// app.get("/", |_| async {
///     Html(format!("<a href=\"{}\">first comment</a>", POST.path(("hello".into(), 1))))
/// });
///
/// assert_eq!(POST.path(("hello world".into(), 7)), "/blog/hello%20world/comments/7");
/// ```
pub struct TypedRoute<P> {
    pattern: &'static str,
    template: OnceLock<Compiled>,
    params: PhantomData<fn() -> P>,
}

impl<P> fmt::Debug for TypedRoute<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("TypedRoute").field(&self.pattern).finish()
    }
}

impl<P: RouteParams> TypedRoute<P> {
    /// The route of `pattern`, whose parameters, in order, have the types
    /// of `P`.
    pub const fn new(pattern: &'static str) -> Self {
        TypedRoute {
            pattern,
            template: OnceLock::new(),
            params: PhantomData,
        }
    }

    /// The pattern, to register the route (`app.get(ROUTE.pattern(), …)`).
    pub fn pattern(&self) -> &'static str {
        self.pattern
    }

    #[allow(clippy::type_complexity)]
    fn template(&self) -> Result<&(UrlTemplate, Box<[Box<str>]>), Error> {
        self.template
            .get_or_init(|| {
                let names = param_names(&parse_pattern(self.pattern)?);
                if names.len() != P::COUNT {
                    return Err(format!(
                        "the pattern has {} parameters, the type {}",
                        names.len(),
                        P::COUNT
                    ));
                }
                Ok((UrlTemplate::new(self.pattern)?, names))
            })
            .as_ref()
            .map_err(|e| {
                Error::internal("Internal Server Error")
                    .with_source(RouteError(format!("route '{}': {e}", self.pattern)))
            })
    }

    /// The path of the route with these parameters, encoded.
    ///
    /// # Panics
    /// If a value doesn't satisfy the constraint of its parameter, or if
    /// the pattern doesn't have as many parameters as `P` (see
    /// [`try_path`](Self::try_path)).
    #[track_caller]
    pub fn path(&self, params: P) -> String {
        match self.try_path(params) {
            Ok(path) => path,
            Err(error) => panic!(
                "{}",
                error.source().map_or(error.to_string(), |s| s.to_string())
            ),
        }
    }

    /// The path of the route with these parameters, or a `500` error
    /// (whose cause is logged) if it cannot be built.
    pub fn try_path(&self, params: P) -> Result<String, Error> {
        let (template, _) = self.template()?;
        template.build(&params.values()).map_err(|e| {
            Error::internal("Internal Server Error")
                .with_source(RouteError(format!("route '{}': {e}", self.pattern)))
        })
    }

    /// The absolute URL of the route, with the protocol and host of `req`
    /// (`https://example.com/users/42`).
    pub fn url(&self, req: &Request, params: P) -> Result<String, Error> {
        let path = self.try_path(params)?;
        let host = req
            .host()
            .ok_or_else(|| Error::internal("cannot build an absolute URL without a host"))?;
        Ok(format!("{}://{host}{path}", req.protocol()))
    }

    /// The parameters of the request, with their types: a value that does
    /// not parse answers `400 Bad Request`.
    pub fn params(&self, req: &Request) -> Result<P, Error> {
        let (_, names) = self.template()?;
        let values: Vec<&str> = names
            .iter()
            .map(|name| req.param(name).unwrap_or(""))
            .collect();
        P::parse(&values).map_err(|i| {
            Error::bad_request(format!(
                "invalid value for the parameter '{}'",
                names.get(i).map_or("?", |n| n)
            ))
        })
    }
}

/// Why a typed route could not build a URL (logged, not sent).
#[derive(Debug)]
struct RouteError(String);

impl fmt::Display for RouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RouteError {}

#[cfg(test)]
mod tests {
    use super::*;

    static USER: TypedRoute<(u32,)> = TypedRoute::new("/users/:id(\\d+)");
    static HOME: TypedRoute<()> = TypedRoute::new("/");
    static WRONG: TypedRoute<(u32, u32)> = TypedRoute::new("/users/:id");

    #[test]
    fn paths() {
        assert_eq!(USER.path((42,)), "/users/42");
        assert_eq!(HOME.path(()), "/");
        assert!(WRONG.try_path((1, 2)).is_err());
    }

    #[test]
    fn parsing() {
        assert_eq!(<(u32, String)>::parse(&["7", "x"]), Ok((7, "x".to_owned())));
        assert_eq!(<(u32, String)>::parse(&["x", "x"]), Err(0));
        assert_eq!(<(u32,)>::parse(&[]), Err(0));
    }
}
