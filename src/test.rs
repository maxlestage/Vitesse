//! Tester une application sans ouvrir de port.
//!
//! ```
//! use vitesse::prelude::*;
//! use vitesse::test::TestClient;
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let mut app = App::new();
//! app.get("/hello/:name", |req: Request| async move {
//!     format!("Salut {} !", req.param("name").unwrap())
//! });
//!
//! let client = TestClient::new(app);
//! let res = client.get("/hello/Ada").await;
//! assert_eq!(res.status(), 200);
//! assert_eq!(res.text(), "Salut Ada !");
//! # }
//! ```

use std::future::IntoFuture;
use std::net::SocketAddr;

use bytes::Bytes;
use http::header::{self, HeaderMap, HeaderName, HeaderValue};
use http::{Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::app::{App, AppService};
use crate::handler::BoxFuture;
use crate::request::{ReqBody, Request};
use crate::server::ResponseFuture;

/// Un client qui appelle l'application directement, en mémoire.
#[derive(Clone, Copy)]
pub struct TestClient {
    app: &'static AppService,
}

impl TestClient {
    /// Fige l'application pour la tester.
    pub fn new(app: App) -> Self {
        TestClient { app: app.build() }
    }

    /// Une requête avec une méthode quelconque.
    pub fn request(&self, method: Method, uri: &str) -> TestRequest {
        TestRequest {
            app: self.app,
            method,
            uri: uri.to_owned(),
            headers: HeaderMap::new(),
            body: Bytes::new(),
        }
    }

    /// `GET uri`
    pub fn get(&self, uri: &str) -> TestRequest {
        self.request(Method::GET, uri)
    }

    /// `POST uri`
    pub fn post(&self, uri: &str) -> TestRequest {
        self.request(Method::POST, uri)
    }

    /// `PUT uri`
    pub fn put(&self, uri: &str) -> TestRequest {
        self.request(Method::PUT, uri)
    }

    /// `PATCH uri`
    pub fn patch(&self, uri: &str) -> TestRequest {
        self.request(Method::PATCH, uri)
    }

    /// `DELETE uri`
    pub fn delete(&self, uri: &str) -> TestRequest {
        self.request(Method::DELETE, uri)
    }
}

/// Une requête de test, à envoyer avec `.await` (ou [`TestRequest::send`]).
#[must_use = "une requête de test ne part qu'avec `.await`"]
pub struct TestRequest {
    app: &'static AppService,
    method: Method,
    uri: String,
    headers: HeaderMap,
    body: Bytes,
}

impl TestRequest {
    /// Ajoute un en-tête.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        let name = HeaderName::try_from(name).expect("nom d'en-tête invalide");
        let value = HeaderValue::try_from(value).expect("valeur d'en-tête invalide");
        self.headers.append(name, value);
        self
    }

    /// Corps brut.
    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = body.into();
        self
    }

    /// Corps JSON.
    pub fn json<T: Serialize>(mut self, value: &T) -> Self {
        self.body = serde_json::to_vec(value).expect("JSON invalide").into();
        self.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self
    }

    /// Corps de formulaire (`application/x-www-form-urlencoded`).
    pub fn form<T: Serialize>(mut self, value: &T) -> Self {
        self.body = serde_urlencoded::to_string(value)
            .expect("formulaire invalide")
            .into();
        self.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        self
    }

    /// Envoie la requête.
    pub async fn send(self) -> TestResponse {
        let mut builder = http::Request::builder()
            .method(self.method.clone())
            .uri(&self.uri);
        if let Some(headers) = builder.headers_mut() {
            *headers = self.headers;
        }
        let (head, ()) = builder.body(()).expect("requête invalide").into_parts();
        let peer = SocketAddr::from(([127, 0, 0, 1], 40000));
        let body = ReqBody::Stream(crate::Body::from(self.body));
        let req = Request::from_parts(head, body, Some(peer), &self.app.shared);
        let res = ResponseFuture::new(self.app, req).await;
        let (parts, body) = res.into_http().into_parts();
        let body = if self.method == Method::HEAD {
            Bytes::new()
        } else {
            body.to_bytes()
                .await
                .expect("lecture du corps de la réponse")
        };
        TestResponse {
            status: parts.status,
            headers: parts.headers,
            body,
        }
    }
}

impl IntoFuture for TestRequest {
    type Output = TestResponse;
    type IntoFuture = BoxFuture<TestResponse>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.send())
    }
}

/// La réponse d'une requête de test, avec son corps déjà lu.
#[derive(Debug)]
pub struct TestResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl TestResponse {
    /// Le statut.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Un en-tête.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    /// Tous les en-têtes.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Le corps brut.
    pub fn bytes(&self) -> &Bytes {
        &self.body
    }

    /// Le corps en texte.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Le corps désérialisé depuis du JSON (panique s'il est invalide).
    pub fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("corps JSON invalide ({e}) : {}", self.text()))
    }
}
