#!/bin/sh
# Build the 32-bit ARM property shim (see wpdprops32.c) in a throwaway Alpine container: clang's
# ARM backend plus lld, freestanding, no NDK.  Output: data/wifi/libwpdprops32.so
set -e
cd "$(dirname "$0")"
docker run --rm --platform linux/amd64 -v "$PWD":/w -w /w alpine:latest sh -c '
  apk add --no-cache clang lld >/dev/null &&
  clang --target=armv7a-linux-androideabi -marm -shared -fPIC -nostdlib -nostartfiles -ffreestanding \
        -fno-stack-protector -fno-builtin -Os -Wl,-soname,libwpdprops32.so -fuse-ld=lld \
        -o libwpdprops32.so wpdprops32.c'
mkdir -p ../../data/wifi && mv libwpdprops32.so ../../data/wifi/libwpdprops32.so
file ../../data/wifi/libwpdprops32.so
