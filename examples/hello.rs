//! Express's "Hello World", Vitesse-style.
//!
//! ```sh
//! cargo run --release --example hello
//! curl http://localhost:3000
//! ```

use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    println!("⚡ Vitesse listening on http://localhost:3000");
    app.run(3000)
}
