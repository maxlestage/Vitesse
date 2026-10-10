//! Tout le site à travers le client de test de Vitesse : chaque page, ses balises
//! SEO et sa Content-Security-Policy, les redirections, le manifeste et le service
//! worker de la PWA, l'index de recherche, le plan du site et les fichiers statiques.

use std::sync::{Arc, OnceLock};

use vitesse::serde_json::{self, Value};
use vitesse::test::TestClient;
use vitesse_site::catalog::PAGES;
use vitesse_site::i18n::Lang;
use vitesse_site::routes::{Page, Route};
use vitesse_site::search::Entry;
use vitesse_site::server::{Site, app, csp};

const ORIGIN: &str = "https://example.org";

fn site() -> Arc<Site> {
    static SITE: OnceLock<Arc<Site>> = OnceLock::new();
    SITE.get_or_init(|| Arc::new(Site::load(true, Some(ORIGIN.into()))))
        .clone()
}

fn client() -> TestClient {
    TestClient::new(app(site()))
}

/// Le contenu des `<script>` et `<style>` en ligne qui s'exécutent (pas les
/// données JSON-LD).
fn inline(html: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut found = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start..];
        let Some(gt) = after.find('>') else { break };
        let attrs = &after[..gt];
        let Some(end) = after.find(&close) else { break };
        if !attrs.contains("src=") && !attrs.contains("application/ld+json") {
            found.push(after[gt + 1..end].to_owned());
        }
        rest = &after[end + close.len()..];
    }
    found
}

