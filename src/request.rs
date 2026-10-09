//! La requête HTTP reçue par les handlers et les middlewares.

use std::any::{Any, TypeId};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::pin::pin;
use std::str::FromStr;
use std::sync::{Mutex, PoisonError};

use bytes::{Bytes, BytesMut};
use http::header::{self, AsHeaderName, HeaderMap};
use http::{Extensions, Method, Uri, Version};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use serde::de::DeserializeOwned;

use crate::body::{Body, BoxError};
use crate::error::Error;
use crate::tree::Captures;
use crate::util::percent_decode;

/// Données partagées par toute l'application (état, configuration).
pub(crate) struct Shared {
    pub(crate) state: StateMap,
    pub(crate) body_limit: usize,
}

/// Stockage typé de l'état global (`app.state(...)`).
#[derive(Default)]
pub(crate) struct StateMap(HashMap<TypeId, Box<dyn Any + Send + Sync>>);

impl StateMap {
    pub(crate) fn insert<T: Send + Sync + 'static>(&mut self, value: T) {
        self.0.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub(crate) fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.0
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref())
    }
}

pub(crate) enum ReqBody {
    Incoming(Incoming),
    Buffered(Bytes),
    Stream(Body),
    Taken,
}

/// Valeur d'un paramètre de route : une tranche du chemin, ou une chaîne
/// décodée si le segment contenait des `%XX`.
enum ParamValue {
    Slice(u32, u32),
    Owned(Box<str>),
}

#[derive(Default)]
pub(crate) struct Params {
    names: &'static [Box<str>],
    values: Vec<ParamValue>,
}

impl Params {
    pub(crate) fn new(names: &'static [Box<str>], path: &str, captures: Captures) -> Self {
        let values = captures
            .into_iter()
            .map(|(a, b)| match percent_decode(&path[a..b]) {
                Cow::Borrowed(_) => ParamValue::Slice(a as u32, b as u32),
                Cow::Owned(s) => ParamValue::Owned(s.into_boxed_str()),
            })
            .collect();
        Params { names, values }
    }
}

/// Une requête HTTP (`req` en Express).
///
/// Toutes les méthodes prennent `&self`, y compris la lecture du corps : on
/// peut donc garder un paramètre emprunté pendant qu'on lit le JSON.
///
/// ```
/// use vitesse::prelude::*;
/// # #[derive(serde::Deserialize)] struct Patch { name: String }
///
/// async fn update(req: Request) -> vitesse::Result<String> {
///     let id: u64 = req.param_as("id")?;
///     let page = req.query("page");
///     let patch: Patch = req.json().await?;
///     Ok(format!("user {id} -> {} (page {page:?})", patch.name))
/// }
/// ```
pub struct Request {
    pub(crate) head: http::request::Parts,
    body: Mutex<ReqBody>,
    pub(crate) params: Params,
    remote: Option<SocketAddr>,
    shared: &'static Shared,
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("method", &self.head.method)
            .field("uri", &self.head.uri)
            .field("version", &self.head.version)
            .field("headers", &self.head.headers)
            .finish()
    }
}

impl Request {
    #[inline]
    pub(crate) fn new(
        head: http::request::Parts,
        body: ReqBody,
        remote: Option<SocketAddr>,
        shared: &'static Shared,
    ) -> Self {
        Request {
            head,
            body: Mutex::new(body),
            params: Params::default(),
            remote,
            shared,
        }
    }

    // ----- Ligne de requête -------------------------------------------------

    /// La méthode HTTP (`req.method`).
    #[inline]
    pub fn method(&self) -> &Method {
        &self.head.method
    }

    /// L'URI complète (chemin + query string).
    #[inline]
    pub fn uri(&self) -> &Uri {
        &self.head.uri
    }

    /// Le chemin, sans la query string (`req.path`).
    #[inline]
    pub fn path(&self) -> &str {
        self.head.uri.path()
    }

