#!/usr/bin/env bash
# Must pass before every report.  Cargo steps run via scripts/rust.sh (host cargo or the CI image).
set -euo pipefail
cd "$(dirname "$0")/.."
echo "== fmt";    scripts/rust.sh fmt --all --check
echo "== clippy"; scripts/rust.sh clippy --workspace --all-targets -- -D warnings
echo "== test (X tests need Xvfb: provided by the CI image)"; scripts/rust.sh test --workspace
echo "== build";  scripts/build_rust.sh
echo "== arch"
for f in build/rust/aarch64/walpad-*; do
  case $f in *sizes.txt) continue;; esac
  file "$f" | grep -q 'ARM aarch64.*statically linked' || { echo "FAIL not static aarch64: $f"; exit 1; }
  echo "ok $f"
done
echo "== qemu smoke"
if command -v qemu-aarch64 >/dev/null 2>&1; then Q=qemu-aarch64; elif command -v qemu-aarch64-static >/dev/null 2>&1; then Q=qemu-aarch64-static; else Q=""; fi
if [ -n "$Q" ]; then
  for f in build/rust/aarch64/walpad-*; do case $f in *sizes.txt) continue;; esac; $Q "$f" --version; done
else
  VER=$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)
  docker run --rm -v "$PWD":/w -w /w "walpad-rust-ci:$VER" sh -c \
    'for f in build/rust/aarch64/walpad-*; do case $f in *sizes.txt) continue;; esac; qemu-aarch64 "$f" --version || exit 1; done'
fi
echo "== shell tests"
if sh tests/shell/run.sh | grep -v '^PASS'; then echo "FAIL: shell tests"; exit 1; fi
echo "== secrets grep"
if git grep -nI -e 'psk=' -e 'PSK=' -- crates tests 2>/dev/null; then echo "FAIL: psk literal found"; exit 1; fi
echo "ALL CHECKS PASSED"