#[tokio::test]
async fn every_page_renders_with_its_seo_and_policy() {
    let client = client();
    for route in Route::all() {
        let path = route.path();
        let res = client.get(&path).await;
        assert_eq!(res.status(), 200, "{path}");
        assert_eq!(
            res.header("content-type"),
            Some("text/html; charset=utf-8"),
            "{path}"
        );
        let html = res.text();
        let code = route.lang.code();
        assert!(
            html.starts_with(&format!("<!DOCTYPE html><html lang=\"{code}\"")),
            "{path}"
        );
        assert_eq!(html.matches("<title>").count(), 1, "{path}");
        assert!(
            html.contains(&format!(r#"<link rel="canonical" href="{ORIGIN}{path}"/>"#)),
            "{path}"
        );
        for lang in Lang::ALL {
            let other = route.with_lang(lang).path();
            assert!(
                html.contains(&format!(
                    r#"<link rel="alternate" hreflang="{}" href="{ORIGIN}{other}"/>"#,
                    lang.code()
                )),
                "{path} → {other}"
            );
        }
        assert!(html.contains(r#"hreflang="x-default""#), "{path}");
        assert!(
            html.contains(r#"<meta name="description" content=""#),
            "{path}"
        );
        assert!(
            html.contains(
                r#"property="og:image" content="https://example.org/assets/og-image.png""#
            )
        );
        // PWA : le manifeste de la langue, la couleur du thème, le service worker.
        assert!(
            html.contains(&format!(
                r#"<link rel="manifest" href="/{code}/manifest.webmanifest"/>"#
            )),
            "{path}"
        );
        assert!(html.contains(r##"<meta name="theme-color" content="#07070b"/>"##));
        assert!(html.contains(r#"navigator.serviceWorker.register("/sw.js")"#));
        // La politique autorise exactement les scripts et styles en ligne de la page.
        let policy = res.header("content-security-policy").expect("CSP");
        let scripts = inline(&html, "script");
        assert_eq!(scripts.len(), 2, "{path}: {scripts:?}");
        for script in scripts {
            assert!(policy.contains(&csp::source(&script)), "{path}: {script}");
        }
        for style in inline(&html, "style") {
            assert!(policy.contains(&csp::source(&style)), "{path}: {style}");
        }
        assert!(policy.contains("worker-src 'self'") && policy.contains("manifest-src 'self'"));
        // Les en-têtes de `middleware::helmet()`.
        assert_eq!(res.header("x-content-type-options"), Some("nosniff"));
        // Le pied de page et son îlot.
        assert!(html.contains(r#"data-island="clock""#), "{path}");
    }
}

#[tokio::test]
async fn the_home_page_has_every_section_and_island() {
    let html = client().get("/fr/").await.text();
    for id in [
        "id=\"top\"",
        "class=\"marquee\"",
        "id=\"chiffres\"",
        "id=\"performances\"",
        "id=\"fonctionnalites\"",
        "id=\"journey\"",
        "id=\"express\"",
        "id=\"demarrer\"",
        "class=\"cta\"",
        "class=\"footer\"",
    ] {
        assert!(html.contains(id), "{id}");
    }
    for island in ["warp", "bench", "compare", "clock"] {
        assert!(
            html.contains(&format!(r#"data-island="{island}" data-props="#)),
            "{island}"
        );
    }
    assert!(html.contains("Le confort d&#x27;Express."));
    assert!(html.contains(r#""@type":"SoftwareSourceCode""#));
    // L'écran de chargement joue la première fois, puis plus jamais de la visite.
    assert!(html.contains("class=\"loader\""));
    let again = client()
        .get("/fr/")
        .header("cookie", "vitesse-intro=1")
        .await
        .text();
    assert!(!again.contains("class=\"loader\""));
    assert!(!again.contains("<html lang=\"fr\" class=\"with-loader\""));
}

#[tokio::test]
async fn doc_pages_have_their_navigation() {
    let html = client().get("/es/docs/routing/").await.text();
    assert!(html.contains("<h1 class=\"doc-title\">"));
    assert!(html.contains(r#"class="doc-toc""#));
    assert!(html.contains(r#"data-island="search" data-props="es""#));
    // Précédent, suivant, et le lien « modifier » vers GitHub.
    assert!(html.contains(r#"href="/es/docs/first-app/" rel="prev""#));
    assert!(html.contains(r#"href="/es/docs/requests/" rel="next""#));
    assert!(html.contains("/edit/master/docs/es/routing.md"));
    assert!(html.contains(r#""@type":"BreadcrumbList""#));
    assert!(html.contains(r#"<a class="is-active" href="/es/docs/routing/" aria-current="page">"#));
    let hub = client().get("/en/docs/").await.text();
    assert!(hub.contains(r#"data-props="en large""#));
    for page in PAGES {
        assert!(
            hub.contains(&format!(r#"href="/en/docs/{}/""#, page.slug)),
            "{}",
            page.slug
        );
    }
}

#[tokio::test]
async fn redirects_to_the_right_language_and_address() {
    let client = client();
    let res = client
        .get("/")
        .header("accept-language", "es-ES,es;q=0.9")
        .await;
    assert_eq!(res.status(), 302);
    assert_eq!(res.header("location"), Some("/es/"));
    // La langue choisie dans le sélecteur passe avant celle du navigateur.
    let res = client
        .get("/")
        .header("accept-language", "es")
        .header("cookie", "vitesse-lang=fr")
        .await;
    assert_eq!(res.header("location"), Some("/fr/"));
    assert_eq!(client.get("/").await.header("location"), Some("/en/"));
    let res = client.get("/fr/docs/routing?x=1").await;
    assert_eq!(res.status(), 301);
    assert_eq!(res.header("location"), Some("/fr/docs/routing/?x=1"));
}

#[tokio::test]
async fn unknown_pages_are_not_found_in_their_language() {
    let client = client();
    let res = client.get("/fr/docs/nope/").await;
    assert_eq!(res.status(), 404);
    let html = res.text();
    assert!(html.contains("<html lang=\"fr\""));
    assert!(html.contains("Page introuvable"));
    assert!(html.contains(r#"content="noindex"#));
    let res = client.get("/nope").header("accept-language", "es").await;
    assert_eq!(res.status(), 404);
    assert!(res.text().contains("<html lang=\"es\""));
}

#[tokio::test]
async fn the_manifests_make_the_site_installable() {
    let client = client();
    for (path, lang) in [
        ("/manifest.webmanifest", "en"),
        ("/en/manifest.webmanifest", "en"),
        ("/fr/manifest.webmanifest", "fr"),
        ("/es/manifest.webmanifest", "es"),
    ] {
        let res = client.get(path).await;
        assert_eq!(res.status(), 200, "{path}");
        assert_eq!(
            res.header("content-type"),
            Some("application/manifest+json")
        );
        let manifest: Value = serde_json::from_slice(res.bytes()).expect("valid JSON");
        assert_eq!(manifest["name"], "Vitesse");
        assert_eq!(manifest["short_name"], "Vitesse");
        assert_eq!(manifest["display"], "standalone");
        assert_eq!(manifest["theme_color"], "#07070b");
        assert_eq!(manifest["background_color"], "#07070b");
        assert_eq!(manifest["lang"], lang);
        assert_eq!(manifest["id"], "/");
        assert_eq!(manifest["scope"], "/");
        assert_eq!(manifest["start_url"], format!("/{lang}/"));
        let icons = manifest["icons"].as_array().expect("icons");
        let sizes: Vec<&str> = icons.iter().filter_map(|i| i["sizes"].as_str()).collect();
        assert!(sizes.contains(&"192x192") && sizes.contains(&"512x512"));
        assert!(icons.iter().any(|i| i["purpose"] == "maskable"));
        for icon in icons {
            let src = icon["src"].as_str().unwrap();
            let res = client.get(src).await;
            assert_eq!(res.status(), 200, "{src}");
            assert_eq!(res.header("content-type"), icon["type"].as_str(), "{src}");
        }
    }
}

#[tokio::test]
async fn the_service_worker_precaches_the_site() {
    let site = site();
    let res = client().get("/sw.js").await;
    assert_eq!(res.status(), 200);
    assert_eq!(
        res.header("content-type"),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(res.header("cache-control"), Some("no-cache"));
    let js = res.text();
    assert!(js.contains(&format!(
        r#"const CACHE = "vitesse-site-{}";"#,
        site.version
    )));
    for url in [
        "\"/en/\"",
        "\"/fr/\"",
        "\"/es/\"",
        "\"/fr/docs/\"",
        "\"/offline/\"",
        "\"/assets/icon-192.png\"",
        "\"/assets/fonts/inter-latin.woff2\"",
    ] {
        assert!(js.contains(url), "{url}");
    }
    assert!(js.contains(&format!("\"{}\"", site.assets.url("/assets/main.css"))));
    assert!(js.contains(r#"const OFFLINE = "/offline/";"#));
    assert!(js.contains(r#"const NETWORK_ONLY = ["/api/"];"#));
    // Hors ligne, la page de secours répond, dans les trois langues.
    let offline = client().get("/offline/").await;
    assert_eq!(offline.status(), 200);
    let html = offline.text();
    assert!(
        html.contains("You are offline")
            && html.contains("Vous êtes hors ligne")
            && html.contains("Sin conexión")
    );
}

#[tokio::test]
async fn the_search_index_lists_pages_and_sections() {
    let client = client();
    for lang in Lang::ALL {
        let res = client.get(&format!("/search/{}.txt", lang.code())).await;
        assert_eq!(res.status(), 200);
        let entries: Vec<Entry> = res.text().lines().filter_map(Entry::parse).collect();
        assert_eq!(entries.iter().filter(|e| e.page).count(), PAGES.len());
        assert!(entries.len() > PAGES.len() * 4);
        assert!(
            entries
                .iter()
                .all(|e| e.href.starts_with(&format!("/{}/docs/", lang.code())))
        );
    }
    assert_eq!(client.get("/search/de.txt").await.status(), 404);
}

#[tokio::test]
async fn sitemap_robots_files_and_api() {
    let client = client();
    let sitemap = client.get("/sitemap.xml").await.text();
    assert_eq!(sitemap.matches("<url>").count(), Route::all().len());
    assert!(sitemap.contains("<loc>https://example.org/fr/docs/routing/</loc>"));
    let robots = client.get("/robots.txt").await.text();
    assert!(robots.contains("Sitemap: https://example.org/sitemap.xml"));
    assert!(robots.contains("Disallow: /api/"));
    for (path, kind) in [
        ("/assets/main.css", "text/css; charset=utf-8"),
        ("/assets/favicon.svg", "image/svg+xml"),
        ("/favicon.svg", "image/svg+xml"),
        ("/assets/fonts/space-grotesk-latin.woff2", "font/woff2"),
        ("/assets/og-image.png", "image/png"),
    ] {
        let res = client.get(path).await;
        assert_eq!(res.status(), 200, "{path}");
        assert_eq!(res.header("content-type"), Some(kind), "{path}");
    }
    // Les fichiers à empreinte sont gardés un an.
    let css = site().assets.url("/assets/main.css");
    let res = client.get(&css).await;
    assert_eq!(
        res.header("cache-control"),
        Some("public, max-age=31536000, immutable")
    );
    assert_eq!(client.get("/assets/../Cargo.toml").await.status(), 404);
    let api = client.get("/api/health").await;
    assert_eq!(api.status(), 200);
    assert_eq!(api.text(), r#"{"status":"ok"}"#);
}

#[test]
fn routes_cover_every_page() {
    assert_eq!(
        Route::all()
            .iter()
            .filter(|r| matches!(r.page, Page::Doc(_)))
            .count(),
        3 * PAGES.len()
    );
}
