//! `vitesse-site export <dossier>` : tout le site en fichiers statiques, pour GitHub
//! Pages ou n'importe quel hébergement statique. Chaque fichier vient des
//! gestionnaires du serveur, appelés en mémoire par le client de test de Vitesse
//! (`vitesse::test::TestClient`) : l'export et le serveur sont identiques.
//!
//! Ce qu'un hébergement statique ne sait pas faire est remplacé : `/` propose les
//! trois langues (et l'îlot `root` emmène le visiteur vers la sienne) au lieu de
//! lire `Accept-Language`, et `404.html` est la page « introuvable » en anglais.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vitesse::test::TestClient;

use super::{Site, app, pages, site_dir};
use crate::i18n::Lang;
use crate::routes::{Route, base, set_base};

/// L'adresse utilisée pour les URL canoniques et Open Graph sans `SITE_URL`.
fn default_site_url(base: &str) -> String {
    if base.is_empty() {
        "http://localhost:3000".to_owned()
    } else {
        format!("https://maxlestage.github.io{base}")
    }
}

/// Écrit le site dans `out`, liens derrière `base_path` (`""` ou `/Vitesse`), URL
/// absolues (canonique, plan du site, Open Graph) sur `site_url`. Renvoie le nombre
/// de fichiers écrits.
pub async fn export(out: &Path, base_path: &str, site_url: Option<String>) -> io::Result<usize> {
    set_base(base_path);
    let site_url = site_url
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| default_site_url(base()));
    let site = Arc::new(Site::load(false, Some(site_url)));
    if !site.assets.has("/pkg/vitesse_site_bg.wasm") {
        eprintln!(
            "warning: site/pkg is missing: the pages will not be interactive (see the README)"
        );
    }
    let origin = site.site_url.clone().unwrap_or_default();
    let client = TestClient::new(app(site.clone()));

    let mut written = 0;
    let mut write = |path: &str, bytes: &[u8]| -> io::Result<()> {
        let file = out.join(path.trim_start_matches('/'));
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file, bytes)?;
        written += 1;
        Ok(())
    };
    let get = |path: String, expected: u16| async move {
        let res = client.get(&path).await;
        if res.status().as_u16() != expected {
            return Err(io::Error::other(format!(
                "GET {path}: {} instead of {expected}",
                res.status()
            )));
        }
        Ok(res.bytes().to_vec())
    };

    for route in Route::all() {
        let html = get(route.path(), 200).await?;
        write(&format!("{}index.html", route.path()), &html)?;
    }
    write("index.html", pages::root(&site, &origin).as_bytes())?;
    write("404.html", &get("/en/404/".into(), 404).await?)?;
    write("offline/index.html", &get("/offline/".into(), 200).await?)?;
    let mut files: Vec<String> = Lang::ALL
        .iter()
        .flat_map(|l| {
            [
                format!("/search/{}.txt", l.code()),
                format!("/{}/manifest.webmanifest", l.code()),
            ]
        })
        .collect();
    files.extend(
        [
            "/manifest.webmanifest",
            "/sw.js",
            "/sitemap.xml",
            "/robots.txt",
            "/favicon.svg",
        ]
        .map(String::from),
    );
    for file in files {
        let body = get(file.clone(), 200).await?;
        write(&file, &body)?;
    }
    let dir = site_dir();
    for (folder, prefix) in [("assets", "assets"), ("pkg", "pkg")] {
        let root = dir.join(folder);
        for file in walk(&root) {
            let relative = file.strip_prefix(&root).map_err(io::Error::other)?;
            let url = format!("{prefix}/{}", relative.to_string_lossy().replace('\\', "/"));
            // Les fichiers annexes de wasm-bindgen, inutiles au navigateur.
            if url.ends_with(".gitignore") || url.ends_with("package.json") {
                continue;
            }
            write(&url, &std::fs::read(&file)?)?;
        }
    }
    // Les fichiers sont servis tels quels : pas de Jekyll sur GitHub Pages.
    write(".nojekyll", b"")?;
    Ok(written)
}

/// Tous les fichiers sous `dir`, récursivement.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path);
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_github_pages() {
        assert_eq!(
            default_site_url("/Vitesse"),
            "https://maxlestage.github.io/Vitesse"
        );
        assert_eq!(default_site_url(""), "http://localhost:3000");
    }
}
