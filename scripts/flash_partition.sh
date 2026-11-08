#!/usr/bin/env bash
# Write an image to a partition with mtkclient, read it back, compare, and reset only on a match.
#   scripts/flash_partition.sh <partition> <image> [--no-reset]
# Start it FIRST, then put the tablet in the preloader window (unplug/replug the USB cable with no
# buttons held, or hold power 15 s).  Result lines go to build/flash/<partition>.result.
# Needs explicit user approval for every run (it writes flash).  One mtkclient process at a time.
set -uo pipefail
cd "$(dirname "$0")/.."
PART=${1:?partition}; IMG=${2:?image}; RESET=1; [ "${3:-}" = "--no-reset" ] && RESET=0
PY=tools/mtk/.venv/bin/python; MTK=tools/mtk/mtk.py
D=build/flash; mkdir -p "$D"; R="$D/$PART.result"; : > "$R"
RB="$D/$PART.readback.img"; rm -f "$RB"
timeout 900 $PY $MTK w "$PART" "$IMG" > "$D/$PART.write.log" 2>&1
echo "WRITE_RC=$?" >> "$R"
if ! grep -q "Wrote .* to sector" "$D/$PART.write.log"; then echo NOWRITE >> "$R"; echo DONE >> "$R"; exit 1; fi
echo WROTE >> "$R"
timeout 600 $PY $MTK r "$PART" "$RB" > "$D/$PART.read.log" 2>&1
echo "READ_RC=$?" >> "$R"
if cmp -s "$RB" "$IMG"; then
  echo MATCH >> "$R"
  if [ "$RESET" = 1 ]; then timeout 120 $PY $MTK reset > "$D/$PART.reset.log" 2>&1; echo "RESET_RC=$?" >> "$R"; fi
else
  echo MISMATCH >> "$R"
fi
echo DONE >> "$R"
