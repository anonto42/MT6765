#!/usr/bin/env bash
# Build wlan_drv_gen4m.ko (and only that module) for the tablet's running kernel, with
# CFG_SUPPORT_SNIFFER_RADIOTAP compiled in (monitor-mode / NL80211_IFTYPE_MONITOR support that the
# stock module has compiled out) -> build/wifi/wlan_drv_gen4m.ko
#
# This is the same header-overlay / CRC-cross-check pipeline as scripts/build_bt_module.sh (read
# that script's comment first) adapted for a much larger out-of-tree module:
#
#   1. vendor modules (kheaders.ko, wmt_drv.ko, wmt_chrdev_wifi.ko, wlan_drv_gen4m.ko, ...) are
#      pulled out of the stock `super` dump the same way build_bt_module.sh does, unless
#      VENDOR_MODULES already holds them;
#   2. the 4.19.191 tree is overlaid with the running kernel's real headers/config exactly as in
#      build_bt_module.sh -- CONFIG_MTK_WIFI_SNIFFER_RADIOTAP is NOT a Kconfig symbol (grep confirms
#      it is nowhere in Kconfig or in data/kernel/walpad-4.19.127.config: it gates
#      -DCFG_SUPPORT_SNIFFER_RADIOTAP purely via a `make` command-line variable in the gen4m
#      Makefile), so unlike CONFIG_BT_HCIVHCI this build needs NO .config / autoconf.h change at
#      all: CONFIG_MTK_COMBO_WIFI is already =y in both the data config and the running kernel's
#      autoconf.h, so the "data config == running kernel autoconf.h" check stays unmodified;
#   3. wlan_drv_gen4m is built directly via `M=drivers/.../wlan/core/gen4m`, the same way
#      hci_vhci is built via `M=drivers/bluetooth` -- this bypasses the normal in-tree recursion
#      through drivers/misc/mediatek/connectivity/Makefile, which is where the real build would
#      normally get MTK_COMBO_CHIP, WLAN_CHIP_ID, CONFIG_MTK_COMBO_WIFI_HIF, etc. as `export`ed make
#      variables (read that Makefile: `obj-$(CONFIG_MTK_COMBO_CHIP_CONSYS_6765) += wlan/core/gen4m/`
#      is guarded on exactly the symbol data/kernel/walpad-4.19.127.config already sets =y, i.e.
#      this is genuinely the chip/platform pairing the running kernel was built for). Since M=
#      skips that parent Makefile, EXTRA_VARS below passes the same variables on the command line:
#        MTK_COMBO_CHIP=CONNAC WLAN_CHIP_ID=6765 CONFIG_MTK_COMBO_WIFI_HIF=axi
#        BT_PLATFORM=connac1x MTK_ANDROID_WMT=y MTK_ANDROID_EMI=y WIFI_IP_SET=1 WIFI_ECO_VER=1
#        MTK_WLAN_SERVICE=yes
#      plus CONFIG_MTK_WIFI_SNIFFER_RADIOTAP=y, the one flag this build turns on that the stock
#      module doesn't have. The gen4m Makefile's object list (COMMON_OBJS/NIC_OBJS/OS_OBJS/...) does
#      NOT change with that flag -- it only toggles #ifdef CFG_SUPPORT_SNIFFER_RADIOTAP code inside
#      files already built (confirmed by grep: os/linux/gl_cfg80211.c, gl_init.c, gl_kal.c,
#      common/wlan_oid.c, nic/nic_rx.c, nic/radiotap.c, chips/common/cmm_asic_connac2x.c -- all
#      already in the object list regardless), so pass 1 (listing needed symbols) only needs the
#      flag on because turning it on can pull in *additional* undefined symbols from those same
#      files' new code paths (radiotap/skb helpers), not because the file set changes;
#   4. ~90 .c files means many more imported symbols than hci_vhci's one file: needed.txt is
#      harvested from the single incremental-linked drivers/.../gen4m/wlan_drv_gen4m.o (kbuild
#      links all of a multi-file module's objects into that one relocatable object before modpost,
#      the same shape hci_vhci.o has for its one file), then resolved the same CRC/genksyms way,
#      searched over a wider set of kernel source dirs (net/, drivers/net, crypto, etc. -- see
#      NEEDED_SEARCH_DIRS) but explicitly EXCLUDING drivers/misc/mediatek/connectivity: any symbol
#      gen4m needs from wmt_drv.ko / wmt_chrdev_wifi.ko (other vendor modules, not the core kernel)
#      must come from their __versions tables (vendor-crcs.txt), not be recomputed by genksyms
#      against our build tree's copy of their source, which may not reflect what's actually loaded;
#   5. pass 1 uses KBUILD_MODPOST_WARN=1 so modpost reports (rather than fails on) the undefined
#      symbols every pass-1 build has before Module.symvers exists -- gen4m's own Makefile forces
#      KBUILD_MODPOST_FAIL_ON_WARNINGS=y, unlike hci_vhci's;
#   6. vermagic of the final .ko is compared with wmt_drv.ko's, and every CRC in its __versions
#      table is checked against the Module.symvers actually used, exactly as build_bt_module.sh does.
#
# Env: KSRC (default tools/kernel-mt6765-4.19), SUPER_IMG (default build/android-restore/super-raw.img),
#      VENDOR_MODULES (directory with kheaders.ko + vendor .ko files; skips the super extraction).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
KSRC=$(realpath "${KSRC:-tools/kernel-mt6765-4.19}")
SUPER_IMG=${SUPER_IMG:-build/android-restore/super-raw.img}
OUT=$ROOT/build/wifi
VM=${VENDOR_MODULES:-$OUT/vendor-modules}
CONFIG=$ROOT/data/kernel/walpad-4.19.127.config
IMG=debian:bullseye

