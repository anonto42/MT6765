#!/bin/sh
# M3.4: run ON the tablet after switching runtime.conf to rust (WALPAD_DEVICE and/or WALPAD_INPUT).
#   ssh root@<ip> sh -s < scripts/device/m3_check.sh
# One PASS/FAIL line per check; exit status = number of failures.  Manual checklist follows the output.
FAIL=0
ok()  { echo "PASS $1"; }
bad() { echo "FAIL $1${2:+ ($2)}"; FAIL=$((FAIL+1)); }
chk() { n=$1; shift; if "$@" >/dev/null 2>&1; then ok "$n"; else bad "$n"; fi; }
conf() { sed -n "s/^$1=//p" /etc/walpad/runtime.conf 2>/dev/null | tail -n 1 | tr -d ' \r'; }
DEV=$(conf WALPAD_DEVICE); INP=$(conf WALPAD_INPUT)
echo "runtime.conf: device=$DEV input=$INP shell=$(conf WALPAD_SHELL) shadow=$(conf WALPAD_SHADOW)"

chk "walpad-core running" pidof walpad-core
chk "walpad-supervise core running" pgrep -f "walpad-supervise core"
[ -e /run/walpad/fallback-core ] && bad "no fallback flag" "core fell back to Python this boot" || ok "no fallback flag"
chk "control.sock exists" test -S /run/walpad/control.sock
[ "$(stat -c %a /run/walpad/control.sock 2>/dev/null)" = 600 ] && ok "control.sock mode 0600" || bad "control.sock mode 0600"
chk "walpad-ctl-rs status" walpad-ctl-rs status
if [ "$DEV" = rust ]; then
  pidof -x walpad-devd.py >/dev/null 2>&1 || ! pgrep -f walpad-devd >/dev/null && ok "python devd not running" || bad "python devd not running" "duplicate device daemon"
fi
if [ "$INP" = rust ]; then
  ! pgrep -f 'walpad-(keys|touchd)' >/dev/null && ok "python keys/touchd not running" || bad "python keys/touchd not running" "duplicate input handling"
fi
o=$(cat /run/walpad/orientation 2>/dev/null); case $o in 0|90|180|270) ok "orientation file ($o)";; *) bad "orientation file" "$o";; esac
b=$(cat /sys/class/leds/lcd-backlight/brightness 2>/dev/null); [ -n "$b" ] && ok "backlight readable ($b)" || bad "backlight readable"
if [ -f /run/walpad/status.json ]; then
  age=$(( $(date +%s) - $(stat -c %Y /run/walpad/status.json) ))
  [ "$age" -lt 5 ] && ok "status.json age ${age}s" || bad "status.json age" "${age}s"
else bad "status.json" missing; fi
chk "/run/walpad-ok exists" test -f /run/walpad-ok
chk "Xvfb + fbmirror + openbox" sh -c 'pidof Xvfb && pidof fbmirror && pidof openbox'
[ -n "$(grep -c . /var/log/walpad-core.log 2>/dev/null)" ] && echo "--- last core log lines:" && tail -n 8 /var/log/walpad-core.log 2>/dev/null

cat <<'MANUAL'
--- manual checklist (tick each, report any that fail):
 [ ] touch moves the pointer where the finger is, in portrait AND after rotating (accuracy as before or better)
 [ ] rotate the tablet 4 ways: picture, touch and the work area follow; note if the direction is mirrored (rotate_flip)
 [ ] idle: dims after ~90 s, off after the timeout; touch wakes; a volume press that wakes does nothing else
 [ ] power: short press = screen off/on; long press = power menu; short press inside the menu activates the focused item
 [ ] volume: single/double/triple tap gestures as in /etc/walpad/keys.json; both buttons = rotation lock toggle
 [ ] cover closed = screen off, opened = on
 [ ] Home (vol down long) raises the launcher; vol up long toggles the on-screen keyboard
 [ ] kill -9 $(pidof walpad-core): it restarts within ~2 s and nothing else breaks
 [ ] rollback test: set the line back to python, reboot, smoke.sh passes
MANUAL
exit $FAIL
