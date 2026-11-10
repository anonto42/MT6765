#!/usr/bin/env bash
# Build camacdk (bionic aarch64 executable, freestanding source, no NDK) in a throwaway Alpine
# container: clang + lld are the whole toolchain.  Output: data/camera/camacdk (gitignored).
#
# The program calls libc/libdl through hand-written prototypes.  To let lld link it we generate two
# stub shared objects named libc.so and libdl.so that define those symbols; they only supply
# DT_NEEDED entries and symbol names, nothing of them is installed.  On the tablet the bionic
# linker resolves everything against the real bootstrap libc.so / libdl.so.
set -euo pipefail
cd "$(dirname "$0")"
HERE=$PWD
OUT=$HERE/../../data/camera
mkdir -p "$OUT"
docker run --rm --platform linux/amd64 -v "$HERE":/w:ro -v "$(cd "$OUT" && pwd)":/out -e OWNER="$(id -u):$(id -g)" -w /tmp alpine:latest sh -c '
  apk add --no-cache clang lld >/dev/null &&
  mkdir stubs &&
  for s in open close read write rename unlink mkdir stat alarm signal usleep _exit syscall vsnprintf malloc free getenv fork waitpid clock_gettime __libc_init; do echo "void $s(void) {}"; done > stubs/libc.c &&
  for s in dlopen dlsym dlerror; do echo "void $s(void) {}"; done > stubs/libdl.c &&
  clang --target=aarch64-linux-android -shared -fPIC -nostdlib -ffreestanding -fno-builtin -fuse-ld=lld -Wl,-soname,libc.so  -o stubs/libc.so  stubs/libc.c &&
  clang --target=aarch64-linux-android -shared -fPIC -nostdlib -ffreestanding -fno-builtin -fuse-ld=lld -Wl,-soname,libdl.so -o stubs/libdl.so stubs/libdl.c &&
  clang --target=aarch64-linux-android -Os -Wall -Wextra -Wno-unused-parameter -ffreestanding -fno-builtin -fno-stack-protector -fPIE -pie \
        -nostdlib -nostartfiles -fuse-ld=lld -Wl,--no-as-needed -Wl,--dynamic-linker=/system/bin/linker64 -Wl,-z,norelro \
        -Lstubs -lc -ldl -o /out/camacdk /w/camacdk.c &&
  chmod 755 /out/camacdk && chown "$OWNER" /out/camacdk'
file "$OUT/camacdk"
