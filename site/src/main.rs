//! `cargo run` : le site, servi par Vitesse sur `http://localhost:3000` (`PORT`
//! pour changer de port). Construire d'abord le code du navigateur (voir le
//! README du dépôt).
//!
//! `cargo run -- export dist` : tout le site en fichiers statiques dans `dist/`,
//! pour GitHub Pages. `BASE_PATH` (`/Vitesse`) préfixe chaque lien, `SITE_URL`
//! (`https://maxlestage.github.io/Vitesse`) donne les URL absolues.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("export") => {
            let dir = args.next().unwrap_or_else(|| "dist".into());
            let base = std::env::var("BASE_PATH").unwrap_or_default();
            let site_url = std::env::var("SITE_URL").ok();
            let runtime = vitesse::tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let files = runtime.block_on(vitesse_site::server::export::export(
                std::path::Path::new(&dir),
                &base,
                site_url,
            ))?;
            println!("📦 {files} files written to {dir}");
            Ok(())
        }
        Some(other) => {
            eprintln!("unknown command {other:?}: run without arguments, or `export <dir>`");
            std::process::exit(2);
        }
        None => vitesse_site::server::run(),
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}
