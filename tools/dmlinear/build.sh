#!/usr/bin/env bash
# Build dmlinear (tiny read-only dm-linear creator, static aarch64) in a throwaway Alpine container.
set -euo pipefail
cd "$(dirname "$0")"
docker run --rm --platform linux/amd64 -v "$PWD":/w -w /w alpine:latest sh -c '
  apk add --no-cache clang lld >/dev/null &&
  clang --target=aarch64-linux-gnu -O2 -ffreestanding -nostdlib -static -fno-pic -fuse-ld=lld \
        -fno-stack-protector -Wl,-z,norelro -o dmlinear dmlinear.c'
file dmlinear
