#!/usr/bin/env bash
# Copies the example built by the emk/rust buildpack (run on Heroku by RustConfig)
# into target/release, where the Procfile starts it: the buildpack only copies the
# binaries of target/release itself.
set -euo pipefail
example="${1:-demo}"
cd "$(dirname "$0")"
mkdir -p target/release
cp "${CARGO_TARGET_DIR:-target}/release/examples/$example" target/release/
echo "       $example ready in target/release"
