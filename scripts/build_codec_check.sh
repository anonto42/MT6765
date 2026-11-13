#!/usr/bin/env bash
# Compile-check one in-tree object against WalPad's exact running kernel config/headers ->
# build/codec-check/<name>.o (plus build/codec-check/build.log).
#
# mtk-soc-codec-6357.c (the MT6357 audio codec driver) is built directly into the boot Image on
# this board, not a loadable module (obj-y under sound/soc/mediatek/codec/mt6357/, selected by
# CONFIG_MTK_SND_SOC_CODEC="mt6357" in data/kernel/walpad-4.19.127.config) -- there is no insmod
# path to test a change the way scripts/build_bt_module.sh / build_zt9101_module.sh do for a real
# loadable module. This script instead reuses the same proven header/config overlay recipe (read
# build_bt_module.sh's comment first) to compile just the one changed object with kbuild's
# single-file-target form (`make path/to/file.o`), which still catches real syntax/type errors
# against the exact struct layouts and CONFIG_* set the running kernel was built with. It does not
# produce a flashable Image and cannot catch a link-time/whole-Image-only problem.
#
# Usage: scripts/build_codec_check.sh <path/to/file.o relative to KSRC>
#   e.g.: scripts/build_codec_check.sh sound/soc/mediatek/codec/mt6357/mtk-soc-codec-6357.o
#
# Env: KSRC (default tools/kernel-mt6765-4.19), VENDOR_MODULES (default build/wifi/vendor-modules,
#      needs kheaders.ko; run scripts/build_wifi_module.sh or scripts/build_bt_module.sh once to
#      populate it if missing).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
TARGET=${1:?usage: scripts/build_codec_check.sh <path/to/file.o>}
KSRC=$(realpath "${KSRC:-tools/kernel-mt6765-4.19}")
VM=$(realpath "${VENDOR_MODULES:-$ROOT/build/wifi/vendor-modules}")
CONFIG=$ROOT/data/kernel/walpad-4.19.127.config
OUT=$ROOT/build/codec-check
IMG=debian:bullseye

[ -f "$KSRC/Makefile" ] || { echo "kernel tree missing: $KSRC (git submodule update --init)"; exit 1; }
[ -f "$VM/kheaders.ko" ] || { echo "no $VM/kheaders.ko: set VENDOR_MODULES or populate it first"; exit 1; }
mkdir -p "$OUT"

docker run --rm -i -v "$KSRC":/ksrc:ro -v "$VM":/vm:ro -v "$OUT":/out -v "$CONFIG":/config:ro \
  -e TARGET="$TARGET" -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" "$IMG" bash -s <<'EOF'
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
sed -n 's/^# *\(deb http:\/\/snapshot.debian.org.*\)/\1/p' /etc/apt/sources.list > /tmp/snap.list
[ -s /tmp/snap.list ] && mv /tmp/snap.list /etc/apt/sources.list
APT="apt-get -qq -o Acquire::Check-Valid-Until=false -o Acquire::Retries=5"
for try in 1 2 3; do
  $APT update >/dev/null && $APT install -y --no-install-recommends clang-11 lld-11 llvm-11 \
    binutils-aarch64-linux-gnu make bc flex bison libssl-dev libelf-dev python3 xz-utils kmod file rsync \
    >/dev/null 2>&1 && break
  [ $try = 3 ] && { echo "FAIL: apt"; exit 1; }; sleep 10
done
export PATH=/usr/lib/llvm-11/bin:$PATH
clang --version | head -1
W=/build; S=$W/src
mkdir -p $W
echo "== copying kernel tree"
rsync -a --exclude .git /ksrc/ $S/

