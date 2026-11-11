#!/bin/sh
# Build the property shims with their own files and a 256 KB read buffer, in a throwaway Alpine
# container (clang + lld, freestanding, no NDK):
#   data/wifi/libwpdprops32.so   32-bit ARM, store /tmp/wpd-props32  (modem, GPS daemons)
#   data/wifi/libwpdprops64b.so  64-bit,     store /tmp/wpd-props64  (camera helper / factory)
# The Wi-Fi stack keeps tools/propshim's libwpdprops.so and /tmp/wpd-props (frozen, never shared).
set -e
cd "$(dirname "$0")"
docker run --rm --platform linux/amd64 -v "$PWD":/w -w /w alpine:latest sh -c '
  apk add --no-cache clang lld >/dev/null &&
  clang --target=armv7a-linux-androideabi -marm -shared -fPIC -nostdlib -nostartfiles -ffreestanding \
        -fno-stack-protector -fno-builtin -Os -Wl,-soname,libwpdprops32.so -fuse-ld=lld \
        -o libwpdprops32.so wpdprops32.c &&
  clang --target=aarch64-linux-android -shared -fPIC -nostdlib -nostartfiles -ffreestanding \
        -fno-stack-protector -fno-builtin -Os -Wl,-soname,libwpdprops64b.so -fuse-ld=lld \
        -o libwpdprops64b.so wpdprops64b.c'
mkdir -p ../../data/wifi && mv libwpdprops32.so libwpdprops64b.so ../../data/wifi/
file ../../data/wifi/libwpdprops32.so ../../data/wifi/libwpdprops64b.so
