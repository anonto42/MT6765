#!/usr/bin/env bash
# Build the boot image that runs the Linux server ramdisk + on-device touch UI (padui).
#   scripts/build_ui_boot.sh <source-boot-image> <output-image> [init-script]   (default config/boot/init-os)
# The source image only supplies the kernel/dtb/cmdline and the BusyBox+dropbear ramdisk
# (firmware/MT6765/backups/boot_a-original.img is exactly that).  Flash with mtkclient:
#   tools/mtk/.venv/bin/python tools/mtk/mtk.py w boot_a <output-image>   (preloader window)
set -euo pipefail
cd "$(dirname "$0")/.."
SRC=${1:?source boot image}; OUT=${2:?output image}; INIT=${3:-config/boot/init-os}
W=$(mktemp -d)
python3 -I - "$SRC" "$W/orig.gz" <<'PY'
import struct, sys
d = open(sys.argv[1], "rb").read()
ks = struct.unpack("<I", d[8:12])[0]; rs = struct.unpack("<I", d[16:20])[0]; pg = struct.unpack("<I", d[36:40])[0]
up = lambda n: (n + pg - 1) // pg * pg
o = pg + up(ks); open(sys.argv[2], "wb").write(d[o:o + rs])
PY
mkdir "$W/rd"; (cd "$W/rd" && zcat ../orig.gz | cpio -idm --quiet)
command cp -f tools/padui/padui "$W/rd/bin/padui"; chmod 755 "$W/rd/bin/padui"
command cp -f "$INIT" "$W/rd/init"; chmod 755 "$W/rd/init"
(cd "$W/rd" && find . | cpio -o -H newc --owner=0:0 --quiet | gzip -9 > ../rd.cpio.gz)
python3 -I scripts/repack_boot.py "$SRC" "$W/rd.cpio.gz" "$W/new.img" >/dev/null
python3 -I - "$SRC" "$W/new.img" "$OUT" <<'PY'
import sys
src = open(sys.argv[1], "rb").read(); t = bytearray(open(sys.argv[2], "rb").read())
t += b"\0" * (len(src) - len(t)); t[-64:] = src[-64:]      # pad to partition size, keep the AVB footer
open(sys.argv[3], "wb").write(bytes(t)); print("wrote", sys.argv[3], len(t))
PY
rm -rf "$W"
