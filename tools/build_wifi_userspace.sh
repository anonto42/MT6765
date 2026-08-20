#!/usr/bin/env bash
# Build data/wifi-userspace/wifi-userspace-aarch64.tar.gz — a self-contained
# aarch64 wpa_supplicant + iw for the tablet's busybox ramdisk.
#
# The ramdisk has neither, and the stock /vendor copies are bionic binaries
# that need /system's linker.  Alpine's aarch64 packages give us musl builds
# instead; we pull them, take just the binaries plus the exact set of shared
# libraries they name, and verify the dependency closure before packing.
#
# Usage: tools/build_wifi_userspace.sh
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$REPO/data/wifi-userspace"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

MIRROR=https://dl-cdn.alpinelinux.org/alpine/latest-stable/main/aarch64
PKGS="musl libnl3 libcrypto3 libssl3 dbus-libs pcsc-lite-libs wpa_supplicant iw"

echo "==> fetching package index"
curl -fsS -o "$WORK/APKINDEX.tar.gz" "$MIRROR/APKINDEX.tar.gz"
tar xzf "$WORK/APKINDEX.tar.gz" -C "$WORK" APKINDEX

# APKINDEX is newline-separated records; pick the P:/V: pair of each package
# we asked for and turn it into a filename.
PAT="$(echo "$PKGS" | tr ' ' '|')"
awk -v RS="" -v pat="^($PAT)$" '
  { p=""; v=""; n=split($0,L,"\n")
    for (i=1;i<=n;i++) { if (L[i]~/^P:/) p=substr(L[i],3); if (L[i]~/^V:/) v=substr(L[i],3) }
    if (p ~ pat) print p "-" v ".apk" }' "$WORK/APKINDEX" > "$WORK/list.txt"

echo "==> packages:"; sed 's/^/    /' "$WORK/list.txt"

mkdir -p "$WORK/root"
while read -r pkg; do
    [ -n "$pkg" ] || continue
    curl -fsS -o "$WORK/$pkg" "$MIRROR/$pkg"
    tar xzf "$WORK/$pkg" -C "$WORK/root" 2>/dev/null || true
done < "$WORK/list.txt"

echo "==> assembling bundle"
B="$WORK/bundle"
mkdir -p "$B/sbin" "$B/lib"
for b in sbin/wpa_supplicant sbin/wpa_cli sbin/wpa_passphrase usr/sbin/iw; do
    cp "$WORK/root/$b" "$B/sbin/"
done
cp -P "$WORK/root/lib/ld-musl-aarch64.so.1" "$B/lib/"
cp -P "$WORK/root/lib/libc.musl-aarch64.so.1" "$B/lib/"
for l in libnl-3.so.200 libnl-genl-3.so.200 libnl-route-3.so.200 \
         libssl.so.3 libcrypto.so.3 libdbus-1.so.3 \
         libpcsclite.so.1 libpcsclite_real.so.1; do
    cp -L "$WORK/root/usr/lib/$l" "$B/lib/" 2>/dev/null || echo "    (no $l)"
done
chmod +x "$B"/sbin/* "$B/lib/ld-musl-aarch64.so.1"

echo "==> verifying dependency closure"
python3 - "$B" <<'PY'
import os, subprocess, sys
b = sys.argv[1]
have = set(os.listdir(os.path.join(b, "lib")))
missing = set()
for d in ("sbin", "lib"):
    for f in os.listdir(os.path.join(b, d)):
        out = subprocess.run(["readelf", "-d", os.path.join(b, d, f)],
                             capture_output=True, text=True).stdout
        for ln in out.splitlines():
            if "(NEEDED)" in ln:
                lib = ln.split("[")[1].split("]")[0]
                if lib not in have:
                    missing.add((f, lib))
if missing:
    for f, lib in sorted(missing):
        print("    MISSING: %s needs %s" % (f, lib))
    sys.exit("dependency closure incomplete - add the providing package above")
print("    closure complete")
PY

mkdir -p "$OUT"
tar czf "$OUT/wifi-userspace-aarch64.tar.gz" -C "$B" sbin lib
echo "==> $OUT/wifi-userspace-aarch64.tar.gz ($(du -h "$OUT/wifi-userspace-aarch64.tar.gz" | cut -f1))"
