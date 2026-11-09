#!/bin/sh
# apply.sh ROOT   turn a copy of config/rootfs (already in ROOT) into the no-Python tree, then
# verify that no Python is left.  Used by scripts/build_rootfs.sh (inside the build container) and
# by tests/shell/run.sh.  POSIX sh.
set -eu
ROOT=${1:?usage: apply.sh ROOT}
D=$(cd "$(dirname "$0")" && pwd)
grep -v '^#' "$D/REMOVE" | while read -r p; do
  [ -n "$p" ] || continue
  [ -e "$ROOT/$p" ] || { echo "apply.sh: REMOVE entry missing from the image: $p" >&2; exit 1; }
  rm -f "$ROOT/$p"
done
cp -a "$D/files/." "$ROOT/"
# Nothing we ship from config/ may mention Python, except walpad-runtime, whose `python` output is
# the "not available" answer that walpad-rust-check turns into a rescue.  Only our own files are
# scanned (not the Alpine packages, e.g. iproute2 has a script that mentions it).
CFG=${WALPAD_CFG_DIR:-$D/../rootfs}
left=$( { (cd "$CFG" && find . -type f); (cd "$D/files" && find . -type f); } | sort -u | while read -r f; do
  [ -f "$ROOT/$f" ] || continue
  case $f in ./usr/local/sbin/walpad-runtime|./usr/local/lib/*) continue ;; esac
  if grep -qIE 'python|\.py([^a-z]|$)' "$ROOT/$f"; then echo "$f"; fi
done; true )
if [ -n "$left" ]; then echo "apply.sh: Python still referenced in: $left" >&2; exit 1; fi
