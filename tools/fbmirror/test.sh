#!/usr/bin/env bash
# Host test of tools/fbmirror/logic.h in a container.  Exit 77 = docker unavailable.
# Prefers the local CI image (has gcc, needs no network); falls back to a throwaway Alpine container.
set -euo pipefail
cd "$(dirname "$0")"
if ! command -v docker >/dev/null 2>&1 || ! docker info >/dev/null 2>&1; then echo "docker unavailable"; exit 77; fi
VER=$(sed -n 's/^channel = "\(.*\)"/\1/p' ../../rust-toolchain.toml)
CC_CMD='gcc -std=c99 -Wall -Wextra -Werror -O1 -o /tmp/test_logic test_logic.c && /tmp/test_logic'
if docker image inspect "walpad-rust-ci:$VER" >/dev/null 2>&1; then
  docker run --rm -v "$PWD":/w:ro -w /w "walpad-rust-ci:$VER" sh -c "$CC_CMD"
else
  docker run --rm -v "$PWD":/w:ro -w /w alpine:latest sh -c "apk add --no-cache gcc musl-dev >/dev/null && $CC_CMD"
fi
