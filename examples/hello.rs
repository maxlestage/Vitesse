//! Le « Hello World » d'Express, version Vitesse.
//!
//! ```sh
//! cargo run --release --example hello
//! curl http://localhost:3000
//! ```

use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    println!("⚡ Vitesse écoute sur http://localhost:3000");
    app.run(3000)
}
