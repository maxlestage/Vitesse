//! [tower](https://crates.io/crates/tower) interop: the middleware ecosystem
//! of axum and hyper in a Vitesse application, and a Vitesse application
//! as a tower `Service`. Needs the `tower` feature.
//!
//! ## Tower layers as middlewares
//!
//! [`layer`] runs any tower `Layer` (tower-http, tower-governor, tower's
//! own limits…) like a Vitesse middleware, globally, on a router or on a
//! route:
//!
//! ```
//! use vitesse::prelude::*;
//! use vitesse::http::{HeaderName, HeaderValue};
//! use tower_http::cors::CorsLayer;
//! use tower_http::set_header::SetResponseHeaderLayer;
//!
//! let mut app = App::new();
//! app.middleware(middleware::layer(SetResponseHeaderLayer::overriding(
//!     HeaderName::from_static("x-powered-by"),
//!     HeaderValue::from_static("Vitesse"),
//! )));
//!
//! let mut api = Router::new();
//! api.middleware(middleware::layer(CorsLayer::permissive()));
//! ```
//!
//! The layer sees an `http::Request<vitesse::Body>` and can change it
//! (method, URI, headers, extensions, body): the changes are carried over
//! to the Vitesse request. Its service is built once, at startup, and
//! cloned for each request, as in axum.
//!
//! ## A Vitesse application as a service
//!
//! [`App::into_service`](crate::App::into_service) turns the application
//! into a `Service<http::Request<B>>`: serve it with hyper, nest it in an
//! axum router (`Router::nest_service`), or test it with
//! `tower::ServiceExt::oneshot`.
//!
//! ```
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! use vitesse::prelude::*;
//! # use tower05 as tower;
//! use tower::ServiceExt;
//!
//! let mut app = App::new();
//! app.get("/", |_| async { "Hello from a service!" });
//!
//! let req = vitesse::http::Request::get("/").body(Body::empty()).unwrap();
//! let res = app.into_service().oneshot(req).await.unwrap();
//! assert_eq!(res.status(), 200);
//! # }
//! ```

use std::any::Any;
use std::convert::Infallible;
use std::fmt;
use std::future::poll_fn;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};

use bytes::Bytes;
use http::StatusCode;

pub use tower_layer::Layer;
pub use tower_service::Service;

use crate::body::{Body, BoxError};
use crate::error::Error;
use crate::handler::{BoxFuture, Middleware, Next};
use crate::request::{ReqBody, Request};
use crate::response::{IntoResponse, Response};
use crate::server::ResponseFuture;

/// Runs a tower [`Layer`] as a Vitesse middleware (see the [module](self)
/// documentation). Also available as `middleware::layer`.
pub fn layer<L, B>(layer: L) -> TowerMiddleware<L::Service>
where
    L: Layer<NextService>,
    L::Service:
        Service<http::Request<Body>, Response = http::Response<B>> + Clone + Send + Sync + 'static,
    <L::Service as Service<http::Request<Body>>>::Future: Send + 'static,
    <L::Service as Service<http::Request<Body>>>::Error: Into<BoxError>,
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
{
    TowerMiddleware {
        service: layer.layer(NextService { _private: () }),
    }
}

/// The middleware returned by [`layer`]: a tower service wrapped around the
/// rest of the chain.
#[derive(Clone)]
pub struct TowerMiddleware<S> {
    service: S,
}

impl<S> fmt::Debug for TowerMiddleware<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TowerMiddleware").finish_non_exhaustive()
    }
}

impl<S, B> Middleware for TowerMiddleware<S>
where
    S: Service<http::Request<Body>, Response = http::Response<B>> + Clone + Send + Sync + 'static,
    S::Future: Send + 'static,
    S::Error: Into<BoxError>,
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
{
    fn handle(&'static self, mut req: Request, next: Next) -> BoxFuture<Response> {
        let mut outgoing = http::Request::new(req.take_body());
        *outgoing.method_mut() = req.method().clone();
        *outgoing.uri_mut() = req.uri().clone();
        *outgoing.version_mut() = req.version();
        *outgoing.headers_mut() = req.headers().clone();
        *outgoing.extensions_mut() = std::mem::take(req.extensions_mut());
        outgoing
            .extensions_mut()
            .insert(Carrier(Arc::new(Mutex::new(Some((req, next))))));
        let mut service = self.service.clone();
        Box::pin(async move {
            if let Err(e) = poll_fn(|cx| service.poll_ready(cx)).await {
                return service_error(e);
            }
            match service.call(outgoing).await {
                Ok(res) => from_http(res),
                Err(e) => service_error(e),
            }
        })
    }
}

