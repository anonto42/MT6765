#!/usr/bin/env bash
# Build walpad-{core,shell,ctl,btbridge,gnss} for aarch64-unknown-linux-musl -> build/rust/aarch64/ (+ sizes.txt).
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=build/rust/aarch64
scripts/rust.sh build --release --target aarch64-unknown-linux-musl -p walpad-core -p walpad-shell -p walpad-ctl -p walpad-btbridge -p walpad-gnss
mkdir -p "$OUT"
for b in walpad-core walpad-shell walpad-ctl walpad-btbridge walpad-gnss; do cp "target/aarch64-unknown-linux-musl/release/$b" "$OUT/$b"; done
( cd "$OUT" && ls -l walpad-* | awk '{print $5, $9}' > sizes.txt; cat sizes.txt )
