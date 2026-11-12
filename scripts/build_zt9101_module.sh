#!/usr/bin/env bash
# Build zt9101_ztopmac_usb.ko (ZT9101 USB Wi-Fi dongle, station mode) for WalPad's running
# kernel -> build/zt9101/zt9101_ztopmac_usb.ko
#
# Same header/autoconf.h/asm-offsets overlay recipe as scripts/build_bt_module.sh (read that
# script's comment first; this reuses its scripts/bt/versions.py and merge_symvers.py unchanged).
# Difference: this is a standalone out-of-tree module (its own Makefile, ~30 .c files, no kernel
# tree integration), built the normal external-module way: `make -C $KSRC M=<driver dir> modules`,
# with ARCH/CROSS_COMPILE passed on the command line (HOST_PLAT is left unmatched so the vendor
# Makefile's platform.mak does not override them with a PC/Raspberry toolchain guess).
#
# Owner request 2026-11-12 (openspec/changes/zt9101-usb-wifi-driver/proposal.md): station mode
# only. CONFIG_WIFI_MODE=all in the vendor Makefile still compiles ap.o/adhoc.o in (upstream
# default, not touched here); monitor/injection code this driver also carries is never exercised
# from WalPad regardless of what the Makefile compiles in.
#
# Env: KSRC (default tools/kernel-mt6765-4.19), ZSRC (the driver's driver/ dir, default
#      $ZT9101_SRC or /home/md/Code/zt9101-wifi/dobbsbp/driver), VENDOR_MODULES (default
#      build/wifi/vendor-modules, already extracted by build_wifi_module.sh: has kheaders.ko +
#      wmt_drv.ko etc., reused here unchanged -- this build needs no SUPER_IMG re-extraction).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
KSRC=$(realpath "${KSRC:-tools/kernel-mt6765-4.19}")
ZSRC=$(realpath "${ZSRC:-${ZT9101_SRC:-/home/md/Code/zt9101-wifi/dobbsbp/driver}}")
OUT=$ROOT/build/zt9101
VM=${VENDOR_MODULES:-$ROOT/build/wifi/vendor-modules}
CONFIG=$ROOT/data/kernel/walpad-4.19.127.config
IMG=debian:bullseye

[ -f "$KSRC/Makefile" ] || { echo "kernel tree missing: $KSRC"; exit 1; }
[ -f "$ZSRC/Makefile" ] || { echo "zt9101 driver source missing: $ZSRC"; exit 1; }
VM=$(realpath "$VM")
[ -f "$VM/kheaders.ko" ] || { echo "no $VM/kheaders.ko: run scripts/build_wifi_module.sh first, or set VENDOR_MODULES"; exit 1; }
mkdir -p "$OUT"

docker run --rm -i -v "$KSRC":/ksrc:ro -v "$ZSRC":/zsrc:ro -v "$VM":/vm:ro -v "$OUT":/out \
  -v "$CONFIG":/config:ro -v "$ROOT/scripts/bt":/tools:ro \
  -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" "$IMG" bash -s <<'EOF'
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
W=/build; S=$W/src; Z=$W/zsrc
mkdir -p $W

echo "== copying kernel tree"
rsync -a --exclude .git /ksrc/ $S/
echo "== copying zt9101 driver source"
rsync -a --exclude .git /zsrc/ $Z/

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
python3 - <<'PY'
import re
kh = open('/build/kh/include/generated/autoconf.h').read()
defs = set(re.findall(r'^#define (CONFIG_\w+) ', kh, re.M))
cfg = [l.strip() for l in open('/config') if l.startswith('CONFIG_')]
want = set()
for l in cfg:
    k, v = l.split('=', 1)
    want.add(k + '_MODULE' if v == 'm' else k)
if defs != want:
    print('FAIL: data config != running kernel autoconf.h:', sorted(defs ^ want)[:20]); raise SystemExit(1)
print(f'data/kernel config == running kernel autoconf.h ({len(want)} symbols)')
open('/build/src/include/generated/autoconf.h', 'w').write(kh)
open('/build/src/include/config/auto.conf', 'w').write('\n'.join(cfg) + '\n')
PY
touch $S/include/config/auto.conf $S/include/generated/autoconf.h
$MK prepare > $W/prepare2.log 2>&1 || { grep -v Kconfig $W/prepare2.log | tail -30; exit 1; }
for h in bounds.h asm-offsets.h vdso-offsets.h; do
  diff <(grep '#define' $W/kh/include/generated/$h | awk '{print $2, $3}') \
       <(grep '#define' $S/include/generated/$h | awk '{print $2, $3}') > /dev/null \
    && echo "generated $h == running kernel's" || { echo "FAIL: generated $h differs"; exit 1; }
done
REL=$(cat $S/include/config/kernel.release)
echo "kernel.release=$REL"; [ "$REL" = 4.19.127 ] || { echo "FAIL: release $REL"; exit 1; }

