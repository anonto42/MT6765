#!/usr/bin/env bash
# Run any cargo command with the pinned toolchain: host cargo if present, else the local CI image
# (scripts/ci/Dockerfile, built once and cached as walpad-rust-ci:<rust version>).
set -euo pipefail
cd "$(dirname "$0")/.."
if command -v cargo >/dev/null 2>&1; then exec cargo "$@"; fi
VER=$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)
IMG=walpad-rust-ci:$VER
docker image inspect "$IMG" >/dev/null 2>&1 || docker build -q -t "$IMG" --build-arg RUST_VERSION="$VER" scripts/ci >&2
exec docker run --rm -v "$PWD":/w -w /w -e CARGO_HOME=/w/target/.cargo-home -e UPDATE_SNAPSHOTS -e RUST_BACKTRACE -u "$(id -u):$(id -g)" -e HOME=/tmp "$IMG" cargo "$@"
