//! La requête HTTP reçue par les handlers et les middlewares.

use std::any::{Any, TypeId};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::mem::ManuallyDrop;
use std::net::{IpAddr, SocketAddr};
use std::pin::pin;
use std::str::FromStr;
use std::sync::{Mutex, OnceLock, PoisonError};

use bytes::{Bytes, BytesMut};
use http::header::{self, HeaderMap, HeaderName, HeaderValue};
use http::{Extensions, Method, Uri, Version};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;

use crate::body::{Body, BoxError};
use crate::error::Error;
use crate::tree::{Captures, Tree};
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
    Empty,
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

/// Position d'un en-tête (nom et valeur) dans les octets bruts de la requête.
#[derive(Clone, Copy, Default)]
struct Slot {
    name: u16,
    name_len: u16,
    value: u16,
    value_len: u16,
}

const INLINE_SLOTS: usize = 12;

/// Les en-têtes tels que reçus : de simples positions, sans copie ni
/// allocation (au-delà de 12 en-têtes, les suivants vont dans un `Vec`).
#[derive(Default)]
pub(crate) struct RawHeaders {
    len: usize,
    inline: [Slot; INLINE_SLOTS],
    extra: Vec<Slot>,
}

impl RawHeaders {
    /// Ajoute un en-tête ; les positions doivent tenir sur 16 bits.
    #[inline]
    pub(crate) fn push(&mut self, name: usize, name_len: usize, value: usize, value_len: usize) {
        let slot = Slot {
            name: name as u16,
            name_len: name_len as u16,
            value: value as u16,
            value_len: value_len as u16,
        };
        if self.len < INLINE_SLOTS {
            self.inline[self.len] = slot;
        } else {
            self.extra.push(slot);
        }
        self.len += 1;
    }

    #[inline]
    pub(crate) fn clear(&mut self) {
        self.len = 0;
        self.extra.clear();
    }

    #[inline]
    fn copy_from(&mut self, other: &RawHeaders) {
        self.len = other.len;
        self.inline = other.inline;
        self.extra.clone_from(&other.extra);
    }

    #[inline]
    fn iter(&self) -> impl Iterator<Item = Slot> + '_ {
        self.inline[..self.len.min(INLINE_SLOTS)]
            .iter()
            .chain(&self.extra)
            .copied()
    }
}

/// Le chemin d'une cible `/chemin?query`.
#[inline]
fn path_of(head: &[u8], target: (u32, u32), path_end: u32) -> &str {
    std::str::from_utf8(&head[target.0 as usize..path_end as usize]).unwrap_or("/")
}

/// Réserve de requêtes par thread : la boîte d'une requête terminée (et ses
/// tampons) sert à la suivante, sans passer par l'allocateur.
// Les boîtes sont voulues : c'est la boîte elle-même qu'on rend à une requête,
// sans recopier son contenu.
#[allow(clippy::vec_box)]
mod pool {
    use std::cell::RefCell;

    use super::Inner;

    const MAX_POOLED: usize = 256;

    thread_local! {
        static POOL: RefCell<Vec<Box<Inner>>> = const { RefCell::new(Vec::new()) };
    }

    #[inline]
    pub(super) fn take() -> Option<Box<Inner>> {
        POOL.try_with(|pool| pool.borrow_mut().pop()).ok().flatten()
    }

    #[inline]
    pub(super) fn give(inner: Box<Inner>) {
        let _ = POOL.try_with(|pool| {
            let mut pool = pool.borrow_mut();
            if pool.len() < MAX_POOLED {
                pool.push(inner);
            }
        });
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
    /// Rendue à la réserve du thread par `Drop`.
    inner: ManuallyDrop<Box<Inner>>,
}

/// Le contenu de la requête, derrière un pointeur : la requête traverse les
/// middlewares et les handlers en ne déplaçant que 8 octets.
struct Inner {
    method: Method,
    version: Version,
    /// Les octets bruts de la tête, référencés par `target` et `raw_headers`.
    head: Vec<u8>,
    /// Position de la cible (`/chemin?query`) dans `head`, et du `?`.
    target: (u32, u32),
    path_end: u32,
    raw_headers: RawHeaders,
    /// Construits seulement si on les demande.
    headers: OnceLock<Box<HeaderMap>>,
    uri: OnceLock<Box<Uri>>,
    extensions: Extensions,
    body: Mutex<ReqBody>,
    /// Positions trouvées par le routeur (tampon réutilisé).
    captures: Captures,
    params: Params,
    remote: Option<SocketAddr>,
    shared: &'static Shared,
}

impl Inner {
    fn new(shared: &'static Shared) -> Self {
        Inner {
            method: Method::GET,
            version: Version::HTTP_11,
            head: Vec::new(),
            target: (0, 0),
            path_end: 0,
            raw_headers: RawHeaders::default(),
            headers: OnceLock::new(),
            uri: OnceLock::new(),
            extensions: Extensions::new(),
            body: Mutex::new(ReqBody::Empty),
            captures: Vec::new(),
            params: Params::default(),
            remote: None,
            shared,
        }
    }