[ -f "$KSRC/drivers/misc/mediatek/connectivity/wlan/core/gen4m/Makefile" ] || {
  echo "kernel tree missing: $KSRC (git submodule update --init, or KSRC=...)"; exit 1; }
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
[ -f "$VM/wlan_drv_gen4m.ko" ] || { echo "no stock wlan_drv_gen4m.ko in $VM"; exit 1; }
echo "stock module (keep for rollback/diff): $VM/wlan_drv_gen4m.ko"

docker run --rm -i -v "$KSRC":/ksrc:ro -v "$VM":/vm:ro -v "$OUT":/out -v "$CONFIG":/config:ro \
  -v "$ROOT/scripts/bt":/tools:ro -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" "$IMG" bash -s <<'EOF'
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

echo "== correcting gen4m source-tree drift against the real shipped module"
# tools/kernel-mt6765-4.19 is a close-but-not-identical vendor tree (see build_bt_module.sh's
# header comment): this is true not just of the kernel headers/Kconfig but, it turns out, of the
# gen4m WLAN DRIVER SOURCE itself. CONFIG_MTK_CONNSYS_DEDICATED_LOG_PATH=y is genuinely set in the
# running kernel's data/kernel/walpad-4.19.127.config, and the sibling call
# wifi_fwlog_event_func_register() it gates IS present and resolvable in the stock
# wlan_drv_gen4m.ko's own __versions table -- but two things this exact checkout additionally
# compiles under the same/an adjacent guard are NOT: wifi_fwlog_get_fw_ver_register() (same ifdef
# block, gl_init.c) and the CFG_SUPPORT_ICS==1 block (config.h ties it to the same CONFIG_ symbol;
# wifi_ics_event_func_register()/wifi_ics_fwlog_write()). None of the 3 appear in the stock
# module's own __versions table, and the stock module's `depends:` is only wmt_drv,wmt_chrdev_wifi
# -- no adaptor module that could provide them is even loaded on this device. Compiling them in
# would produce a module that cannot resolve those symbols against anything actually present on
# the tablet, independent of monitor-mode support. So: force CFG_SUPPORT_ICS off (matching the
# real binary) and drop the one extra fwlog call its real-build sibling doesn't have. Neither
# touches CFG_SUPPORT_SNIFFER_RADIOTAP or any monitor-mode code path.
GEN4M=$S/drivers/misc/mediatek/connectivity/wlan/core/gen4m
python3 - "$GEN4M" <<'PY'
import sys
g = sys.argv[1]
p = f'{g}/include/config.h'
s = open(p).read()
old = ("#ifdef CONFIG_MTK_CONNSYS_DEDICATED_LOG_PATH\n"
       "#define CFG_SUPPORT_ICS                 1\n"
       "#else\n"
       "#define CFG_SUPPORT_ICS                 0\n"
       "#endif\n")
