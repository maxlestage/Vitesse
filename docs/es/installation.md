# Instalación

Esta página te guía para instalar Rust, crear un proyecto y agregarle Vitesse. También presenta los crates opcionales que probablemente vas a querer.

## Requisitos previos

Vitesse necesita **Rust 1.85 o posterior**, porque usa la edición 2024. Instala Rust con [rustup](https://rustup.rs), el instalador oficial. Gestiona las versiones de Rust igual que `nvm` gestiona las de Node.js.

En macOS, Linux o WSL:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

En Windows, descarga y ejecuta `rustup-init.exe` desde [rustup.rs](https://rustup.rs). Si te faltan las herramientas de compilación C++ de Visual Studio, te ofrecerá instalarlas. La [página oficial de instalación](https://www.rust-lang.org/tools/install) enumera otras opciones.

Después abre una terminal nueva y comprueba que todo esté en su lugar:

```sh
rustc --version
cargo --version
```

Si ya tenías Rust instalado, actualízalo:

```sh
rustup update
```

`cargo` es la herramienta de compilación y el gestor de paquetes de Rust. Hace el trabajo de `npm` (dependencias, ejecutar el proyecto) y además controla el compilador.

## Crear un proyecto

```sh
cargo new my-app
cd my-app
```

Cargo crea un proyecto mínimo:

```text
my-app/
├── Cargo.toml   # el manifiesto, como package.json
└── src/
    └── main.rs  # el punto de entrada del programa
```

## Agregar Vitesse

```sh
cargo add vitesse
```

> [!NOTE]
> Vitesse está a punto de publicarse en [crates.io](https://crates.io). Mientras tanto (o si quieres los cambios más recientes sin publicar), instálalo desde GitHub:
>
> `cargo add vitesse --git https://github.com/maxlestage/Vitesse`

`cargo add` escribe la dependencia en `Cargo.toml`. También puedes agregarla a mano:

```toml
[dependencies]
vitesse = "0.1"
# o, directamente desde GitHub:
# vitesse = { git = "https://github.com/maxlestage/Vitesse" }
```

Vitesse trae consigo `tokio`, `serde_json`, `http` y `bytes`, y los reexporta. Para una aplicación sencilla, es la única dependencia que necesitas.

## Dependencias opcionales

### serde, para JSON tipado

Para convertir cuerpos JSON en tus propias estructuras (y tus estructuras en JSON), agrega [serde](https://serde.rs) con su característica `derive`:

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

No hace falta agregar `serde_json`. Vitesse lo reexporta como `vitesse::serde_json`, y la macro `json!` está en el preludio. Para JSON sin forma fija, usa `vitesse::serde_json::Value`.

### tokio, solo si lo necesitas

`app.run(3000)` crea y gestiona el runtime de tokio por ti, así que la mayoría de las aplicaciones **no** necesitan `tokio` en su `Cargo.toml`. Agrégalo si quieres:

- arrancar el servidor dentro de tu propio runtime, con `#[tokio::main]` y `app.listen(3000).await` (consulta [Configuración del servidor](server.md));
- escribir pruebas asíncronas con `#[tokio::test]` (consulta [Pruebas](testing.md));
- usar tokio directamente en tu código: `tokio::spawn`, `tokio::time::sleep`, `tokio::fs`, etc.

```sh
cargo add tokio --features full
```

Vitesse también usa tokio 1.x, así que Cargo compila una sola copia. Agregar la dependencia no cuesta nada extra.

> [!TIP]
> Vitesse reexporta tokio como `vitesse::tokio`, con las características `rt`, `rt-multi-thread`, `macros`, `net`, `time`, `fs`, `io-util`, `signal` y `sync` activadas. Así que `vitesse::tokio::time::sleep` funciona sin la dependencia adicional, y también la macro `#[tokio::main]` si le indicas dónde está tokio (ejemplo abajo).

```rust
use vitesse::prelude::*;

#[vitesse::tokio::main(crate = "vitesse::tokio")]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "Hello World!" });
    app.listen(3000).await
}
```

## Perfil de release recomendado

Agrega esto a tu `Cargo.toml`. Es el perfil que usan los benchmarks de Vitesse:

```toml
[profile.release]
lto = "fat"         # optimiza entre crates (Vitesse + tu código)
codegen-units = 1   # mejor optimización, compilación más lenta
```

Luego compila o ejecuta con `--release`:

```sh
cargo run --release
cargo build --release   # binario en target/release/my-app
```

> [!IMPORTANT]
> Las compilaciones de depuración (`cargo run` sin `--release`) compilan más rápido pero se ejecutan mucho más lento. Sirven mientras desarrollas. Mide y despliega siempre compilaciones release.

> [!WARNING]
> No agregues `panic = "abort"`. Vitesse atrapa los pánicos de los handlers y los convierte en respuestas `500`, para que una ruta con un error no tumbe el servidor. Con `panic = "abort"`, cualquier pánico termina todo el proceso.

## Configurar tu editor

Instala [rust-analyzer](https://rust-analyzer.github.io), el servidor de lenguaje oficial de Rust. Te da autocompletado, tipos inferidos, «ir a la definición» y errores mientras escribes. En VS Code, instala la extensión **rust-analyzer** (`rust-lang.rust-analyzer`). La mayoría de los demás editores también lo soportan: Zed, Neovim, Helix, Emacs y otros. Si usas JetBrains, está RustRover.

rustup instala además dos herramientas que conviene usar desde el primer día:

```sh
cargo fmt      # formatea tu código, como Prettier
cargo clippy   # detecta errores y sugiere mejoras, como ESLint
```

## Comprobar la instalación

Reemplaza el contenido de `src/main.rs` por:

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    println!("Escuchando en http://localhost:3000");
    app.run(3000)
}
```

Arráncalo:

```sh
cargo run
```

Luego, en otra terminal:

```sh
curl http://localhost:3000
```

```text
Hello World!
```

Listo. Detén el servidor con `Ctrl+C` y sigue con [Tu primera aplicación](first-app.md).

> [!TIP]
> Si obtienes `Address already in use`, otro programa ya está usando el puerto 3000. Elige otro, por ejemplo `app.run(8080)`.