    /// Une boîte de la réserve, ou une nouvelle.
    #[inline]
    fn take(shared: &'static Shared) -> Box<Inner> {
        match pool::take() {
            Some(mut inner) => {
                inner.shared = shared;
                inner
            }
            None => Box::new(Inner::new(shared)),
        }
    }

    /// Vide la requête pour la réutiliser (en gardant ses tampons).
    fn reset(&mut self) {
        if self.head.capacity() > 16 * 1024 {
            self.head = Vec::new();
        } else {
            self.head.clear();
        }
        self.raw_headers.clear();
        self.headers = OnceLock::new();
        self.uri = OnceLock::new();
        self.extensions.clear();
        *self.body.get_mut().unwrap_or_else(PoisonError::into_inner) = ReqBody::Empty;
        self.captures.clear();
        self.params.names = &[];
        self.params.values.clear();
        self.remote = None;
    }

    /// Ajoute la cible à la fin de `head` et la désigne.
    fn push_target(&mut self, target: &[u8]) {
        let start = self.head.len();
        self.head
            .extend_from_slice(if target.is_empty() { b"/" } else { target });
        self.set_target(start, self.head.len());
    }

    fn set_target(&mut self, start: usize, end: usize) {
        let path_end = self.head[start..end]
            .iter()
            .position(|&b| b == b'?')
            .map_or(end, |i| start + i);
        self.target = (start as u32, end as u32);
        self.path_end = path_end as u32;
    }

    #[inline]
    fn path(&self) -> &str {
        path_of(&self.head, self.target, self.path_end)
    }

    #[inline]
    fn query(&self) -> Option<&str> {
        if self.path_end >= self.target.1 {
            return None;
        }
        std::str::from_utf8(&self.head[self.path_end as usize + 1..self.target.1 as usize]).ok()
    }

    #[inline]
    fn target_bytes(&self) -> &[u8] {
        &self.head[self.target.0 as usize..self.target.1 as usize]
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        // SAFETY : `inner` n'est plus jamais lu après ce point.
        let mut inner = unsafe { ManuallyDrop::take(&mut self.inner) };
        inner.reset();
        pool::give(inner);
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("method", &self.inner.method)
            .field("uri", self.uri())
            .field("version", &self.inner.version)
            .field("headers", self.headers())
            .finish()
    }
}

impl Request {
    /// Construit une requête lue sur le réseau ; `head` est la tête brute,
    /// copiée dans une boîte recyclée.
    #[inline]
    pub(crate) fn from_wire(
        head: &[u8],
        method: Method,
        version: Version,
        target: (usize, usize),
        raw_headers: &RawHeaders,
        remote: Option<SocketAddr>,
        shared: &'static Shared,
    ) -> Self {
        let mut inner = Inner::take(shared);
        inner.method = method;
        inner.version = version;
        inner.head.extend_from_slice(head);
        inner.raw_headers.copy_from(raw_headers);
        inner.remote = remote;
        let (start, end) = target;
        if head.get(start) == Some(&b'/') {
            inner.set_target(start, end);
        } else {
            // Forme absolue (`GET http://hôte/chemin`) ou `*` : rare, on passe par `Uri`.
            match Uri::try_from(&head[start..end]) {
                Ok(uri) => {
                    let pq = uri
                        .path_and_query()
                        .map_or("/", |pq| pq.as_str())
                        .to_owned();
                    inner.push_target(pq.as_bytes());
                    inner.uri = OnceLock::from(Box::new(uri));
                }
                Err(_) => inner.set_target(start, end),
            }
        }
        Request {
            inner: ManuallyDrop::new(inner),
        }
    }

    /// Construit une requête à partir d'une [`http::Request`] (client de test).
    pub(crate) fn from_parts(
        parts: http::request::Parts,
        body: ReqBody,
        remote: Option<SocketAddr>,
        shared: &'static Shared,
    ) -> Self {
        let mut inner = Inner::take(shared);
        inner.method = parts.method;
        inner.version = parts.version;
        let pq = parts.uri.path_and_query().map_or("/", |pq| pq.as_str());
        inner.push_target(pq.as_bytes());
        inner.headers = OnceLock::from(Box::new(parts.headers));
        inner.uri = OnceLock::from(Box::new(parts.uri));
        inner.extensions = parts.extensions;
        *inner.body.get_mut().unwrap_or_else(PoisonError::into_inner) = body;
        inner.remote = remote;
        Request {
            inner: ManuallyDrop::new(inner),
        }
    }