assert old in s, 'CFG_SUPPORT_ICS block not found verbatim'
open(p, 'w').write(s.replace(old, "#define CFG_SUPPORT_ICS                 0\n"))

p = f'{g}/os/linux/gl_init.c'
s = open(p).read()
for line in ('\twifi_fwlog_get_fw_ver_register(wlanReadRamCodeReleaseManifest);\n',
             '\twifi_fwlog_get_fw_ver_register(NULL);\n'):
    assert s.count(line) == 1, f'expected exactly one occurrence of: {line!r}'
    s = s.replace(line, '')
open(p, 'w').write(s)

# Device-test finding (owner request 2026-11-12): CFG_MODIFY_TX_POWER_BY_BAT_VOLT=1 (this tree's
# default) calls lbat_user_register(), a symbol no vendor module on the tablet imports or exports --
# so the earlier build's genksyms-computed CRC for it had no live module to cross-check against, and
# turned out wrong: insmod rejected the real module with "disagrees about version of symbol
# lbat_user_register" (err -22). No loaded vendor module needs it, so the real shipped build compiles
# this out too, same as CFG_SUPPORT_ICS above. Force it off; this is also unrelated to monitor mode.
p = f'{g}/include/config.h'
s = open(p).read()
old = '#define CFG_MODIFY_TX_POWER_BY_BAT_VOLT         1\n'
assert old in s, 'CFG_MODIFY_TX_POWER_BY_BAT_VOLT=1 not found verbatim'
open(p, 'w').write(s.replace(old, '#define CFG_MODIFY_TX_POWER_BY_BAT_VOLT         0\n'))
print('gen4m source-drift patch applied (config.h, gl_init.c)')
PY

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
diff <(grep -v '^#' /config | grep . | sort) <(grep -v '^#' $S/.config | grep . | sort) > $W/config.diff || true
echo "olddefconfig: $(grep -c '^[<>]' $W/config.diff) differing lines (this Kconfig tree is not Walton's; not used for the build, see below)"

echo "== modules_prepare"
$MK modules_prepare > $W/prepare.log 2>&1 || { grep -v Kconfig $W/prepare.log | tail -30; exit 1; }
# Struct layouts (and so CRCs) follow the CONFIG_ macros: the build must see the RUNNING kernel's
# configuration, not this tree's olddefconfig result. Unlike the BT build, CONFIG_MTK_WIFI_SNIFFER_RADIOTAP
# is a pure make-variable (not Kconfig), so nothing is added here: CONFIG_MTK_COMBO_WIFI is already
# =y in both the data config and the running kernel's autoconf.h -- this check must pass unmodified.
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
print(f'data/kernel config == running kernel autoconf.h ({len(want)} symbols, unmodified)')
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

MDIR=drivers/misc/mediatek/connectivity/wlan/core/gen4m
# Exact make variables drivers/misc/mediatek/connectivity/Makefile normally `export`s before
# recursing into wlan/core/gen4m (read there: "for gen4m options"), plus the one flag this build
# adds. M= bypasses that parent Makefile so they must be given on the command line.
WIFI_VARS="MTK_COMBO_CHIP=CONNAC WLAN_CHIP_ID=6765 CONFIG_MTK_COMBO_WIFI_HIF=axi BT_PLATFORM=connac1x MTK_ANDROID_WMT=y MTK_ANDROID_EMI=y WIFI_IP_SET=1 WIFI_ECO_VER=1 MTK_WLAN_SERVICE=yes CONFIG_MTK_WIFI_SNIFFER_RADIOTAP=y"

