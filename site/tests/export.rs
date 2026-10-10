//! L'export statique derrière le préfixe de GitHub Pages (`/Vitesse`) : chaque page,
//! la racine, la page 404, le manifeste et le service worker, avec le préfixe dans
//! chaque adresse. (Un fichier de test à part : le préfixe se fixe une fois par
//! programme.)

use std::path::Path;

use vitesse_site::catalog::PAGES;
use vitesse_site::server::export::export;

#[tokio::test]
async fn exports_every_page_behind_the_base_path() {
    let out = std::env::temp_dir().join(format!("vitesse-site-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let files = export(
        &out,
        "/Vitesse",
        Some("https://maxlestage.github.io/Vitesse".into()),
    )
    .await
    .expect("export");
    assert!(files > 3 * (2 + PAGES.len()));
    let read = |path: &str| {
        std::fs::read_to_string(out.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
    };
    for lang in ["en", "fr", "es"] {
        let home = read(&format!("{lang}/index.html"));
        assert!(home.contains(&format!("<html lang=\"{lang}\"")));
        assert!(home.contains(r#"data-base="/Vitesse""#));
        assert!(home.contains(&format!(
            r#"<link rel="canonical" href="https://maxlestage.github.io/Vitesse/{lang}/"/>"#
        )));
        assert!(home.contains(&format!(
            r#"<link rel="manifest" href="/Vitesse/{lang}/manifest.webmanifest"/>"#
        )));
        assert!(home.contains(r#"navigator.serviceWorker.register("/Vitesse/sw.js")"#));
        assert!(home.contains(r#"href="/Vitesse/assets/main.css?v="#));
        // Aucun lien du site sans le préfixe.
        for unprefixed in [
            "href=\"/en/",
            "href=\"/fr/",
            "href=\"/es/",
            "href=\"/assets/",
            "\"/pkg/",
        ] {
            assert!(!home.contains(unprefixed), "{lang}: {unprefixed}");
        }
        for page in PAGES {
            let html = read(&format!("{lang}/docs/{}/index.html", page.slug));
            assert!(
                html.contains("<h1 class=\"doc-title\">"),
                "{lang}/{}",
                page.slug
            );
            assert!(!html.contains("href=\"/en/docs/"), "{lang}/{}", page.slug);
        }
        let manifest = read(&format!("{lang}/manifest.webmanifest"));
        assert!(manifest.contains(&format!(r#""start_url":"/Vitesse/{lang}/""#)));
        assert!(manifest.contains(r#""scope":"/Vitesse/""#));
        assert!(manifest.contains(r#""src":"/Vitesse/assets/icon-512.png""#));
        let index = read(&format!("search/{lang}.txt"));
        assert!(index.contains(&format!("\t/Vitesse/{lang}/docs/routing/\t")));
    }
    let root = read("index.html");
    assert!(root.contains(r#"data-island="root""#));
    assert!(root.contains(r#"href="/Vitesse/fr/""#));
    assert!(read("404.html").contains("Page not found"));
    let worker = read("sw.js");
    assert!(worker.contains(r#""/Vitesse/en/""#));
    assert!(worker.contains(r#"const OFFLINE = "/Vitesse/offline/";"#));
    assert!(worker.contains(r#"const NETWORK_ONLY = ["/Vitesse/api/"];"#));
    assert!(worker.contains("\"/Vitesse/assets/main.css?v="));
    assert!(read("offline/index.html").contains("Vous êtes hors ligne"));
    assert!(read("manifest.webmanifest").contains(r#""start_url":"/Vitesse/en/""#));
    assert!(
        read("sitemap.xml")
            .contains("<loc>https://maxlestage.github.io/Vitesse/fr/docs/routing/</loc>")
    );
    assert!(
        read("robots.txt").contains("Sitemap: https://maxlestage.github.io/Vitesse/sitemap.xml")
    );
    for file in [
        ".nojekyll",
        "favicon.svg",
        "assets/favicon.svg",
        "assets/main.css",
        "assets/icon-192.png",
        "assets/icon-maskable-512.png",
        "assets/fonts/inter-latin.woff2",
    ] {
        assert!(Path::new(&out.join(file)).exists(), "{file}");
    }
    let _ = std::fs::remove_dir_all(&out);
}
