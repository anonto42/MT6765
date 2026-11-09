#!/usr/bin/env bash
# M1.6: run the device-like session on the host: Xvfb :5 (1280x1280x24, like the tablet), openbox
# with the device config (if installed), walpad-core --fake and walpad-shell.  Saves a screenshot.
#   scripts/dev/host_session.sh [--keep] [--watch]
#   --keep   leave the session running (prints DISPLAY and socket); stop with `kill $(cat build/host-session/pids)`
#   --watch  also open Xephyr :6 showing the 800x1280 panel area (needs a desktop)
set -euo pipefail
cd "$(dirname "$0")/../.."
OUT=build/host-session; mkdir -p "$OUT"
KEEP=0; for a in "$@"; do [ "$a" = "--keep" ] && KEEP=1; done
for t in Xvfb xwd convert; do command -v $t >/dev/null || { echo "missing tool: $t" >&2; exit 1; }; done

# host (debug) binaries from the pinned toolchain
scripts/rust.sh build -p walpad-core -p walpad-shell -p walpad-ctl >/dev/null
BIN=target/debug

ROOT=$PWD/$OUT/root; rm -rf "$ROOT"; mkdir -p "$ROOT"
export WALPAD_ROOT=$ROOT DISPLAY=:5
: > "$OUT/pids"
cleanup() { [ $KEEP -eq 1 ] || { kill $(cat "$OUT/pids" 2>/dev/null) 2>/dev/null || true; }; }
trap cleanup EXIT
spawn() { "$@" >>"$OUT/session.log" 2>&1 & echo $! >> "$OUT/pids"; }

: > "$OUT/session.log"
spawn Xvfb :5 -screen 0 1280x1280x24 -nolisten tcp -ac
for _ in $(seq 50); do [ -S /tmp/.X11-unix/X5 ] && break; sleep 0.1; done
[ -S /tmp/.X11-unix/X5 ] || { echo "Xvfb did not start" >&2; exit 1; }
if command -v openbox >/dev/null; then
  spawn openbox --config-file config/rootfs/etc/walpad/openbox-rc.xml
else echo "note: openbox not installed; running without a window manager" >&2; fi
spawn "$BIN/walpad-core" --fake
for _ in $(seq 50); do [ -S "$ROOT/run/walpad/control.sock" ] && break; sleep 0.1; done
spawn env WALPAD_X_SOCKET=/tmp/.X11-unix/X5 WALPAD_FONTS="$PWD/crates/walpad-ui/assets/fonts" "$BIN/walpad-shell"
for _ in $(seq 60); do [ -f "$ROOT/run/walpad-ok" ] && break; sleep 0.1; done
if [ -f "$ROOT/run/walpad-ok" ]; then echo "shell wrote /run/walpad-ok"; else echo "WARNING: shell did not write /run/walpad-ok (see $OUT/session.log)" >&2; fi
"$BIN/walpad-ctl" status > "$OUT/status.json"
xwd -root -display :5 | convert xwd:- "png:$OUT/screenshot.png"
echo "screenshot: $OUT/screenshot.png  status: $OUT/status.json  log: $OUT/session.log"
[ $KEEP -eq 1 ] && echo "session kept: DISPLAY=:5 WALPAD_ROOT=$ROOT"
exit 0