echo "== building wlan_drv_gen4m.ko (pass 1: list the symbols it needs)"
$MK $WIFI_VARS KBUILD_MODPOST_WARN=1 M=$MDIR modules > $W/pass1.log 2>&1 || { tail -60 $W/pass1.log; exit 1; }
# Undefined symbols come in two ELF bindings: 'U' (strong -- modpost/insmod must resolve it, needs
# a CRC) and 'w' (weak -- e.g. get_wifi_standalone_log_mode, declared
# `extern ... __attribute__((weak))` in include/debug.h: the kernel module loader leaves an
# unresolved weak symbol as NULL rather than refusing to load, and modpost does not require a
# modversion CRC for one. Confirmed absent from the stock wlan_drv_gen4m.ko's own __versions table
# for exactly this reason. Only strong-undefined symbols need resolving here.
llvm-nm -u $S/$MDIR/wlan_drv_gen4m.o | awk '$1 == "U" {print $2}' | grep -vx __this_module | sort -u > $W/needed.txt
llvm-nm -u $S/$MDIR/wlan_drv_gen4m.o | awk '$1 == "w" {print $2}' | sort -u > $W/needed-weak.txt
echo "-- monitor-mode symbol check in pass-1 .o (diagnostic)"
llvm-nm $S/$MDIR/os/linux/gl_cfg80211.o 2>&1 | grep -w 'mtk_init_monitor_role\|mtk_uninit_monitor_role\|mtk_cfg80211_set_monitor_channel' || echo "NOT FOUND in gl_cfg80211.o"
grep -o '\-D *CFG_SUPPORT_SNIFFER_RADIOTAP\|CFG_SUPPORT_SNIFFER_RADIOTAP' $S/$MDIR/os/linux/.gl_cfg80211.o.cmd 2>/dev/null | sort -u || echo "no CFG_SUPPORT_SNIFFER_RADIOTAP in gl_cfg80211.o.cmd"
echo "needs $(wc -l < $W/needed.txt) symbols ($(wc -l < $W/needed-weak.txt) more are weak-undefined, left unresolved like the stock build)"

