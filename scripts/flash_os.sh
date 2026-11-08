#!/usr/bin/env bash
# One mtkclient session: write the Alpine root filesystem to userdata and the boot image to boot_a,
# read both back, compare, reset only if everything matches.
#   scripts/flash_os.sh <rootfs.ext4> <boot_a image>
# Start it FIRST, then put the tablet in the preloader window (unplug/replug the cable, no buttons).
# Progress/results: build/flash/os.result.   Needs explicit user approval (writes flash).
set -uo pipefail
cd "$(dirname "$0")/.."
ROOTFS=${1:?rootfs image}; BOOT=${2:?boot image}
PY=tools/mtk/.venv/bin/python; MTK=tools/mtk/mtk.py
D=build/flash; mkdir -p "$D"; R="$D/os.result"; : > "$R"
step() { echo "$1" >> "$R"; }
RSIZE=$(stat -c %s "$ROOTFS")
timeout 1500 $PY $MTK w userdata "$ROOTFS" > "$D/os.userdata.write.log" 2>&1
grep -q "Wrote .* to sector" "$D/os.userdata.write.log" || { step "USERDATA_WRITE_FAILED"; step DONE; exit 1; }
step USERDATA_WRITTEN
rm -f "$D/os.userdata.rb"
timeout 900 $PY $MTK ro 5175771136 "$RSIZE" "$D/os.userdata.rb" > "$D/os.userdata.read.log" 2>&1   # userdata starts at byte 0x134800000
if [ ! -s "$D/os.userdata.rb" ] || [ "$(stat -c %s "$D/os.userdata.rb")" -lt "$RSIZE" ]; then step USERDATA_VERIFY_SKIPPED_readback_failed
elif cmp -n "$RSIZE" "$D/os.userdata.rb" "$ROOTFS" > /dev/null 2>&1; then step USERDATA_VERIFIED
else step USERDATA_VERIFY_MISMATCH; step DONE; exit 1; fi
rm -f "$D/os.userdata.rb"
timeout 600 $PY $MTK w boot_a "$BOOT" > "$D/os.boot.write.log" 2>&1
grep -q "Wrote .* to sector" "$D/os.boot.write.log" || { step "BOOT_WRITE_FAILED"; step DONE; exit 1; }
step BOOT_WRITTEN
rm -f "$D/os.boot.rb"
timeout 600 $PY $MTK r boot_a "$D/os.boot.rb" > "$D/os.boot.read.log" 2>&1
if cmp -s "$D/os.boot.rb" "$BOOT"; then step BOOT_VERIFIED
else step BOOT_VERIFY_FAILED; step DONE; exit 1; fi
timeout 120 $PY $MTK reset > "$D/os.reset.log" 2>&1
step "RESET_RC=$?"
step DONE
