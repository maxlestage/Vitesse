//! Le serveur : une application Vitesse (le site est servi par le framework qu'il
//! présente). Elle rend les pages dans les trois langues avec active, sert les
//! fichiers statiques (`assets/`) et le code du navigateur (`pkg/`) avec
//! `static_dir`, l'index de recherche, le plan du site, le manifeste et le service
//! worker de la PWA.

pub mod csp;
pub mod docs;
pub mod export;
pub mod markdown;
pub mod pages;
pub mod pwa;

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use vitesse::prelude::*;

use crate::i18n::{Lang, pick_lang};
use crate::routes::{self, Page as PageKind, Route, base, link};
use docs::Docs;

/// Le nom du cookie de la langue choisie dans le sélecteur.
pub const LANG_COOKIE: &str = "vitesse-lang";
/// Le cookie qui ne fait jouer l'écran de chargement qu'une fois par visite.
pub const INTRO_COOKIE: &str = "vitesse-intro";

/// Le dossier `site/` : `SITE_DIR`, sinon le dossier courant s'il contient
/// `assets/`, sinon celui de la crate.
pub fn site_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SITE_DIR") {
        return PathBuf::from(dir);
    }
    if Path::new("assets/main.css").is_file() {
        return PathBuf::from(".");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// FNV-1a, assez pour distinguer deux versions d'un fichier.
pub fn version(bytes: &[u8]) -> String {
    let h = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{h:016x}")[..10].to_owned()
}

/// L'empreinte de chaque fichier statique, pour les liens `?v=` (que les
/// navigateurs gardent en cache pour toujours) et la version du service worker.
#[derive(Default)]
pub struct Assets {
    files: BTreeMap<String, String>,
}

impl Assets {
    /// Les fichiers de `dir`, récursivement, sous le préfixe d'adresse `prefix`.
    pub fn load(&mut self, dir: &Path, prefix: &str) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                self.load(&path, &format!("{prefix}/{name}"));
            } else if let Ok(bytes) = std::fs::read(&path) {
                self.files
                    .insert(format!("{prefix}/{name}"), version(&bytes));
            }
        }
    }

    /// Le lien vers `url`, derrière le préfixe, avec `?v=<empreinte>` si le fichier
    /// existe.
    pub fn url(&self, url: &str) -> String {
        match self.files.get(url) {
            Some(hash) => format!("{}?v={hash}", link(url)),
            None => link(url),
        }
    }

    pub fn has(&self, url: &str) -> bool {
        self.files.contains_key(url)
    }

    /// Les adresses des fichiers dont le nom commence par `prefix`.
    pub fn under<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.files
            .keys()
            .filter(move |url| url.starts_with(prefix))
            .map(String::as_str)
    }

    /// Une empreinte de tous les fichiers.
    pub fn digest(&self) -> String {
        let all: String = self
            .files
            .iter()
            .map(|(url, hash)| format!("{url}={hash};"))
            .collect();
        version(all.as_bytes())
    }
}

pub struct Site {
    pub docs: Docs,
    pub assets: Assets,
    /// L'adresse publique, avec le préfixe (`SITE_URL`) ; `None` : celle de chaque
    /// requête.
    pub site_url: Option<String>,
    /// Le serveur en marche ; `false` pour l'export statique.
    pub live: bool,
    /// La version du service worker : elle change avec les styles, le code du
    /// navigateur, la documentation et les pages.
    pub version: String,
    /// La Content-Security-Policy des pages.
    pub policy: String,
}

impl Site {
    /// Lit la documentation et les fichiers statiques. Appeler
    /// [`routes::set_base`] avant.
    pub fn load(live: bool, site_url: Option<String>) -> Site {
        let dir = site_dir();
        let mut assets = Assets::default();
        assets.load(&dir.join("assets"), "/assets");
        assets.load(&dir.join("pkg"), "/pkg");
        let mut site = Site {
            docs: Docs::load(&docs::docs_dir()),
            assets,
            site_url: site_url
                .map(|u| u.trim().trim_end_matches('/').to_owned())
                .filter(|u| !u.is_empty())
                .map(|u| {
                    if u.ends_with(base()) {
                        u
                    } else {
                        format!("{u}{}", base())
                    }
                }),
            live,
            version: String::new(),
            policy: String::new(),
        };
        site.policy = pages::policy(&site);
        // Les pages mises en cache changent aussi quand seul leur code change.
        let pages: String = Lang::ALL
            .iter()
            .flat_map(|&l| [Route::new(l, PageKind::Home), Route::new(l, PageKind::Docs)])
            .map(|route| {
                pages::render(&pages::Ctx {
                    site: &site,
                    route,
                    origin: String::new(),
                    intro: false,
                })
            })
            .collect();
        site.version = version(
            format!(
                "{}{}{}{}",
                env!("CARGO_PKG_VERSION"),
                site.assets.digest(),
                site.docs.digest,
                version(pages.as_bytes())
            )
            .as_bytes(),
        );
        site
    }