echo "== CRCs from vendor modules"
python3 /tools/versions.py /vm/*.ko > $W/vendor-crcs.txt
echo "vendor __versions tables (what each vendor .ko imports): $(wc -l < $W/vendor-crcs.txt) symbols"
# gen4m, unlike hci_vhci, genuinely cross-links against symbols EXPORTED BY other vendor .ko files
# (wmt_drv.ko, wmt_chrdev_wifi.ko) rather than only the core kernel. Those don't show up in any
# module's __versions table (that only lists what a module ITSELF imports) -- the authoritative CRC
# for a symbol a vendor .ko exports is the value of its own absolute `__crc_<sym>` symtab entry
# (CONFIG_MODULE_REL_CRCS is not set for this kernel, confirmed against data/kernel/walpad-4.19.127.config,
# so that value IS the CRC directly, not a section-relative offset). Cross-checked against
# mtk_wcn_wmt_wlan_reg, which resolves via both paths and agrees (0xf6f3f906) -- confirms this is
# reading the same authoritative numbers __versions does, just from the exporting side.
python3 - <<'PY' >> $W/vendor-crcs.txt
import struct, glob
def crc_exports(path):
    d = open(path, 'rb').read()
    shoff, = struct.unpack_from('<Q', d, 0x28)
    shentsize, shnum, shstrndx = struct.unpack_from('<HHH', d, 0x3A)
    def sh(i): return struct.unpack_from('<IIQQQQIIQQ', d, shoff + i * shentsize)
    stroff = sh(shstrndx)[4]
    def shname(off):
        end = d.index(b'\0', stroff + off); return d[stroff + off:end].decode()
    symtab_off = symtab_size = symtab_link = None
    for i in range(shnum):
        n_off, typ, flags, addr, off, size, link, info, align, entsz = sh(i)
        if shname(n_off) == '.symtab':
            symtab_off, symtab_size, symtab_link = off, size, link
    if symtab_off is None:
        return {}
    strtab_off = sh(symtab_link)[4]
    out = {}
    for e in range(symtab_off, symtab_off + symtab_size, 24):
        st_name, st_info, st_other, st_shndx = struct.unpack_from('<IBBH', d, e)
        st_value, st_size = struct.unpack_from('<QQ', d, e + 8)
        end = d.index(b'\0', strtab_off + st_name)
        nm = d[strtab_off + st_name:end].decode(errors='replace')
        if nm.startswith('__crc_'):
            out[nm[6:]] = st_value & 0xFFFFFFFF
    return out
for f in sorted(glob.glob('/vm/*.ko')):
    for sym, crc in crc_exports(f).items():
        print(f'{sym} {crc:#010x}')
PY
echo "vendor CRCs total (imports + exports): $(wc -l < $W/vendor-crcs.txt) lines"
# merge_symvers.py always writes "vmlinux" as the resolving module (it has no notion of a vendor
# .ko owning a symbol) -- record real ownership separately so pass 2's Module.symvers can be
# corrected before the build: modpost reads this column straight into the final module's
# `depends:` modinfo field, and the stock wlan_drv_gen4m.ko has `depends: wmt_drv,wmt_chrdev_wifi`
# for exactly this reason (mtk_wcn_wmt_wlan_reg and friends are real cross-module calls, not core
# kernel ones). Skip wlan_drv_gen4m.ko itself (that would be a self-dependency, not a real one).
python3 - <<'PY' > $W/vendor-owner.txt
import struct, glob, os
def crc_exports(path):
    d = open(path, 'rb').read()
    shoff, = struct.unpack_from('<Q', d, 0x28)
    shentsize, shnum, shstrndx = struct.unpack_from('<HHH', d, 0x3A)
    def sh(i): return struct.unpack_from('<IIQQQQIIQQ', d, shoff + i * shentsize)
    stroff = sh(shstrndx)[4]
    def shname(off):
        end = d.index(b'\0', stroff + off); return d[stroff + off:end].decode()
    symtab_off = symtab_size = symtab_link = None
    for i in range(shnum):
        n_off, typ, flags, addr, off, size, link, info, align, entsz = sh(i)
        if shname(n_off) == '.symtab':
            symtab_off, symtab_size, symtab_link = off, size, link
    if symtab_off is None:
        return []
    strtab_off = sh(symtab_link)[4]
    out = []
    for e in range(symtab_off, symtab_off + symtab_size, 24):
        st_name, st_info, st_other, st_shndx = struct.unpack_from('<IBBH', d, e)
        end = d.index(b'\0', strtab_off + st_name)
        nm = d[strtab_off + st_name:end].decode(errors='replace')
        if nm.startswith('__crc_'):
            out.append(nm[6:])
    return out
for f in sorted(glob.glob('/vm/*.ko')):
    mod = os.path.basename(f)[:-3]
    if mod == 'wlan_drv_gen4m':
        continue
    for sym in crc_exports(f):
        print(f'{sym} {mod}')
PY

echo "== CRCs computed by genksyms (4.19.127 headers + running config)"
# Search the core kernel tree for EXPORT_SYMBOL, explicitly NOT drivers/misc/mediatek/connectivity:
# any symbol gen4m needs from there is from another vendor .ko (wmt_drv, wmt_chrdev_wifi) and must
# come from vendor-crcs.txt, not be recomputed against our build tree's copy of their source.
: > $W/files.txt
for sym in $(cat $W/needed.txt); do
  grep -rlE --include='*.c' "^EXPORT_SYMBOL(_GPL)?\($sym\);" $S/kernel $S/mm $S/fs $S/lib $S/net \
    $S/drivers/base $S/drivers/char $S/arch/arm64 $S/block $S/crypto $S/drivers/net $S/drivers/rtc \
    $S/security $S/sound $S/drivers/of $S/drivers/misc/mediatek --exclude-dir=connectivity 2>/dev/null >> $W/files.txt || true
done
# bpf_trace_run1..N are generated by a BPF_TRACE_DEFN_x(x) macro in kernel/trace/bpf_trace.c
# (EXPORT_SYMBOL_GPL(bpf_trace_run##x)) -- the literal name never appears in the raw .c source, so
# the grep above can never find it there; force it in when any bpf_trace_run* symbol is needed, so
# genksyms sees it after macro expansion (in the preprocessed .i) instead of reporting "no CRC".
grep -q '^bpf_trace_run' $W/needed.txt && echo $S/kernel/trace/bpf_trace.c >> $W/files.txt
sort -u -o $W/files.txt $W/files.txt
: > $W/crc-computed.txt
while read -r f; do
  rel=${f#$S/}; i=${rel%.c}.i
  if $MK $WIFI_VARS KCFLAGS=-D__GENKSYMS__ "$i" > $W/i.log 2>&1; then
    $S/scripts/genksyms/genksyms < $S/$i 2>/dev/null \
      | sed -n 's/^__crc_\([A-Za-z0-9_]*\) = \(0x[0-9a-f]*\).*/\1 \2/p' >> $W/crc-computed.txt
    echo "   $rel"
  else
    echo "   (cannot preprocess $rel; its symbols rely on vendor CRCs)"
  fi
