#!/bin/sh
# M8.2 measurement: CPU use of fbmirror with the screen on (idle) and with the screen off.
# Run ON the tablet as root (busybox sh), e.g.:   ssh root@<ip> sh -s < scripts/device/measure_fbmirror.sh
#   or copy it over and run it.  Takes about 2 x DURATION + 10 s (default 130 s).  Leave the tablet untouched
#   (no touch, no keys, nothing moving on screen) while it runs.
# It switches the screen with the device daemon (echo "screen off" > /run/walpad/devd.cmd) and always switches it
# back on at the end.  Nothing else is changed.  It prints measured numbers only; it never decides PASS/FAIL.
# Env: DURATION=60 (seconds per phase), FBMIRROR_PID=<pid> (override pidof), SKIP_SCREEN=1 (do not touch the
# screen, measure the current state twice; for testing the script off the tablet).
DURATION=${DURATION:-60}
SKIP_SCREEN=${SKIP_SCREEN:-0}
CMD=/run/walpad/devd.cmd
STATUS=/run/walpad/status.json
THRESH10=50   # 5.0 % of one core, in tenths of a percent

PID=${FBMIRROR_PID:-$(pidof fbmirror 2>/dev/null | awk '{print $1}')}
[ -n "$PID" ] && [ -r /proc/$PID/stat ] || { echo "fbmirror is not running (pidof fbmirror empty)"; exit 2; }
HZ=$(getconf CLK_TCK 2>/dev/null); HZ=${HZ:-100}

# utime+stime in clock ticks (fields 14 and 15; strip "pid (comm)" first because comm may contain spaces)
ticks() { sed 's/^.*) //' /proc/$PID/stat | awk '{print $12 + $13}'; }
screen_state() { sed -n 's/.*"screen" *: *"\([a-z]*\)".*/\1/p' $STATUS 2>/dev/null | head -n1; }
set_screen() {   # $1 = on|off
  [ "$SKIP_SCREEN" = 1 ] && return 0
  [ -p $CMD ] || [ -e $CMD ] || { echo "no $CMD: is the device daemon running?"; return 1; }
  echo "screen $1" > $CMD
}
restore() { [ "$SKIP_SCREEN" = 1 ] || echo "screen on" > $CMD 2>/dev/null; }
trap restore EXIT INT TERM

# measure <label>: prints "label state ticks seconds cpu%"
measure() {
  t0=$(ticks); s0=$(date +%s); sleep "$DURATION"
  t1=$(ticks); s1=$(date +%s)
  [ -r /proc/$PID/stat ] || { echo "fbmirror (pid $PID) exited during the measurement"; exit 3; }
  dt=$((t1 - t0)); secs=$((s1 - s0))
  pct10=$(( dt * 1000 / (HZ * secs) ))
  state=$(screen_state); state=${state:-unknown}
  printf '%-10s %-8s %8s %8s %6d.%d\n' "$1" "$state" "$dt" "$secs" $((pct10 / 10)) $((pct10 % 10))
  eval "PCT10_$1=$pct10"
}

echo "fbmirror pid $PID, CLK_TCK $HZ, $DURATION s per phase"
echo "binary: $(readlink /proc/$PID/exe 2>/dev/null)  size: $(stat -c %s /proc/$PID/exe 2>/dev/null) bytes"
set_screen on; sleep 5   # known start state, let the panel and X settle
printf '%-10s %-8s %8s %8s %8s\n' phase screen ticks seconds cpu%
measure screen_on
set_screen off; sleep 5  # status.json is rewritten every 2 s, fbmirror polls it every 250 ms
measure screen_off
set_screen on

# screen_on and screen_off are set by measure() through eval
on10=$PCT10_screen_on; off10=$PCT10_screen_off
echo
echo "screen on : $((on10 / 10)).$((on10 % 10)) % of one core"
echo "screen off: $((off10 / 10)).$((off10 % 10)) % of one core"
if [ "$on10" -gt "$THRESH10" ]; then v=yes; else v=no; fi
echo "M8.2 needed: $v (threshold 5% of one core with the screen on)"
echo "Note: the 'screen' column is read from $STATUS after each phase; if it says unknown or the wrong state the phase is not valid."