    /// Donne son corps à la requête.
    #[inline]
    pub(crate) fn put_body(&mut self, body: ReqBody) {
        *self
            .inner
            .body
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner) = body;
    }

    /// Cherche la route de la requête dans l'arbre ; les positions des
    /// paramètres sont gardées pour [`Request::set_params`].
    #[inline]
    pub(crate) fn find_route<T>(&mut self, tree: &'static Tree<T>) -> Option<&'static T> {
        let inner = &mut **self.inner;
        let path = path_of(&inner.head, inner.target, inner.path_end);
        tree.find(path, &mut inner.captures)
    }

    /// Enregistre les paramètres de la route trouvée par le routeur.
    #[inline]
    pub(crate) fn set_params(&mut self, names: &'static [Box<str>]) {
        let inner = &mut **self.inner;
        let path = path_of(&inner.head, inner.target, inner.path_end);
        inner.params.names = names;
        inner.params.values.clear();
        for &(a, b) in &inner.captures {
            inner.params.values.push(match percent_decode(&path[a..b]) {
                Cow::Borrowed(_) => ParamValue::Slice(a as u32, b as u32),
                Cow::Owned(s) => ParamValue::Owned(s.into_boxed_str()),
            });
        }
    }

    // ----- Ligne de requête -------------------------------------------------

    /// La méthode HTTP (`req.method`).
    #[inline]
    pub fn method(&self) -> &Method {
        &self.inner.method
    }

    /// L'URI complète (chemin + query string).
    pub fn uri(&self) -> &Uri {
        self.inner.uri.get_or_init(|| {
            Box::new(
                Uri::try_from(self.inner.target_bytes()).unwrap_or_else(|_| Uri::from_static("/")),
            )
        })
    }

    /// Le chemin, sans la query string (`req.path`).
    #[inline]
    pub fn path(&self) -> &str {
        self.inner.path()
    }

    /// Remplace l'URI (ex. réécriture d'URL dans un middleware global).
    pub fn set_uri(&mut self, uri: Uri) {
        // Les paramètres pointent dans l'ancien chemin : on les rend autonomes.
        let inner = &mut **self.inner;
        let path = path_of(&inner.head, inner.target, inner.path_end);
        for value in &mut inner.params.values {
            if let ParamValue::Slice(a, b) = *value {
                *value = ParamValue::Owned(Box::from(&path[a as usize..b as usize]));
            }
        }
        // La nouvelle cible s'ajoute après les en-têtes bruts, toujours valides.
        let pq = uri
            .path_and_query()
            .map_or("/", |pq| pq.as_str())
            .to_owned();
        inner.push_target(pq.as_bytes());
        inner.uri = OnceLock::from(Box::new(uri));
    }

    /// La version HTTP.
    #[inline]
    pub fn version(&self) -> Version {
        self.inner.version
    }

    // ----- En-têtes ---------------------------------------------------------

    fn raw_name(&self, slot: Slot) -> &[u8] {
        &self.inner.head[slot.name as usize..][..slot.name_len as usize]
    }

    fn raw_value(&self, slot: Slot) -> &[u8] {
        &self.inner.head[slot.value as usize..][..slot.value_len as usize]
    }

    /// Tous les en-têtes (construits à la première demande).
    pub fn headers(&self) -> &HeaderMap {
        self.inner.headers.get_or_init(|| {
            let mut map = HeaderMap::with_capacity(self.inner.raw_headers.len);
            for slot in self.inner.raw_headers.iter() {
                let name = HeaderName::from_bytes(self.raw_name(slot));
                let value = HeaderValue::from_bytes(self.raw_value(slot));
                if let (Ok(name), Ok(value)) = (name, value) {
                    map.append(name, value);
                }
            }
            Box::new(map)
        })
    }

    /// Tous les en-têtes, modifiables.
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        self.headers();
        self.inner.headers.get_mut().expect("en-têtes initialisés")
    }

    /// Un en-tête sous forme de texte (`req.get('host')`), sans tenir compte
    /// de la casse du nom.
    #[inline]
    pub fn header(&self, name: impl AsRef<str>) -> Option<&str> {
        let name = name.as_ref();
        if let Some(map) = self.inner.headers.get() {
            return map.get(name).and_then(|v| v.to_str().ok());
        }
        self.inner
            .raw_headers
            .iter()
            .find(|&slot| self.raw_name(slot).eq_ignore_ascii_case(name.as_bytes()))
            .and_then(|slot| std::str::from_utf8(self.raw_value(slot)).ok())
    }

