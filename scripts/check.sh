#!/usr/bin/env bash
# Must pass before every report.  Uses host tools when present, else a rust container.
set -euo pipefail
cd "$(dirname "$0")/.."
VER=$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)
run() {
  if command -v cargo >/dev/null 2>&1; then sh -c "$1"
  else docker run --rm -v "$PWD":/w -w /w -e CARGO_HOME=/w/target/.cargo-home "rust:$VER-slim" sh -c \
    "rustup component add rustfmt clippy >/dev/null 2>&1; $1; rc=\$?; chown -R $(id -u):$(id -g) /w/target 2>/dev/null; exit \$rc"; fi
}
echo "== fmt";    run "cargo fmt --all --check"
echo "== clippy"; run "cargo clippy --workspace --all-targets -- -D warnings"
echo "== test";   run "cargo test --workspace"
echo "== build";  scripts/build_rust.sh
echo "== arch"
for f in build/rust/aarch64/walpad-*; do
  case $f in *sizes.txt) continue;; esac
  file "$f" | grep -q 'ARM aarch64.*statically linked' || { echo "FAIL not static aarch64: $f"; exit 1; }
  echo "ok $f"
done
echo "== qemu smoke"
if command -v qemu-aarch64 >/dev/null 2>&1; then Q=qemu-aarch64
elif command -v qemu-aarch64-static >/dev/null 2>&1; then Q=qemu-aarch64-static
else Q=""; fi
if [ -n "$Q" ]; then
  for f in build/rust/aarch64/walpad-*; do case $f in *sizes.txt) continue;; esac; $Q "$f" --version; done
else
  docker run --rm -v "$PWD":/w -w /w debian:stable-slim sh -c \
    'apt-get update -qq >/dev/null && apt-get install -y -qq qemu-user >/dev/null &&
     for f in build/rust/aarch64/walpad-*; do case $f in *sizes.txt) continue;; esac; qemu-aarch64 "$f" --version || exit 1; done'
fi
echo "== shell tests"
sh tests/shell/run.sh | grep -v '^PASS' && { echo "FAIL: shell tests"; exit 1; } || true
echo "== secrets grep"
if git grep -nI -e 'psk=' -e 'PSK=' -- crates tests 2>/dev/null; then echo "FAIL: psk literal found"; exit 1; fi
echo "ALL CHECKS PASSED"