echo "== building zt9101 driver (pass 1: list the symbols it needs; HOST_PLAT left unmatched)"
MODMK="make -C $Z HOST_PLAT=walpad ARCH=arm64 LLVM=1 CROSS_COMPILE=aarch64-linux-gnu- KSRC=$S USER_EXTRA_CFLAGS=-DCONFIG_LITTLE_ENDIAN modules"
$MODMK > $W/pass1.log 2>&1 || { tail -60 $W/pass1.log; exit 1; }
MOD=$(find $Z -maxdepth 1 -name '*.o' ! -name '*.mod.o' | head -1)
echo "module object: $MOD"
llvm-nm -u "$MOD" | awk '{print $2}' | grep -vx __this_module | sort -u > $W/needed.txt
echo "needs $(wc -l < $W/needed.txt) symbols"

echo "== CRCs from vendor modules"
python3 /tools/versions.py /vm/*.ko > $W/vendor-crcs.txt
echo "vendor __versions tables: $(wc -l < $W/vendor-crcs.txt) symbols"

echo "== CRCs computed by genksyms (4.19.127 headers + running config)"
: > $W/files.txt
for sym in $(cat $W/needed.txt); do
  grep -rlE --include='*.c' "^EXPORT_SYMBOL(_GPL)?\($sym\);" $S/kernel $S/mm $S/fs $S/lib $S/net \
    $S/drivers/base $S/drivers/char $S/drivers/usb $S/arch/arm64 $S/block $S/crypto 2>/dev/null >> $W/files.txt || true
done
sort -u -o $W/files.txt $W/files.txt
: > $W/crc-computed.txt
while read -r f; do
  rel=${f#$S/}; i=${rel%.c}.i
  if $MK KCFLAGS=-D__GENKSYMS__ "$i" > $W/i.log 2>&1; then
    $S/scripts/genksyms/genksyms < $S/$i 2>/dev/null \
      | sed -n 's/^__crc_\([A-Za-z0-9_]*\) = \(0x[0-9a-f]*\).*/\1 \2/p' >> $W/crc-computed.txt
    echo "   $rel"
  else
    echo "   (cannot preprocess $rel; its symbols rely on vendor CRCs)"
  fi
done < $W/files.txt
sort -u -o $W/crc-computed.txt $W/crc-computed.txt

echo "== expected CRC mismatches (root-caused, not a blanket override)"
# kernel/sched/*.c and mm/memory.c are MTK-vendor-patched source (not just headers) on this
# platform; tools/kernel-mt6765-4.19's 4.19.191 copies of those two files differ from what
# actually built Walton's shipped 4.19.127 kernel, and the header-overlay step above cannot fix
# a .c-file difference (it only overlays include/, not kernel/sched/ or mm/). Confirmed: every
# computed-vs-vendor mismatch this build has ever hit traces to an EXPORT_SYMBOL in exactly one
# of those files (wake_up_process, remap_pfn_range, set_user_nice -- kernel/sched/core.c and
# mm/memory.c). Vendor's CRC (read from the real, already-loaded wmt_drv.ko etc. on this exact
# device) is ground truth here, not a guess, so it is trusted over our locally recomputed one.
printf '%s\n' wake_up_process remap_pfn_range set_user_nice > $W/expected-mismatch.txt
# modpost reads Module.symvers from the kernel BUILD tree ($S), not from the external module's
# own directory (M=$Z) -- the symbol dump location is tied to $(objtree), unrelated to where the
# out-of-tree module source lives. Writing it to $Z (first attempt) produced a silent "Symbol
# version dump ./Module.symvers is missing" and a CRC-less module; $S is the right target.
python3 /tools/merge_symvers.py $W/needed.txt $W/vendor-crcs.txt $W/crc-computed.txt $S/Module.symvers $W/crc-report.txt $W/expected-mismatch.txt
cat $W/crc-report.txt

echo "== building zt9101 driver (pass 2: with Module.symvers)"
rm -f "$Z"/*.ko "$Z"/*.mod.* "$MOD"
$MODMK > $W/pass2.log 2>&1 || { tail -60 $W/pass2.log; exit 1; }
if grep -E 'undefined!|WARNING' $W/pass2.log; then echo "FAIL: modpost warnings"; exit 1; fi
K=$(find $Z -maxdepth 1 -name '*.ko' | head -1)
[ -n "$K" ] || { echo "FAIL: no .ko produced"; exit 1; }
llvm-strip --strip-debug "$K"
cp "$K" /out/
cp $W/crc-report.txt /out/
modinfo "$K" | tee /out/modinfo.txt
python3 /tools/versions.py "$K" > /out/zt9101.versions.txt
VM_OURS=$(modinfo -F vermagic "$K")
VM_VENDOR=$(modinfo -F vermagic /vm/wmt_drv.ko)
echo "vermagic ours  : $VM_OURS"; echo "vermagic vendor: $VM_VENDOR"
[ "$VM_OURS" = "$VM_VENDOR" ] || { echo "FAIL: vermagic mismatch"; exit 1; }
python3 - <<PY
sv = {l.split('\t')[1]: int(l.split('\t')[0], 16) for l in open('$S/Module.symvers')}
for line in open('/out/zt9101.versions.txt'):
    s, c = line.split(); assert sv.get(s) == int(c, 16), f'CRC of {s} not from Module.symvers'
print('module __versions match Module.symvers')
PY
file "$K"
chown -R "$HOST_UID:$HOST_GID" /out
EOF
echo "== done; build/zt9101/ contents:"
ls -la "$OUT"
