#!/usr/bin/env bash
# Host test of tools/fbmirror/logic.h inside a throwaway Alpine container.  Exit 77 = docker unavailable.
set -euo pipefail
cd "$(dirname "$0")"
if ! command -v docker >/dev/null 2>&1 || ! docker info >/dev/null 2>&1; then echo "docker unavailable"; exit 77; fi
docker run --rm -v "$PWD":/w:ro -w /w alpine:latest sh -c '
  apk add --no-cache gcc musl-dev >/dev/null &&
  gcc -std=c99 -Wall -Wextra -Werror -O1 -o /tmp/test_logic test_logic.c && /tmp/test_logic'