    /// L'adresse publique du site, avec le préfixe.
    fn origin(&self, req: &Request) -> String {
        if let Some(url) = &self.site_url {
            return url.clone();
        }
        let header = |name: &str| {
            req.header(name)
                .map(|v| v.split(',').next().unwrap_or("").trim().to_owned())
        };
        let host = header("x-forwarded-host")
            .or_else(|| header("host"))
            .unwrap_or_default();
        if host.is_empty()
            || !host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
        {
            return format!("http://localhost{}", base());
        }
        let proto = if header("x-forwarded-proto").as_deref() == Some("https") {
            "https"
        } else {
            "http"
        };
        format!("{proto}://{host}{}", base())
    }

    /// Une page HTML, avec sa Content-Security-Policy.
    fn html(&self, status: u16, html: String) -> Response {
        res::status(status)
            .header("content-security-policy", self.policy.as_str())
            .header("cache-control", "no-cache")
            .html(html)
    }
}

/// La langue d'une requête : celle choisie dans le sélecteur, sinon celle du
/// navigateur.
fn preferred_lang(req: &Request) -> Lang {
    req.cookie(LANG_COOKIE)
        .and_then(Lang::parse)
        .unwrap_or_else(|| pick_lang(req.header("accept-language").unwrap_or("")))
}

/// `/` : l'accueil de la langue choisie ou de celle du navigateur. (Le navigateur
/// garde l'ancre : `/#/docs/routing` de l'ancien site arrive sur l'accueil, qui
/// mène à la bonne page.)
fn root(_site: &Site, req: &Request) -> Response {
    let lang = preferred_lang(req);
    Redirect::to(link(&format!("/{}/", lang.code())))
        .into_response()
        .header("vary", "Accept-Language, Cookie")
        .header("cache-control", "no-cache")
}

/// Une page du site : `/{lang}/`, `/{lang}/docs/`, `/{lang}/docs/{page}/`.
fn page(site: &Site, req: &Request) -> Response {
    let path = req.path();
    let Some(route) = Route::parse(path) else {
        return not_found(site, req);
    };
    if route.page == PageKind::NotFound {
        return not_found(site, req);
    }
    if !path.ends_with('/') {
        let query = req
            .query_string()
            .map(|q| format!("?{q}"))
            .unwrap_or_default();
        return Redirect::permanent(format!("{}/{query}", link(path))).into_response();
    }
    // L'écran de chargement joue sur le premier accueil de la visite.
    let intro = route.page == PageKind::Home && (!site.live || req.cookie(INTRO_COOKIE).is_none());
    let html = pages::render(&pages::Ctx {
        site,
        route,
        origin: site.origin(req),
        intro,
    });
    let response = site.html(200, html);
    if intro && site.live {
        response.header(
            "set-cookie",
            format!("{INTRO_COOKIE}=1; Path={}/; SameSite=Lax", base()),
        )
    } else {
        response
    }
}

/// La page « introuvable », dans la langue de l'adresse ou du navigateur.
fn not_found(site: &Site, req: &Request) -> Response {
    let lang = Route::parse(req.path()).map_or_else(|| preferred_lang(req), |r| r.lang);
    let html = pages::render(&pages::Ctx {
        site,
        route: Route::new(lang, PageKind::NotFound),
        origin: site.origin(req),
        intro: false,
    });
    site.html(404, html)
}

/// `/offline/` : la page montrée hors ligne quand la page demandée n'est pas en cache.
fn offline(site: &Site, req: &Request) -> Response {
    site.html(200, pages::offline(site, &site.origin(req)))
}

/// `/search/{lang}.txt` : l'index de recherche d'une langue, une entrée par ligne
/// (voir [`crate::search::Entry`]). Le navigateur le télécharge une fois et cherche
/// dedans sans réseau.
fn search_index(site: &Site, req: &Request) -> Response {
    let file = req.param("file").unwrap_or("");
    let Some(lang) = file.strip_suffix(".txt").and_then(Lang::parse) else {
        return not_found(site, req);
    };
    let body: String = site
        .docs
        .search_index(lang)
        .iter()
        .map(|e| e.line())
        .collect();
    res::text(body).header("cache-control", "public, max-age=300")
}

fn sitemap(site: &Site, req: &Request) -> Response {
    let origin = site.origin(req);
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
    );
    for route in Route::all() {
        xml.push_str(&format!("<url><loc>{origin}{}</loc>", route.path()));
        for lang in Lang::ALL {
            xml.push_str(&format!(
                r#"<xhtml:link rel="alternate" hreflang="{}" href="{origin}{}"/>"#,
                lang.code(),
                route.with_lang(lang).path()
            ));
        }
        xml.push_str(&format!(
            r#"<xhtml:link rel="alternate" hreflang="x-default" href="{origin}{}"/></url>"#,
            route.with_lang(Lang::En).path()
        ));
        xml.push('\n');
    }
    xml.push_str("</urlset>\n");
    res::send(xml).header("content-type", "application/xml; charset=utf-8")
}

