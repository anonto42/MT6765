#!/usr/bin/env bash
# Build hci_vhci.ko (and only that module) for the tablet's running kernel -> build/bt/hci_vhci.ko
#
# The running kernel is Walton's 4.19.127 (Android clang 11.0.1 r383902).  The only source tree we
# have (tools/kernel-mt6765-4.19) is a different MT6765 vendor tree at 4.19.191 with a different
# Kconfig, so on its own it would give the wrong vermagic and symbol CRCs computed from 4.19.191
# struct layouts.  The vendor partition ships kheaders.ko (CONFIG_IKHEADERS): the exact include/ and
# arch/arm64/include/ of the running kernel, including generated autoconf.h / asm-offsets.h /
# vdso-offsets.h.  So:
#
#   1. vendor modules (kheaders.ko, wmt_drv.ko, bt_drv.ko, ...) are pulled out of the stock `super`
#      dump (build/android-restore/super-raw.img, offsets from data/super/layout.json) unless
#      VENDOR_MODULES points at a directory that already holds them;
#   2. the 4.19.191 tree is copied (no VCS metadata) and the running kernel's headers are laid over
#      its include/ and arch/arm64/include/; SUBLEVEL is set to 127;
#   3. .config = data/kernel/walpad-4.19.127.config + CONFIG_BT_HCIVHCI=m, `make olddefconfig`
#      (needed by this tree's Kconfig), `make modules_prepare`; then autoconf.h/auto.conf are replaced
#      by the RUNNING kernel's (+ BT_HCIVHCI=m), and asm-offsets.h / vdso-offsets.h are the running
#      kernel's (the 4.19.191 asm-offsets.c and vdso do not build against 4.19.127 headers);
#   4. Module.symvers for the symbols hci_vhci needs: CRCs read from the vendor modules' __versions
#      tables (authoritative: they load on the running kernel); the rest (the built-in Bluetooth
#      core) computed by genksyms over the exporting .c file preprocessed against the 4.19.127
#      headers and config.  Symbols both methods cover are cross-checked; a disagreement fails;
#   5. modpost with that Module.symvers; vermagic is compared with wmt_drv.ko's.
#
# insmod refuses a module whose CRC disagrees ("disagrees about version of symbol ..."); it does
# not crash.  Runs in a throwaway debian:bullseye container (clang/lld 11.0.1, like r383902; GNU as
# from binutils-aarch64-linux-gnu because the 4.19.127 headers predate integrated-as support).
#
# Env: KSRC (default tools/kernel-mt6765-4.19), SUPER_IMG (default build/android-restore/super-raw.img),
#      VENDOR_MODULES (directory with kheaders.ko + vendor .ko files; skips the super extraction).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
KSRC=$(realpath "${KSRC:-tools/kernel-mt6765-4.19}")
SUPER_IMG=${SUPER_IMG:-build/android-restore/super-raw.img}
OUT=$ROOT/build/bt
VM=${VENDOR_MODULES:-$OUT/vendor-modules}
CONFIG=$ROOT/data/kernel/walpad-4.19.127.config
IMG=debian:bullseye

[ -f "$KSRC/drivers/bluetooth/hci_vhci.c" ] || { echo "kernel tree missing: $KSRC (git submodule update --init, or KSRC=...)"; exit 1; }
mkdir -p "$OUT" "$VM"
VM=$(realpath "$VM")

