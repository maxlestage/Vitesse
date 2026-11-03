//! A server shaped like a real application, for measuring what the
//! benchmark of `examples/bench.rs` leaves out: security headers, CORS and
//! compression on every response, a JSON list built by the handler, and a
//! stylesheet served from disk (`bench/assets`).
//!
//! ```sh
//! cargo run --release --example bench_app --features compress
//! wrk -c128 -d10s -s bench/lua/navigateur.lua http://127.0.0.1:3000/api/users
//! wrk -c128 -d10s -s bench/lua/navigateur.lua http://127.0.0.1:3000/static/app.css
//! ```
//!
//! Environment variables: `PORT`, `WORKERS`.

use vitesse::prelude::*;

#[derive(serde::Serialize)]
struct User {
    id: u32,
    name: String,
    email: String,
    active: bool,
    roles: Vec<&'static str>,
}

fn main() -> std::io::Result<()> {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let mut app = App::new();
    if let Some(n) = std::env::var("WORKERS").ok().and_then(|w| w.parse().ok()) {
        app.workers(n);
    }
    // `LOG=1`: every request logged on the standard output, like in production.
    if std::env::var("LOG").as_deref() == Ok("1") {
        app.middleware(middleware::logger());
    }
    app.middleware(middleware::helmet());
    app.middleware(middleware::cors());
    app.middleware(middleware::compress());
    app.get("/api/users", |_| async {
        let users: Vec<User> = (1..=50)
            .map(|id| User {
                id,
                name: format!("Utilisateur {id}"),
                email: format!("utilisateur{id}@exemple.fr"),
                active: id % 3 != 0,
                roles: if id % 10 == 0 {
                    vec!["admin", "editeur"]
                } else {
                    vec!["lecteur"]
                },
            })
            .collect();
        Json(users)
    });
    app.static_dir(
        "/static",
        concat!(env!("CARGO_MANIFEST_DIR"), "/bench/assets"),
    );
    app.run(port)
}