echo "== laying the running kernel's headers (kheaders.ko) over include/ and arch/arm64/include/"
python3 - <<'PY'
d = open('/vm/kheaders.ko', 'rb').read()
i = d.find(b'\xfd7zXZ\x00')
assert i >= 0, 'no xz stream in kheaders.ko'
open('/build/kh.tar.xz', 'wb').write(d[i:])
PY
mkdir -p $W/kh && (xz -dc $W/kh.tar.xz 2>/dev/null || true) | tar -x -C $W/kh
grep -q '"4.19.127"' $W/kh/include/generated/utsrelease.h
cp -a $W/kh/include/. $S/include/
cp -a $W/kh/arch/arm64/include/. $S/arch/arm64/include/
rm -rf $S/include/generated $S/include/config $S/arch/arm64/include/generated
sed -i 's/^SUBLEVEL = .*/SUBLEVEL = 127/' $S/Makefile
python3 - <<'PY'
import re
src = open('/build/kh/include/generated/asm-offsets.h').read()
out = ['#include <linux/kbuild.h>', 'int main(void)', '{']
out += [f'  DEFINE({m[0]}, {m[1]});' for m in re.findall(r'^#define (\w+) (.+?)\s*$', src, re.M)
        if not m[0].startswith('__ASM_OFFSETS_H__')]
open('/build/src/arch/arm64/kernel/asm-offsets.c', 'w').write('\n'.join(out + ['  return 0;', '}']) + '\n')
p = '/build/src/arch/arm64/Makefile'
s = open(p).read()
s2 = re.sub(r'(vdso_prepare: prepare0\n)(?:\t.*\n)+',
            r'\1\t$(Q)cp /build/kh/include/generated/vdso-offsets.h include/generated/vdso-offsets.h\n', s)
assert s2 != s, 'vdso_prepare rule not found'
open(p, 'w').write(s2)
PY

MK="make -C $S ARCH=arm64 LLVM=1 CROSS_COMPILE=aarch64-linux-gnu- -j$(nproc)"
cp /config $S/.config
$MK olddefconfig >/dev/null 2>&1

echo "== modules_prepare"
$MK modules_prepare > $W/prepare.log 2>&1 || { grep -v Kconfig $W/prepare.log | tail -30; exit 1; }
# Struct layouts the running kernel actually used come from its own autoconf.h, not this tree's
# olddefconfig result (same substitution build_bt_module.sh relies on -- see its comment).
python3 - <<'PY'
import re
kh = open('/build/kh/include/generated/autoconf.h').read()
cfg = [l.strip() for l in open('/config') if l.startswith('CONFIG_')]
open('/build/src/include/generated/autoconf.h', 'w').write(kh)
open('/build/src/include/config/auto.conf', 'w').write('\n'.join(cfg) + '\n')
PY
touch $S/include/config/auto.conf $S/include/generated/autoconf.h
$MK prepare > $W/prepare2.log 2>&1 || { grep -v Kconfig $W/prepare2.log | tail -30; exit 1; }

echo "== compiling $TARGET"
rm -f "$S/$TARGET"
# subdir-ccflags-y from sound/soc/mediatek/Makefile (common_int/ + codec/$(MTK_CODEC)/ include
# paths) is only added when kbuild recurses into that Makefile as part of a real top-level build;
# a bare single-object target under it does not pick it up, so pass the same -I paths explicitly.
# This tree's sound/soc/mediatek/Makefile already disables -Wincompatible-pointer-types
# treewide (the halium-12 fork added -Wall -Wextra -Werror, "treewide: respect global Wall,
# Wextra, Werror"); Debian's clang-11.0.1-2 package splits function-pointer-signature
# mismatches into a separate -Wincompatible-function-pointer-types the real Android prebuilt
# clang-11.0.1 (r383902, vendor's actual toolchain) does not, so one real, pre-existing,
# harmless mismatch here (mt6357_component_remove() returning int for a void-returning
# .remove field -- same as the vendor's own upstream driver on other MTK boards in this tree)
# would otherwise fail only under this stand-in toolchain, not the real one. Suppressed here,
# not in the source: this is a toolchain gap, not something to silently "fix" in the driver.
KCFLAGS="-I$S/sound/soc/mediatek/common_int -I$S/sound/soc/mediatek/codec/mt6357 \
  -Wno-incompatible-function-pointer-types" \
  $MK "$TARGET" 2>&1 | tee $W/build.log
test -f "$S/$TARGET"
file "$S/$TARGET"
cp "$S/$TARGET" /out/
cp $W/build.log /out/
echo "OK: $TARGET compiled clean against the running kernel's exact config/headers"
EOF

chown "$(id -u)":"$(id -g)" "$OUT"/* 2>/dev/null || true
echo "output: $OUT/$(basename "$TARGET"), $OUT/build.log"
