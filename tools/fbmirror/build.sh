#!/usr/bin/env bash
# Build fbmirror (static aarch64, freestanding) in a throwaway Alpine container.
set -euo pipefail
cd "$(dirname "$0")"
docker run --rm --platform linux/amd64 -v "$PWD":/w -w /w alpine:latest sh -c '
  apk add --no-cache clang lld >/dev/null &&
  clang --target=aarch64-linux-gnu -O2 -Wall -ffreestanding -fno-builtin -nostdlib -static -fno-pic -fuse-ld=lld \
        -fno-stack-protector -Wl,-z,norelro -o fbmirror fbmirror.c'
file fbmirror
