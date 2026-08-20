#!/bin/sh
# Build the aarch64 property shim.  Freestanding, so no Android NDK needed:
# clang's aarch64 backend plus lld is the whole toolchain.
set -e
HERE=$(dirname "$0")
OUT=$HERE/../../data/wifi/libwpdprops.so
mkdir -p "$(dirname "$OUT")"
clang --target=aarch64-linux-android \
      -shared -fPIC -nostdlib -nostartfiles -ffreestanding \
      -fno-stack-protector -fno-builtin -Os \
      -Wl,-soname,libwpdprops.so -fuse-ld=lld \
      -o "$OUT" "$HERE/wpdprops.c"
echo "built $OUT"