if [ ! -f "$VM/kheaders.ko" ]; then
  [ -f "$SUPER_IMG" ] || { echo "no $VM/kheaders.ko and no $SUPER_IMG: set VENDOR_MODULES"; exit 1; }
  read -r OFF LEN < <(python3 -c '
import json; l = json.load(open("data/super/layout.json"))
e = [p for p in l["partitions"] if p["name"] == "vendor_a"][0]["extents"][0]
print(e["byte_offset"], e["byte_length"])')
  echo "== extracting vendor_a ($LEN bytes at $OFF) from $SUPER_IMG"
  dd if="$SUPER_IMG" of="$OUT/vendor_a.img" bs=4096 skip=$((OFF / 4096)) count=$((LEN / 4096)) status=none
  for m in kheaders bt_drv wmt_drv wmt_chrdev_wifi wlan_drv_gen4m gps_drv fmradio_drv mt6357-accdet trace_mmstat fpsgo met; do
    debugfs -R "dump /lib/modules/$m.ko $VM/$m.ko" "$OUT/vendor_a.img" 2>/dev/null
  done
  rm -f "$OUT/vendor_a.img"
fi
ls "$VM"/*.ko >/dev/null

docker run --rm -i -v "$KSRC":/ksrc:ro -v "$VM":/vm:ro -v "$OUT":/out -v "$CONFIG":/config:ro \
  -v "$ROOT/scripts/bt":/tools:ro -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" "$IMG" bash -s <<'EOF'
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
# bullseye is past its support and deb.debian.org's security pool is half gone: install from the
# snapshot the image itself names (its sources.list carries commented snapshot.debian.org lines)
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
# asm-offsets.h: emit the running kernel's values instead of compiling 4.19.191's asm-offsets.c
python3 - <<'PY'
import re
src = open('/build/kh/include/generated/asm-offsets.h').read()
out = ['#include <linux/kbuild.h>', 'int main(void)', '{']
out += [f'  DEFINE({m[0]}, {m[1]});' for m in re.findall(r'^#define (\w+) (.+?)\s*$', src, re.M)
        if not m[0].startswith('__ASM_OFFSETS_H__')]
open('/build/src/arch/arm64/kernel/asm-offsets.c', 'w').write('\n'.join(out + ['  return 0;', '}']) + '\n')
# vdso-offsets.h: copy the running kernel's instead of building 4.19.191's vdso
p = '/build/src/arch/arm64/Makefile'
s = open(p).read()
s2 = re.sub(r'(vdso_prepare: prepare0\n)(?:\t.*\n)+',
            r'\1\t$(Q)cp /build/kh/include/generated/vdso-offsets.h include/generated/vdso-offsets.h\n', s)
assert s2 != s, 'vdso_prepare rule not found'
open(p, 'w').write(s2)
PY

MK="make -C $S ARCH=arm64 LLVM=1 CROSS_COMPILE=aarch64-linux-gnu- -j$(nproc)"
cp /config $S/.config
$S/scripts/config --file $S/.config --module BT_HCIVHCI
$MK olddefconfig >/dev/null 2>&1
diff <(grep -v '^#' /config | grep . | sort) <(grep -v '^#' $S/.config | grep . | sort) > $W/config.diff || true
echo "olddefconfig: $(grep -c '^[<>]' $W/config.diff) differing lines (this Kconfig tree is not Walton's; not used for the build, see below)"

echo "== modules_prepare"
$MK modules_prepare > $W/prepare.log 2>&1 || { grep -v Kconfig $W/prepare.log | tail -30; exit 1; }
# Struct layouts (and so CRCs) follow the CONFIG_ macros: the build must see the RUNNING kernel's
# configuration, not this tree's olddefconfig result.
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
open('/build/src/include/generated/autoconf.h', 'w').write(kh + '#define CONFIG_BT_HCIVHCI_MODULE 1\n')
open('/build/src/include/config/auto.conf', 'w').write('\n'.join(cfg + ['CONFIG_BT_HCIVHCI=m']) + '\n')
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

echo "== building hci_vhci.ko (pass 1: list the symbols it needs)"
$MK M=drivers/bluetooth modules > $W/pass1.log 2>&1 || { tail -30 $W/pass1.log; exit 1; }
llvm-nm -u $S/drivers/bluetooth/hci_vhci.o | awk '{print $2}' | grep -vx __this_module | sort -u > $W/needed.txt
echo "needs $(wc -l < $W/needed.txt) symbols"

echo "== CRCs from vendor modules"
python3 /tools/versions.py /vm/*.ko > $W/vendor-crcs.txt
echo "vendor __versions tables: $(wc -l < $W/vendor-crcs.txt) symbols"

echo "== CRCs computed by genksyms (4.19.127 headers + running config)"
: > $W/files.txt
for sym in $(cat $W/needed.txt); do
  grep -rlE --include='*.c' "^EXPORT_SYMBOL(_GPL)?\($sym\);" $S/kernel $S/mm $S/fs $S/lib $S/net \
    $S/drivers/base $S/drivers/char $S/arch/arm64 $S/block 2>/dev/null >> $W/files.txt || true
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

python3 /tools/merge_symvers.py $W/needed.txt $W/vendor-crcs.txt $W/crc-computed.txt $S/Module.symvers $W/crc-report.txt
cat $W/crc-report.txt

echo "== building hci_vhci.ko (pass 2: with Module.symvers)"
rm -f $S/drivers/bluetooth/hci_vhci.ko $S/drivers/bluetooth/hci_vhci.mod.* $S/drivers/bluetooth/hci_vhci.o
$MK M=drivers/bluetooth modules > $W/pass2.log 2>&1 || { tail -30 $W/pass2.log; exit 1; }
if grep -E 'undefined!|WARNING' $W/pass2.log; then echo "FAIL: modpost warnings"; exit 1; fi
K=$S/drivers/bluetooth/hci_vhci.ko
llvm-strip --strip-debug $K
cp $K /out/hci_vhci.ko
cp $W/crc-report.txt $W/config.diff /out/
modinfo /out/hci_vhci.ko | tee /out/modinfo.txt
python3 /tools/versions.py /out/hci_vhci.ko > /out/hci_vhci.versions.txt
VM_OURS=$(modinfo -F vermagic /out/hci_vhci.ko)
VM_VENDOR=$(modinfo -F vermagic /vm/wmt_drv.ko)
echo "vermagic ours  : $VM_OURS"; echo "vermagic vendor: $VM_VENDOR"
[ "$VM_OURS" = "$VM_VENDOR" ] || { echo "FAIL: vermagic mismatch"; exit 1; }
# every CRC in the module must come from the Module.symvers we wrote
python3 - <<'PY'
sv = {l.split('\t')[1]: int(l.split('\t')[0], 16) for l in open('/build/src/Module.symvers')}
for line in open('/out/hci_vhci.versions.txt'):
    s, c = line.split(); assert sv.get(s) == int(c, 16), f'CRC of {s} not from Module.symvers'
print('module __versions match Module.symvers')
PY
file /out/hci_vhci.ko
chown -R "$HOST_UID:$HOST_GID" /out
EOF
sha256sum "$OUT/hci_vhci.ko" | tee "$OUT/hci_vhci.ko.sha256"
