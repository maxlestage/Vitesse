# Installation

Cette page vous guide pour installer Rust, créer un projet et y ajouter Vitesse. Elle présente aussi les crates optionnelles dont vous aurez probablement besoin.

## Prérequis

Vitesse demande **Rust 1.85 ou plus récent**, car il utilise l'édition 2024. Installez Rust avec [rustup](https://rustup.rs), l'installateur officiel. Il gère les versions de Rust comme `nvm` gère celles de Node.js.

Sous macOS, Linux ou WSL :

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Sous Windows, téléchargez et lancez `rustup-init.exe` depuis [rustup.rs](https://rustup.rs). Il propose d'installer les outils de compilation C++ de Visual Studio s'ils manquent. La [page d'installation officielle](https://www.rust-lang.org/tools/install) liste les autres possibilités.

Ouvrez ensuite un nouveau terminal et vérifiez que tout est là :

```sh
rustc --version
cargo --version
```

Si Rust était déjà installé, mettez-le à jour :

```sh
rustup update
```

`cargo` est l'outil de build et le gestionnaire de paquets de Rust : comme le gestionnaire de paquets d'un projet Node.js, il gère les dépendances et lance le projet, et il pilote aussi le compilateur.

## Créer un projet

```sh
cargo new my-app
cd my-app
```

Cargo crée un projet minimal :

```text
my-app/
├── Cargo.toml   # le manifeste, comme package.json
└── src/
    └── main.rs  # le point d'entrée du programme
```

## Ajouter Vitesse

```sh
cargo add vitesse --git https://github.com/maxlestage/Vitesse
```

`cargo add` inscrit la dépendance dans `Cargo.toml`. Vous pouvez aussi l'ajouter à la main :

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse" }
```

Vitesse embarque `tokio`, `serde_json`, `http` et `bytes`, et les réexporte. Pour une application simple, c'est donc la seule dépendance nécessaire.

## Les features Cargo

Certaines parties de Vitesse sont des features Cargo, que vous activez ou désactivez dans `Cargo.toml`. Seule `ws` est activée par défaut : les autres ajoutent des dépendances et du temps de compilation, et vous ne payez ainsi que ce que vous utilisez.

| Feature | Par défaut | Apporte |
|---|---|---|
| `ws` | Activée | `app.ws(...)` et le module `vitesse::ws` : voir [WebSocket](websocket.md) |
| `tls` | Désactivée | `app.tls(...)` et le module `vitesse::tls` : HTTPS (TLS 1.2 et 1.3, avec rustls), voir [HTTPS et HTTP/2](https.md) |
| `http2` | Désactivée | HTTP/2 : négocié par les navigateurs sur TLS (avec `tls`), et en clair pour les clients qui commencent en HTTP/2, voir [HTTPS et HTTP/2](https.md) |
| `http3` | Désactivée | `app.http3(...)` et le module `vitesse::http3` : HTTP/3 sur QUIC, voir [HTTP/3 et QUIC](http3.md) |
| `multipart` | Désactivée | `req.multipart()` et le module `vitesse::multipart` : envois de fichiers (`multipart/form-data`), voir [Envois de fichiers](requests.md#envois-de-fichiers-multipart) |
| `compress` | Désactivée | `middleware::compress()` (réponses en Brotli ou gzip) et `middleware::decompress()` (corps de requête compressés), voir [`compress`](middleware.md#compress) |
| `session` | Désactivée | `app.cookie_secret(...)`, les cookies signés (`req.sign_cookie`, `req.signed_cookie`), `middleware::session(...)` et le module `vitesse::session`, avec les crates hmac, sha2 et getrandom : voir [Cookies signés et sessions](sessions.md) |
| `templates` | Désactivée | `app.views(...)`, `app.templates(...)`, `req.render(...)` et le module `vitesse::templates`, avec [minijinja](https://crates.io/crates/minijinja) : voir [Gabarits HTML](templates.md) |
| `tower` | Désactivée | `middleware::layer(...)`, `app.into_service()` et le module `vitesse::tower` : les middlewares de tower et de tower-http, voir [Couches tower](middleware.md#couches-tower) |

Tout le reste, y compris les Server-Sent Events (`vitesse::sse`) et les autres middlewares fournis, est toujours disponible.

```toml
[dependencies]
# HTTPS et HTTP/2 servis par Vitesse lui-même, en plus des features par défaut :
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tls", "http2"] }
# Un site avec envois de fichiers et compression, derrière un reverse proxy :
# vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["multipart", "compress"] }
# Des pages rendues sur le serveur, avec connexion des utilisateurs :
# vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["templates", "session"] }
# Sans WebSocket, pour compiler un peu moins :
# vitesse = { git = "https://github.com/maxlestage/Vitesse", default-features = false }
```

Avec `cargo add` : `cargo add vitesse --git https://github.com/maxlestage/Vitesse --features tls,http2`, ou `cargo add vitesse --git https://github.com/maxlestage/Vitesse --no-default-features`.

## Dépendances optionnelles

### serde, pour du JSON typé

Pour transformer des corps JSON en structures à vous (et vos structures en JSON), ajoutez [serde](https://serde.rs) avec sa fonctionnalité `derive` :

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

Inutile d'ajouter `serde_json`. Vitesse le réexporte sous le nom `vitesse::serde_json`, et la macro `json!` est dans le prélude. Pour du JSON sans forme fixe, utilisez `vitesse::serde_json::Value`.

### tokio, seulement si vous en avez besoin

`app.run(3000)` crée et gère le runtime tokio pour vous, donc la plupart des applications n'ont **pas** besoin de `tokio` dans leur `Cargo.toml`. Ajoutez-le si vous voulez :

- démarrer le serveur dans votre propre runtime, avec `#[tokio::main]` et `app.listen(3000).await` (voir [Configuration du serveur](server.md)) ;
- écrire des tests asynchrones avec `#[tokio::test]` (voir [Tests](testing.md)) ;
- utiliser tokio directement dans votre code : `tokio::spawn`, `tokio::time::sleep`, `tokio::fs`, etc.

```sh
cargo add tokio --features full
```

Vitesse utilise lui aussi tokio 1.x, donc Cargo n'en compile qu'une seule copie. Ajouter la dépendance ne coûte rien de plus.

> [!TIP]
> Vitesse réexporte tokio sous le nom `vitesse::tokio`, avec les fonctionnalités `rt`, `rt-multi-thread`, `macros`, `net`, `time`, `fs`, `io-util`, `signal` et `sync` activées. `vitesse::tokio::time::sleep` marche donc sans dépendance supplémentaire, tout comme la macro `#[tokio::main]` si vous lui indiquez où se trouve tokio (exemple ci-dessous).

```rust
use vitesse::prelude::*;

#[vitesse::tokio::main(crate = "vitesse::tokio")]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "Hello World!" });
    app.listen(3000).await
}
```

## Profil release recommandé

Ajoutez ceci à votre `Cargo.toml`. C'est le profil utilisé par les benchmarks de Vitesse :

```toml
[profile.release]
lto = "fat"         # optimise entre les crates (Vitesse + votre code)
codegen-units = 1   # meilleure optimisation, compilation plus lente
```

Puis compilez ou lancez avec `--release` :

```sh
cargo run --release
cargo build --release   # binaire dans target/release/my-app
```

> [!IMPORTANT]
> Les builds de debug (`cargo run` sans `--release`) compilent plus vite mais s'exécutent beaucoup plus lentement. Ils conviennent pendant le développement. Mesurez et déployez toujours des builds release.

> [!WARNING]
> N'ajoutez pas `panic = "abort"`. Vitesse rattrape les paniques des handlers et les transforme en réponses `500`, pour qu'une route boguée ne fasse pas tomber le serveur. Avec `panic = "abort"`, la moindre panique arrête tout le processus.

## Configurer son éditeur

Installez [rust-analyzer](https://rust-analyzer.github.io), le serveur de langage officiel de Rust. Il vous donne la complétion, les types inférés, le « aller à la définition » et les erreurs au fil de la frappe. Dans VS Code, installez l'extension **rust-analyzer** (`rust-lang.rust-analyzer`). La plupart des autres éditeurs le prennent aussi en charge : Zed, Neovim, Helix, Emacs, etc. Chez JetBrains, utilisez RustRover.

rustup installe aussi deux outils à utiliser dès le premier jour :

```sh
cargo fmt      # formate votre code, comme Prettier
cargo clippy   # repère les erreurs et suggère des améliorations, comme ESLint
```

## Vérifier l'installation

Remplacez le contenu de `src/main.rs` par :

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    println!("Écoute sur http://localhost:3000");
    app.run(3000)
}
```

Lancez-le :

```sh
cargo run
```

Puis, dans un autre terminal :

```sh
curl http://localhost:3000
```

```text
Hello World!
```

C'est tout. Arrêtez le serveur avec `Ctrl+C`, puis passez à [Votre première application](first-app.md).

> [!TIP]
> Si vous obtenez `Address already in use`, un autre programme utilise déjà le port 3000. Choisissez-en un autre, par exemple `app.run(8080)`.