    /// Toutes les valeurs d'un en-tête répété.
    pub fn header_all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        let from_map = self.inner.headers.get().map(|map| {
            map.get_all(name)
                .into_iter()
                .filter_map(|v| v.to_str().ok())
        });
        let from_raw = if from_map.is_none() {
            Some(
                self.inner
                    .raw_headers
                    .iter()
                    .filter(move |&slot| self.raw_name(slot).eq_ignore_ascii_case(name.as_bytes()))
                    .filter_map(move |slot| std::str::from_utf8(self.raw_value(slot)).ok()),
            )
        } else {
            None
        };
        from_map
            .into_iter()
            .flatten()
            .chain(from_raw.into_iter().flatten())
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
        let host = match self.header(header::HOST) {
            Some(host) => host,
            None => self.inner.uri.get()?.host()?,
        };
        if host.starts_with('[') {
            return host.split_once(']').map(|(h, _)| &h[1..]);
        }
        Some(host.rsplit_once(':').map_or(host, |(h, _)| h))
    }

    /// La valeur d'un cookie (`req.cookies.name`).
    pub fn cookie(&self, name: &str) -> Option<&str> {
        for s in self.header_all("cookie") {
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
        let i = self.inner.params.names.iter().position(|n| &**n == name)?;
        Some(match self.inner.params.values.get(i)? {
            ParamValue::Slice(a, b) => &self.inner.path()[*a as usize..*b as usize],
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
        self.inner
            .params
            .names
            .iter()
            .filter_map(|n| Some((&**n, self.param(n)?)))
    }

    /// La query string brute (`a=1&b=2`).
    #[inline]
    pub fn query_string(&self) -> Option<&str> {
        self.inner.query()
    }

    /// Un paramètre de query string décodé (`req.query.page`).
    ///
    /// N'alloue que si la valeur contient des caractères encodés.
    pub fn query(&self, name: &str) -> Option<Cow<'_, str>> {
        let q = self.inner.query()?;
        form_urlencoded::parse(q.as_bytes())
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }

    /// Toutes les paires de la query string.
    pub fn query_pairs(&self) -> impl Iterator<Item = (Cow<'_, str>, Cow<'_, str>)> {
        form_urlencoded::parse(self.inner.query().unwrap_or("").as_bytes())
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
        serde_urlencoded::from_str(self.inner.query().unwrap_or(""))
            .map_err(|e| Error::bad_request(format!("query string invalide : {e}")))
    }

    // ----- Connexion --------------------------------------------------------

    /// L'adresse du client (IP + port).
    #[inline]
    pub fn remote_addr(&self) -> Option<SocketAddr> {
        self.inner.remote
    }

    /// L'IP du client (`req.ip`). Derrière un proxy, voyez plutôt
    /// l'en-tête `X-Forwarded-For`.
    #[inline]
    pub fn ip(&self) -> Option<IpAddr> {
        self.inner.remote.map(|a| a.ip())
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
        self.inner.shared.state.get::<T>()
    }

    /// Lit une donnée attachée à cette requête par un middleware (`res.locals`).
    #[inline]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.inner.extensions.get::<T>()
    }

    /// Attache une donnée à cette requête (ex. l'utilisateur authentifié).
    #[inline]
    pub fn set<T: Clone + Send + Sync + 'static>(&mut self, value: T) -> Option<T> {
        self.inner.extensions.insert(value)
    }

    /// Les extensions de la requête.
    pub fn extensions(&self) -> &Extensions {
        &self.inner.extensions
    }

    /// Les extensions de la requête, modifiables.
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.inner.extensions
    }

    // ----- Corps ------------------------------------------------------------

    fn lock_body(&self) -> std::sync::MutexGuard<'_, ReqBody> {
        self.inner
            .body
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Lit tout le corps (limité par `app.body_limit`, `413` au-delà).
    ///
    /// Le résultat est mis en cache : on peut l'appeler plusieurs fois.
    pub async fn bytes(&self) -> Result<Bytes, Error> {
        let limit = self.inner.shared.body_limit;
        let body = {
            let mut guard = self.lock_body();
            match &*guard {
                ReqBody::Empty => return Ok(Bytes::new()),
                ReqBody::Buffered(bytes) if bytes.len() > limit => {
                    return Err(Error::payload_too_large());
                }
                ReqBody::Buffered(bytes) => return Ok(bytes.clone()),
                _ => std::mem::replace(&mut *guard, ReqBody::Taken),
            }
        };
        let bytes = match body {
            ReqBody::Stream(stream) => collect(stream, limit).await?,
            ReqBody::Empty | ReqBody::Buffered(_) => unreachable!("traité ci-dessus"),
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
            ReqBody::Empty => Body::empty(),
            ReqBody::Buffered(bytes) => Body::from(bytes),
            ReqBody::Stream(body) => body,
            ReqBody::Taken => Body::empty(),
        }
    }

    /// Remplace le corps (ex. un middleware qui décompresse).
    pub fn set_body(&mut self, body: impl Into<Body>) {
        *self
            .inner
            .body
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner) = ReqBody::Stream(body.into());
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