done < $W/files.txt
sort -u -o $W/crc-computed.txt $W/crc-computed.txt

# genksyms recomputes a CRC from THIS checkout's (4.19.191-vintage) copy of the exporting .c file,
# not the exact source the running 4.19.127 kernel was actually built from, so a genuine mismatch
# between vendor (authoritative -- read from a module that loads today) and computed is possible
# for reasons having nothing to do with gen4m itself (e.g. an upstream sched.c revision between
# vintages changing a prototype's referenced types). merge_symvers.py rightly treats any such
# mismatch as fatal by default -- it is not something to wave through blind. But when the disputed
# vendor CRC is independently confirmed by the STOCK wlan_drv_gen4m.ko's own __versions table (i.e.
# the real, currently-loaded driver already relies on exactly that CRC for that symbol), that is
# about as strong a confirmation as this method can produce without the device itself, so drop the
# conflicting computed candidate and let vendor win for that symbol alone -- not a blanket
# vendor-always-wins rule, only for symbols the stock module itself is already proof of.
python3 - <<'PY'
import struct
def crc_versions(path):
    d = open(path, 'rb').read()
    shoff, = struct.unpack_from('<Q', d, 0x28)
    shentsize, shnum, shstrndx = struct.unpack_from('<HHH', d, 0x3A)
    def sh(i): return struct.unpack_from('<IIQQQQIIQQ', d, shoff + i * shentsize)
    stroff = sh(shstrndx)[4]
    out = {}
    for i in range(shnum):
        name_off, _, _, _, off, size, *_ = sh(i)
        end = d.index(b'\0', stroff + name_off)
        if d[stroff + name_off:end] == b'__versions':
            for e in range(off, off + size, 64):
                crc, = struct.unpack_from('<Q', d, e)
                name = d[e + 8:e + 64].split(b'\0', 1)[0].decode()
                out[name] = crc & 0xFFFFFFFF
    return out
stock = crc_versions('/vm/wlan_drv_gen4m.ko')
vendor = {}
for l in open('/build/vendor-crcs.txt'):
    n, c = l.split(); vendor[n] = int(c, 16)
lines = open('/build/crc-computed.txt').read().splitlines()
keep, dropped = [], []
for l in lines:
    n, c = l.split(); c = int(c, 16)
    if n in vendor and vendor[n] != c and stock.get(n) == vendor[n]:
        dropped.append(n)
        continue
    keep.append(l)
open('/build/crc-computed.txt', 'w').write('\n'.join(keep) + ('\n' if keep else ''))
if dropped:
    print('vendor CRC confirmed by stock module, computed candidate dropped:', dropped)
PY

python3 /tools/merge_symvers.py $W/needed.txt $W/vendor-crcs.txt $W/crc-computed.txt $S/Module.symvers $W/crc-report.txt
cat $W/crc-report.txt

# merge_symvers.py (shared with build_bt_module.sh, which never needed this) writes every row's
# module column as "vmlinux", even for symbols a vendor .ko exports -- modpost reads this column
# straight into the final module's `depends:`, so left uncorrected the built module would claim no
# dependency on wmt_drv/wmt_chrdev_wifi at all (confirmed: an uncorrected build's modinfo shows
# `depends: ` empty, where stock's is `wmt_drv,wmt_chrdev_wifi`). Fix the column for exactly the
# symbols vendor-owner.txt says a vendor .ko exports, before pass 2 runs modpost against this file.
python3 - <<'PY'
owner = {}
for l in open('/build/vendor-owner.txt'):
    sym, mod = l.split()
    owner[sym] = mod
out = []
fixed = set()
for l in open('/build/src/Module.symvers'):
    crc, sym, mod, kind, *rest = l.rstrip('\n').split('\t')
    if sym in owner and mod == 'vmlinux':
        mod = owner[sym]
        fixed.add(mod)
    out.append('\t'.join([crc, sym, mod, kind] + rest))
open('/build/src/Module.symvers', 'w').write('\n'.join(out) + '\n')
print('Module.symvers module column corrected for vendor-owned symbols, depends now:', sorted(fixed))
PY

