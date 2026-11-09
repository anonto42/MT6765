#!/usr/bin/env bash
# Run any cargo command with the pinned toolchain (host cargo, else a throwaway container).
set -euo pipefail
cd "$(dirname "$0")/.."
if command -v cargo >/dev/null 2>&1; then exec cargo "$@"; fi
VER=$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)
docker run --rm -v "$PWD":/w -w /w -e CARGO_HOME=/w/target/.cargo-home "rust:$VER-slim" sh -c \
  'rustup component add rustfmt clippy >/dev/null 2>&1; cargo "$@"; rc=$?; chown -R '"$(id -u):$(id -g)"' /w/target 2>/dev/null; exit $rc' sh "$@"
