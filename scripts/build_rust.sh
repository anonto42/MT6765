#!/usr/bin/env bash
# Build walpad-{core,shell,ctl} for aarch64-unknown-linux-musl -> build/rust/aarch64/ (+ sizes.txt).
# Uses host cargo when present, otherwise a throwaway rust container (pinned in rust-toolchain.toml).
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=build/rust/aarch64
build() {
  cargo build --release --target aarch64-unknown-linux-musl -p walpad-core -p walpad-shell -p walpad-ctl
}
if command -v cargo >/dev/null 2>&1; then
  build
  T=target
else
  VER=$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)
  docker run --rm -v "$PWD":/w -w /w -e CARGO_HOME=/w/target/.cargo-home "rust:$VER-slim" sh -c '
    rustup target add aarch64-unknown-linux-musl >/dev/null 2>&1 || true
    cargo build --release --target aarch64-unknown-linux-musl -p walpad-core -p walpad-shell -p walpad-ctl
    rc=$?; chown -R '"$(id -u):$(id -g)"' /w/target 2>/dev/null; exit $rc'
  T=target
fi
mkdir -p "$OUT"
for b in walpad-core walpad-shell walpad-ctl; do cp "$T/aarch64-unknown-linux-musl/release/$b" "$OUT/$b"; done
( cd "$OUT" && ls -l walpad-* | awk '{print $5, $9}' > sizes.txt; cat sizes.txt )