fn robots(site: &Site, req: &Request) -> Response {
    let origin = site.origin(req);
    res::text(format!(
        "User-agent: *\nAllow: /\nDisallow: {}/api/\n\nSitemap: {origin}/sitemap.xml\n",
        base()
    ))
}

/// `/manifest.webmanifest` (en anglais) et `/{lang}/manifest.webmanifest`.
fn manifest(_site: &Site, req: &Request) -> Response {
    let lang = req.param("lang").map_or(Some(Lang::En), Lang::parse);
    match lang {
        Some(lang) => res::send(pwa::manifest(lang).to_json())
            .header("content-type", "application/manifest+json")
            .header("cache-control", "no-cache"),
        None => res::send_status(404),
    }
}

/// `/sw.js` : le service worker, toujours revalidé pour que les mises à jour
/// arrivent vite.
fn worker(site: &Site, _req: &Request) -> Response {
    res::send(pwa::worker(site).to_js())
        .header("content-type", "text/javascript; charset=utf-8")
        .header("cache-control", "no-cache")
}

/// Route un gestionnaire qui n'a besoin que du site et de la requête.
fn route(app: &mut App, path: &str, site: &Arc<Site>, handler: fn(&Site, &Request) -> Response) {
    let site = site.clone();
    app.get(path, move |req: Request| {
        let site = site.clone();
        async move { handler(&site, &req) }
    });
}

/// Les fichiers dont l'adresse porte une empreinte (`?v=…`) ne changent jamais :
/// les navigateurs les gardent un an.
fn immutable(req: Request, next: Next) -> impl std::future::Future<Output = Response> + Send {
    let versioned = req.query_string().is_some_and(|q| q.starts_with("v="))
        && (req.path().starts_with("/assets/") || req.path().starts_with("/pkg/"));
    async move {
        let mut res = next.run(req).await;
        if versioned && res.status_code() == StatusCode::OK {
            res.set_header("cache-control", "public, max-age=31536000, immutable");
        }
        res
    }
}

/// Ajoute les routes du site à `app`.
pub fn configure(app: &mut App, site: Arc<Site>) {
    let dir = site_dir();
    app.middleware(middleware::helmet());
    app.middleware(immutable);
    route(app, "/", &site, root);
    for path in ["/:lang", "/:lang/docs", "/:lang/docs/:slug"] {
        route(app, path, &site, page);
    }
    route(app, "/offline", &site, offline);
    route(app, "/search/:file", &site, search_index);
    route(app, "/sitemap.xml", &site, sitemap);
    route(app, "/robots.txt", &site, robots);
    route(app, "/manifest.webmanifest", &site, manifest);
    route(app, "/:lang/manifest.webmanifest", &site, manifest);
    route(app, "/sw.js", &site, worker);
    // Réservé aux API : jamais mis en cache par le service worker.
    app.get("/api/health", |_| async { Json(json!({ "status": "ok" })) });
    let hour = Duration::from_secs(3600);
    app.serve_dir("/assets", ServeDir::new(dir.join("assets")).max_age(hour));
    app.serve_dir("/pkg", ServeDir::new(dir.join("pkg")).max_age(hour));
    // L'icône, à la racine comme le cherchent les navigateurs.
    let icon = dir.join("assets/favicon.svg");
    app.get("/favicon.svg", move |_| {
        let icon = icon.clone();
        async move { res::file(icon).await }
    });
    app.fallback(move |req: Request| {
        let site = site.clone();
        async move { not_found(&site, &req) }
    });
}

/// L'application, prête à servir ou à tester.
pub fn app(site: Arc<Site>) -> App {
    let mut app = App::new();
    configure(&mut app, site);
    app
}

/// `cargo run` : le site sur `PORT` (Heroku) ou 3000.
pub fn run() -> io::Result<()> {
    routes::set_base(&std::env::var("BASE_PATH").unwrap_or_default());
    let site = Arc::new(Site::load(true, std::env::var("SITE_URL").ok()));
    if !site.assets.has("/pkg/vitesse_site_bg.wasm") {
        eprintln!(
            "warning: site/pkg is missing: the pages will not be interactive (see the README)"
        );
    }
    for lang in Lang::ALL {
        println!(
            "   docs: {}/{} pages in {}",
            site.docs.count(lang),
            crate::catalog::PAGES.len(),
            lang.code()
        );
    }
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000u16);
    let mut app = App::new();
    app.middleware(middleware::logger());
    configure(&mut app, site);
    println!("⚡ Vitesse site on http://localhost:{port}{}/", base());
    app.run(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_change_with_the_content() {
        assert_eq!(version(b"a").len(), 10);
        assert_ne!(version(b"a"), version(b"b"));
    }
}