    /// Remplace l'URI (ex. réécriture d'URL dans un middleware global).
    pub fn set_uri(&mut self, uri: Uri) {
        // Les paramètres pointent dans l'ancien chemin : on les rend autonomes.
        let path = self.head.uri.path();
        for value in &mut self.params.values {
            if let ParamValue::Slice(a, b) = *value {
                *value = ParamValue::Owned(Box::from(&path[a as usize..b as usize]));
            }
        }
        self.head.uri = uri;
    }

    /// La version HTTP.
    #[inline]
    pub fn version(&self) -> Version {
        self.head.version
    }

    // ----- En-têtes ---------------------------------------------------------

    /// Tous les en-têtes.
    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        &self.head.headers
    }

    /// Tous les en-têtes, modifiables.
    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.head.headers
    }

    /// Un en-tête sous forme de texte (`req.get('host')`).
    #[inline]
    pub fn header(&self, name: impl AsHeaderName) -> Option<&str> {
        self.head.headers.get(name).and_then(|v| v.to_str().ok())
    }

    /// Le `Content-Type`.
    pub fn content_type(&self) -> Option<&str> {
        self.header(header::CONTENT_TYPE)
    }

    /// Vérifie le type du corps (`req.is('json')`) : accepte un type complet
    /// (`application/json`), un sous-type (`json`) ou un joker (`text/*`).
    pub fn is(&self, ty: &str) -> bool {
        let Some(ct) = self.content_type() else {
            return false;
        };
        let mime = ct.split(';').next().unwrap_or("").trim();
        let Some((main, sub)) = mime.split_once('/') else {
            return false;
        };
        match ty.split_once('/') {
            Some((m, "*")) => main.eq_ignore_ascii_case(m),
            Some(_) => mime.eq_ignore_ascii_case(ty),
            None => {
                sub.eq_ignore_ascii_case(ty)
                    || sub
                        .rsplit_once('+')
                        .is_some_and(|(_, s)| s.eq_ignore_ascii_case(ty))
            }
        }
    }

    /// L'hôte demandé, sans le port (`req.hostname`).
    pub fn hostname(&self) -> Option<&str> {
        let host = self.header(header::HOST).or_else(|| self.head.uri.host())?;
        if host.starts_with('[') {
            return host.split_once(']').map(|(h, _)| &h[1..]);
        }
        Some(host.rsplit_once(':').map_or(host, |(h, _)| h))
    }

    /// La valeur d'un cookie (`req.cookies.name`).
    pub fn cookie(&self, name: &str) -> Option<&str> {
        for value in self.head.headers.get_all(header::COOKIE) {
            let Ok(s) = value.to_str() else { continue };
            for pair in s.split(';') {
                if let Some((k, v)) = pair.split_once('=') {
                    if k.trim() == name {
                        let v = v.trim();
                        return Some(
                            v.strip_prefix('"')
                                .and_then(|v| v.strip_suffix('"'))
                                .unwrap_or(v),
                        );
                    }
                }
            }
        }
        None
    }

    // ----- Paramètres de route et query string ------------------------------

    /// Un paramètre de route (`req.params.id` pour la route `/users/:id`).
    ///
    /// Pour un joker `*chemin`, le nom est `chemin` (ou `*` pour un joker anonyme).
    #[inline]
    pub fn param(&self, name: &str) -> Option<&str> {
        let i = self.params.names.iter().position(|n| &**n == name)?;
        Some(match self.params.values.get(i)? {
            ParamValue::Slice(a, b) => &self.head.uri.path()[*a as usize..*b as usize],
            ParamValue::Owned(s) => s,
        })
    }

    /// Un paramètre de route converti dans le type voulu ; `400` en cas d'échec.
    ///
    /// ```
    /// # use vitesse::prelude::*;
    /// async fn show(req: Request) -> vitesse::Result<String> {
    ///     let id: u32 = req.param_as("id")?;
    ///     Ok(format!("user {id}"))
    /// }
    /// ```
    pub fn param_as<T: FromStr>(&self, name: &str) -> Result<T, Error> {
        let raw = self
            .param(name)
            .ok_or_else(|| Error::bad_request(format!("paramètre « {name} » manquant")))?;
        raw.parse()
            .map_err(|_| Error::bad_request(format!("paramètre « {name} » invalide : « {raw} »")))
    }

    /// Tous les paramètres de route, sous forme de paires `(nom, valeur)`.
    pub fn params(&self) -> impl Iterator<Item = (&str, &str)> {
        self.params
            .names
            .iter()
            .filter_map(|n| Some((&**n, self.param(n)?)))
    }

    /// La query string brute (`a=1&b=2`).
    #[inline]
    pub fn query_string(&self) -> Option<&str> {
        self.head.uri.query()
    }

    /// Un paramètre de query string décodé (`req.query.page`).
    ///
    /// N'alloue que si la valeur contient des caractères encodés.
    pub fn query(&self, name: &str) -> Option<Cow<'_, str>> {
        let q = self.head.uri.query()?;
        form_urlencoded::parse(q.as_bytes())
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }

    /// Toutes les paires de la query string.
    pub fn query_pairs(&self) -> impl Iterator<Item = (Cow<'_, str>, Cow<'_, str>)> {
        form_urlencoded::parse(self.head.uri.query().unwrap_or("").as_bytes())
    }

    /// Désérialise la query string dans une structure ; `400` en cas d'échec.
    ///
    /// ```
    /// # use vitesse::prelude::*;
    /// #[derive(serde::Deserialize)]
    /// struct Search { q: String, page: Option<u32> }
    ///
    /// async fn search(req: Request) -> vitesse::Result<String> {
    ///     let s: Search = req.query_as()?;
    ///     Ok(format!("{} p.{}", s.q, s.page.unwrap_or(1)))
    /// }
    /// ```
    pub fn query_as<T: DeserializeOwned>(&self) -> Result<T, Error> {
        serde_urlencoded::from_str(self.head.uri.query().unwrap_or(""))
            .map_err(|e| Error::bad_request(format!("query string invalide : {e}")))
    }

    // ----- Connexion --------------------------------------------------------

    /// L'adresse du client (IP + port).
    #[inline]
    pub fn remote_addr(&self) -> Option<SocketAddr> {
        self.remote
    }

    /// L'IP du client (`req.ip`). Derrière un proxy, voyez plutôt
    /// l'en-tête `X-Forwarded-For`.
    #[inline]
    pub fn ip(&self) -> Option<IpAddr> {
        self.remote.map(|a| a.ip())
    }

    // ----- État global et données locales -----------------------------------

    /// L'état global enregistré avec `app.state(valeur)` (`app.locals`).
    ///
    /// # Panique
    /// Si aucun état de ce type n'a été enregistré (la panique est convertie
    /// en `500`). Voir [`Request::try_state`].
    #[inline]
    pub fn state<T: Send + Sync + 'static>(&self) -> &'static T {
        self.try_state().unwrap_or_else(|| {
            panic!(
                "aucun état de type `{}` : appelez `app.state(...)` au démarrage",
                std::any::type_name::<T>()
            )
        })
    }

    /// Comme [`Request::state`], sans paniquer.
    #[inline]
    pub fn try_state<T: Send + Sync + 'static>(&self) -> Option<&'static T> {
        self.shared.state.get::<T>()
    }

    /// Lit une donnée attachée à cette requête par un middleware (`res.locals`).
    #[inline]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.head.extensions.get::<T>()
    }

    /// Attache une donnée à cette requête (ex. l'utilisateur authentifié).
    #[inline]
    pub fn set<T: Clone + Send + Sync + 'static>(&mut self, value: T) -> Option<T> {
        self.head.extensions.insert(value)
    }

    /// Les extensions de la requête.
    pub fn extensions(&self) -> &Extensions {
        &self.head.extensions
    }

    /// Les extensions de la requête, modifiables.
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.head.extensions
    }

    // ----- Corps ------------------------------------------------------------

    fn lock_body(&self) -> std::sync::MutexGuard<'_, ReqBody> {
        self.body.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Lit tout le corps (limité par `app.body_limit`, `413` au-delà).
    ///
    /// Le résultat est mis en cache : on peut l'appeler plusieurs fois.
    pub async fn bytes(&self) -> Result<Bytes, Error> {
        let body = {
            let mut guard = self.lock_body();
            if let ReqBody::Buffered(bytes) = &*guard {
                return Ok(bytes.clone());
            }
            std::mem::replace(&mut *guard, ReqBody::Taken)
        };
        let limit = self.shared.body_limit;
        let bytes = match body {
            ReqBody::Incoming(incoming) => collect(incoming, limit).await?,
            ReqBody::Stream(stream) => collect(stream, limit).await?,
            ReqBody::Buffered(bytes) => bytes,
            ReqBody::Taken => {
                return Err(Error::internal(
                    "le corps de la requête a déjà été consommé",
                ));
            }
        };
        *self.lock_body() = ReqBody::Buffered(bytes.clone());
        Ok(bytes)
    }

    /// Lit le corps comme du texte UTF-8 ; `400` s'il est invalide.
    pub async fn text(&self) -> Result<String, Error> {
        let bytes = self.bytes().await?;
        std::str::from_utf8(&bytes)
            .map(str::to_owned)
            .map_err(|_| Error::bad_request("le corps n'est pas de l'UTF-8 valide"))
    }

    /// Désérialise un corps JSON (`express.json()`) ; `400` s'il est invalide.
    pub async fn json<T: DeserializeOwned>(&self) -> Result<T, Error> {
        let bytes = self.bytes().await?;
        serde_json::from_slice(&bytes)
            .map_err(|e| Error::bad_request(format!("JSON invalide : {e}")))
    }

    /// Désérialise un formulaire `application/x-www-form-urlencoded`
    /// (`express.urlencoded()`) ; `400` s'il est invalide.
    pub async fn form<T: DeserializeOwned>(&self) -> Result<T, Error> {
        let bytes = self.bytes().await?;
        serde_urlencoded::from_bytes(&bytes)
            .map_err(|e| Error::bad_request(format!("formulaire invalide : {e}")))
    }

    /// Récupère le corps brut en flux, sans le lire (upload, proxy…).
    pub fn take_body(&self) -> Body {
        match std::mem::replace(&mut *self.lock_body(), ReqBody::Taken) {
            ReqBody::Incoming(incoming) => Body::wrap(incoming),
            ReqBody::Buffered(bytes) => Body::from(bytes),
            ReqBody::Stream(body) => body,
            ReqBody::Taken => Body::empty(),
        }
    }

    /// Remplace le corps (ex. un middleware qui décompresse).
    pub fn set_body(&mut self, body: impl Into<Body>) {
        *self.body.get_mut().unwrap_or_else(PoisonError::into_inner) = ReqBody::Stream(body.into());
    }
}

/// Lit un corps en mémoire, avec une taille maximale.
async fn collect<B>(body: B, limit: usize) -> Result<Bytes, Error>
where
    B: http_body::Body<Data = Bytes>,
    B::Error: Into<BoxError>,
{
    if body.size_hint().lower() > limit as u64 {
        return Err(Error::payload_too_large());
    }
    let mut body = pin!(body);
    let mut first: Option<Bytes> = None;
    let mut rest: Option<BytesMut> = None;
    let mut total = 0usize;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|e| {
            Error::bad_request("lecture du corps de la requête impossible").with_source(e.into())
        })?;
        let Ok(data) = frame.into_data() else {
            continue;
        };
        total += data.len();
        if total > limit {
            return Err(Error::payload_too_large());
        }
        if let Some(buf) = rest.as_mut() {
            buf.extend_from_slice(&data);
        } else if let Some(prev) = first.take() {
            let mut buf = BytesMut::with_capacity(prev.len() + data.len());
            buf.extend_from_slice(&prev);
            buf.extend_from_slice(&data);
            rest = Some(buf);
        } else {
            first = Some(data);
        }
    }
    Ok(match rest {
        Some(buf) => buf.freeze(),
        None => first.unwrap_or_default(),
    })
}