echo "== building wlan_drv_gen4m.ko (pass 2: with Module.symvers)"
rm -f $S/$MDIR/wlan_drv_gen4m.ko $S/$MDIR/wlan_drv_gen4m.mod.* $S/$MDIR/wlan_drv_gen4m.o $S/$MDIR/*.o
$MK $WIFI_VARS M=$MDIR modules > $W/pass2.log 2>&1 || { tail -60 $W/pass2.log; exit 1; }
if grep -E 'undefined!|WARNING' $W/pass2.log; then echo "FAIL: modpost warnings"; exit 1; fi
K=$S/$MDIR/wlan_drv_gen4m.ko
llvm-strip --strip-debug $K
cp $K /out/wlan_drv_gen4m.ko
cp $W/crc-report.txt $W/config.diff /out/
modinfo /out/wlan_drv_gen4m.ko | tee /out/modinfo.txt
python3 /tools/versions.py /out/wlan_drv_gen4m.ko > /out/wlan_drv_gen4m.versions.txt
VM_OURS=$(modinfo -F vermagic /out/wlan_drv_gen4m.ko)
VM_VENDOR=$(modinfo -F vermagic /vm/wmt_drv.ko)
echo "vermagic ours  : $VM_OURS"; echo "vermagic vendor: $VM_VENDOR"
[ "$VM_OURS" = "$VM_VENDOR" ] || { echo "FAIL: vermagic mismatch"; exit 1; }
python3 - <<'PY'
sv = {l.split('\t')[1]: int(l.split('\t')[0], 16) for l in open('/build/src/Module.symvers')}
for line in open('/out/wlan_drv_gen4m.versions.txt'):
    s, c = line.split(); assert sv.get(s) == int(c, 16), f'CRC of {s} not from Module.symvers'
print('module __versions match Module.symvers')
PY
echo "== monitor-mode code presence (sanity check, no device needed)"
# Reads the ELF .symtab directly (name presence only, regardless of ELF section quirks a
# particular nm build/mount combination might trip on) instead of shelling out to llvm-nm, which
# proved unreliable against the bind-mounted /out path in earlier runs of this script (silently
# empty output) despite the same file correctly round-tripping through modinfo/vermagic/CRC checks
# moments before -- confirmed independently afterwards by parsing the copied-out .ko on the host.
python3 - <<'PY'
import struct
def symtab_names(path):
    d = open(path, 'rb').read()
    shoff, = struct.unpack_from('<Q', d, 0x28)
    shentsize, shnum, shstrndx = struct.unpack_from('<HHH', d, 0x3A)
    def sh(i): return struct.unpack_from('<IIQQQQIIQQ', d, shoff + i * shentsize)
    stroff = sh(shstrndx)[4]
    def shname(off):
        end = d.index(b'\0', stroff + off); return d[stroff + off:end].decode()
    symtab_off = symtab_size = symtab_link = None
    for i in range(shnum):
        n_off, typ, flags, addr, off, size, link, info, align, entsz = sh(i)
        if shname(n_off) == '.symtab':
            symtab_off, symtab_size, symtab_link = off, size, link
    if symtab_off is None:
        return set()
    strtab_off = sh(symtab_link)[4]
    out = set()
    for e in range(symtab_off, symtab_off + symtab_size, 24):
        st_name, = struct.unpack_from('<I', d, e)
        end = d.index(b'\0', strtab_off + st_name)
        out.add(d[strtab_off + st_name:end].decode(errors='replace'))
    return out
ours = symtab_names('/out/wlan_drv_gen4m.ko')
stock = symtab_names('/vm/wlan_drv_gen4m.ko')
for sym in ('mtk_init_monitor_role', 'mtk_uninit_monitor_role', 'mtk_cfg80211_set_monitor_channel'):
    print(f'{sym}: ours={"yes" if sym in ours else "no"}  stock={"yes" if sym in stock else "no"}')
PY
file /out/wlan_drv_gen4m.ko
chown -R "$HOST_UID:$HOST_GID" /out
EOF
sha256sum "$OUT/wlan_drv_gen4m.ko" | tee "$OUT/wlan_drv_gen4m.ko.sha256"