/// An error returned by a tower service: a `500` (its details are only
/// logged).
fn service_error(e: impl Into<BoxError>) -> Response {
    Error::internal("Internal Server Error")
        .with_source(e)
        .into_response()
}

/// The rest of the Vitesse chain, as the innermost tower service of a
/// [`layer`]. It only answers requests that come from a Vitesse
/// middleware.
#[derive(Clone, Copy)]
pub struct NextService {
    _private: (),
}

impl fmt::Debug for NextService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NextService")
    }
}

/// The Vitesse request and the rest of the chain, carried by the
/// `http::Request` through the tower layers.
#[derive(Clone)]
struct Carrier(Arc<Mutex<Option<(Request, Next)>>>);

/// The error behind a response, carried by the `http::Response` through the
/// tower layers, for `app.on_error`.
#[derive(Clone)]
struct CarriedError(Arc<Mutex<Option<Error>>>);

impl<B> Service<http::Request<B>> for NextService
where
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
{
    type Response = http::Response<Body>;
    type Error = Infallible;
    type Future = BoxFuture<Result<http::Response<Body>, Infallible>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        let (mut parts, body) = req.into_parts();
        let carried = parts
            .extensions
            .remove::<Carrier>()
            .and_then(|c| c.0.lock().unwrap_or_else(PoisonError::into_inner).take());
        let Some((mut req, next)) = carried else {
            let res = Error::internal("this service only runs inside a Vitesse middleware");
            return Box::pin(std::future::ready(Ok(to_http(res.into_response()))));
        };
        if parts.method != *req.method() {
            req.set_method(parts.method);
        }
        if parts.uri != *req.uri() {
            req.set_uri(parts.uri);
        }
        *req.headers_mut() = parts.headers;
        *req.extensions_mut() = parts.extensions;
        req.set_body(into_body(body));
        Box::pin(async move { Ok(to_http(next.run(req).await)) })
    }
}

/// Converts a Vitesse response, keeping its error for `app.on_error`.
fn to_http(mut res: Response) -> http::Response<Body> {
    let error = res.take_error();
    let mut res = res.into_http();
    if let Some(error) = error {
        res.extensions_mut()
            .insert(CarriedError(Arc::new(Mutex::new(Some(error)))));
    }
    res
}

/// Converts the response of a tower service.
fn from_http<B>(res: http::Response<B>) -> Response
where
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
{
    let mut res = Response::from_http(res.map(into_body));
    if let Some(carried) = res.extensions_mut().remove::<CarriedError>() {
        res.error = carried
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .map(Box::new);
    }
    res
}

/// A body of any type as a Vitesse [`Body`], without wrapping a body that
/// already is one.
fn into_body<B>(body: B) -> Body
where
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
{
    let mut slot = Some(body);
    if let Some(body) = (&mut slot as &mut dyn Any).downcast_mut::<Option<Body>>() {
        return body.take().unwrap_or_default();
    }
    match slot {
        Some(body) => Body::wrap(body),
        None => Body::empty(),
    }
}

/// A Vitesse application as a tower [`Service`], created by
/// [`App::into_service`](crate::App::into_service).
///
/// The client's address is read from a `SocketAddr` in the request's
/// extensions, if the server puts one there.
#[derive(Clone, Copy)]
pub struct AppService {
    app: &'static crate::app::AppService,
}

impl fmt::Debug for AppService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppService").finish_non_exhaustive()
    }
}

impl AppService {
    pub(crate) fn new(app: &'static crate::app::AppService) -> Self {
        AppService { app }
    }
}

impl<B> Service<http::Request<B>> for AppService
where
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
{
    type Response = http::Response<Body>;
    type Error = Infallible;
    type Future = BoxFuture<Result<http::Response<Body>, Infallible>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        let (parts, body) = req.into_parts();
        let remote = parts.extensions.get::<SocketAddr>().copied();
        let secure = parts.uri.scheme() == Some(&http::uri::Scheme::HTTPS);
        let body = ReqBody::Stream(into_body(body));
        let req = Request::from_parts(parts, body, remote, secure, &self.app.shared);
        let fut = ResponseFuture::new(self.app, req);
        Box::pin(async move {
            let mut res = fut.await.into_http();
            if res.status() == StatusCode::SWITCHING_PROTOCOLS {
                // Les WebSockets passent par le moteur de Vitesse.
                *res.status_mut() = StatusCode::NOT_IMPLEMENTED;
            }
            Ok(res)
        })
    }
}
