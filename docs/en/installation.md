# Installation

This page walks you through installing Rust, creating a project and adding Vitesse to it. It also covers the optional crates you will probably want.

## Prerequisites

Vitesse needs **Rust 1.85 or later** because it uses the 2024 edition. Install Rust with [rustup](https://rustup.rs), the official toolchain installer. It manages Rust versions the way `nvm` manages Node.js versions.

On macOS, Linux or WSL:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

On Windows, download and run `rustup-init.exe` from [rustup.rs](https://rustup.rs). It offers to install the Visual Studio C++ build tools if they are missing. The [official installation page](https://www.rust-lang.org/tools/install) lists other options.

Then open a new terminal and check that everything is there:

```sh
rustc --version
cargo --version
```

If Rust was already installed, update it:

```sh
rustup update
```

`cargo` is Rust's build tool and package manager: like the package manager of a Node.js project, it handles dependencies and runs the project, and it also drives the compiler.

## Create a project

```sh
cargo new my-app
cd my-app
```

Cargo creates a minimal project:

```text
my-app/
├── Cargo.toml   # the manifest, like package.json
└── src/
    └── main.rs  # the entry point of your program
```

## Add Vitesse

```sh
cargo add vitesse
```

> [!NOTE]
> Vitesse is about to be published on [crates.io](https://crates.io). Until it is (or if you want the latest unreleased changes), install it from GitHub instead:
>
> `cargo add vitesse --git https://github.com/maxlestage/Vitesse`

`cargo add` writes the dependency into `Cargo.toml`. You can also add it by hand:

```toml
[dependencies]
vitesse = "0.1"
# or, straight from GitHub:
# vitesse = { git = "https://github.com/maxlestage/Vitesse" }
```

Vitesse pulls in `tokio`, `serde_json`, `http` and `bytes` and re-exports them, so for a simple app it is the only dependency you need.

## Optional dependencies

### serde, for typed JSON

To turn JSON bodies into your own structs (and your structs into JSON), add [serde](https://serde.rs) with its `derive` feature:

```sh
cargo add serde --features derive
```

```rust
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

#[derive(Serialize)]
struct User {
    id: u64,
    name: String,
}
```

You don't need `serde_json`. Vitesse re-exports it as `vitesse::serde_json`, and the `json!` macro is in the prelude. For JSON with no fixed shape, use `vitesse::serde_json::Value`.

### tokio, only when you need it

`app.run(3000)` creates and manages the tokio runtime for you, so most apps **don't** need `tokio` in their `Cargo.toml`. Add it when you want to:

- start the server inside your own runtime, with `#[tokio::main]` and `app.listen(3000).await` (see [Server configuration](server.md));
- write async tests with `#[tokio::test]` (see [Testing](testing.md));
- use tokio directly in your code: `tokio::spawn`, `tokio::time::sleep`, `tokio::fs` and so on.

```sh
cargo add tokio --features full
```

Vitesse also uses tokio 1.x, so Cargo compiles a single copy of it. Adding the dependency costs nothing extra.

> [!TIP]
> Vitesse re-exports tokio as `vitesse::tokio`, with the `rt`, `rt-multi-thread`, `macros`, `net`, `time`, `fs`, `io-util`, `signal` and `sync` features enabled. So `vitesse::tokio::time::sleep` works without the extra dependency, and so does the `#[tokio::main]` macro if you tell it where tokio lives (example below).

```rust
use vitesse::prelude::*;

#[vitesse::tokio::main(crate = "vitesse::tokio")]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "Hello World!" });
    app.listen(3000).await
}
```

## Recommended release profile

Add this to your `Cargo.toml`. It is the profile Vitesse's own benchmarks use:

```toml
[profile.release]
lto = "fat"         # optimise across crates (Vitesse + your code)
codegen-units = 1   # better optimisation, slower compilation
```

Then build or run with `--release`:

```sh
cargo run --release
cargo build --release   # binary in target/release/my-app
```

> [!IMPORTANT]
> Debug builds (`cargo run` without `--release`) compile faster but run much slower. They're fine while you develop. Always measure and deploy release builds.

> [!WARNING]
> Don't add `panic = "abort"`. Vitesse catches panics in handlers and turns them into `500` responses, so one buggy route can't take the server down. With `panic = "abort"`, any panic kills the whole process.

## Editor setup

Install [rust-analyzer](https://rust-analyzer.github.io), the official Rust language server. It gives you completion, inferred types, go-to-definition and errors as you type. In VS Code, install the **rust-analyzer** extension (`rust-lang.rust-analyzer`). Most other editors support it too: Zed, Neovim, Helix, Emacs and others. JetBrains users can use RustRover.

rustup also installs two tools worth using from day one:

```sh
cargo fmt      # formats your code, like Prettier
cargo clippy   # catches mistakes and suggests improvements, like ESLint
```

## Check your setup

Replace the contents of `src/main.rs` with:

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    println!("Listening on http://localhost:3000");
    app.run(3000)
}
```

Start it:

```sh
cargo run
```

Then, in another terminal:

```sh
curl http://localhost:3000
```

```text
Hello World!
```

That's it. Stop the server with `Ctrl+C`, then continue with [Your first app](first-app.md).

> [!TIP]
> If you get `Address already in use`, another program is already using port 3000. Pick another port, such as `app.run(8080)`.
