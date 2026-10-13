#!/usr/bin/env bash
# Builds the demo app (an example of examples/), as the Dockerfile does: run on
# Heroku by RustConfig once the emk/rust buildpack has installed Rust. Works by
# hand too, from the repository: ./heroku-build.sh. The Procfile starts the binary.
# To deploy another example, change `example` (the file name without .rs) and
# the Procfile.
set -euo pipefail
example=demo
cd "$(dirname "$0")"
# On Heroku, Cargo builds into the buildpack's cache (CARGO_TARGET_DIR).
target="${CARGO_TARGET_DIR:-target}"

echo "-----> Building the $example example"
cargo build --release --example "$example"
mkdir -p target/release
if [ "$target" != target ]; then
  cp "$target/release/examples/$example" target/release/
fi
echo "       target/release/$example ready"
