#!/bin/sh
# M6.4: run ON the tablet with walpad-core (and ideally walpad-shell) running.
#   ssh root@<ip> sh -s < scripts/device/m6_check.sh
# One PASS/FAIL line per check; exit status = number of failures.  Manual touch-only checklist follows.
# NOTE: this script launches and then kills the browser (kill -9) to test crash recovery.
CTL=walpad-ctl-rs
FAIL=0
ok()  { echo "PASS $1"; }
bad() { echo "FAIL $1${2:+ ($2)}"; FAIL=$((FAIL+1)); }
chk() { n=$1; shift; if "$@" >/dev/null 2>&1; then ok "$n"; else bad "$n"; fi; }
running() { $CTL status 2>/dev/null | tr -d ' \n' | grep -q "\"app\":\"$1\",\"running\":true"; }
listed()  { $CTL status 2>/dev/null | tr -d ' \n' | grep -q "\"app\":\"$1\""; }
waitfor() { i=0; while [ $i -lt 20 ]; do "$@" && return 0; sleep 0.5; i=$((i+1)); done; return 1; }
gone() { ! pgrep -x "$1" >/dev/null; }

chk "walpad-core running" pidof walpad-core
chk "$CTL status works" $CTL status
chk "/etc/walpad/apps.json present" test -s /etc/walpad/apps.json

# M6.2: an app is listed only if its executable exists
for pair in browser:firefox-esr terminal:st; do
  id=${pair%%:*}; exe=${pair##*:}
  if command -v "$exe" >/dev/null 2>&1; then
    listed "$id" && ok "$id installed and listed" || bad "$id installed and listed"
  else
    listed "$id" && bad "$id not installed but listed" || ok "$id not installed and hidden"
  fi
done

# M6.1: launch, single instance, running detection
if listed terminal; then
  $CTL launch terminal >/dev/null 2>&1
  waitfor running terminal && ok "terminal running after launch" || bad "terminal running after launch"
  [ "$(pgrep -c -x st)" -ge 1 ] && ok "terminal process exists" || bad "terminal process exists"
fi
if listed browser; then
  $CTL launch browser >/dev/null 2>&1
  waitfor running browser && ok "browser running after launch" || bad "browser running after launch" "no window with the wm_class yet?"
  before=$(pgrep -c firefox-esr)
  r=$($CTL launch browser 2>&1)
  echo "$r" | grep -Eq 'focused|already_running' && ok "second launch focuses ($(echo "$r" | tr -d ' \n'))" || bad "second launch focuses" "$r"
  sleep 3
  after=$(pgrep -c firefox-esr)
  [ "$after" -le "$((before+1))" ] && ok "no second Firefox instance ($before -> $after processes)" || bad "no second Firefox instance" "$before -> $after"

  # M6.3: crash recovery. kill -9 the browser: the core reports it, raises the shell, accepts a relaunch
  pkill -9 firefox-esr
  waitfor gone firefox-esr && ok "browser killed" || bad "browser killed"
  sleep 2
  running browser && bad "browser not running after kill" "still reported running" || ok "browser not running after kill"
  $CTL launch browser >/dev/null 2>&1
  waitfor running browser && ok "browser relaunches after a kill" || bad "browser relaunches after a kill"
fi
echo "--- last core log lines (look for 'crashed'):"
tail -n 10 /var/log/walpad-core.log 2>/dev/null

cat <<'MANUAL'
--- manual checklist, touch only (no keyboard, no ssh); tick each and report failures:
 [ ] Home -> Apps -> Browser opens Firefox full screen; load a site (type the address with the on-screen keyboard)
 [ ] return to the shell (volume-down long press, or the Home gesture): Home appears; Apps shows Browser as running
 [ ] tap Browser again: the existing window comes back, no second Firefox
 [ ] Apps -> Terminal: type `echo hello` with the on-screen keyboard, it prints; `exit` returns to the shell
 [ ] `false` then `exit` in the terminal: NO "closed unexpectedly" toast expected
        (st may exit with the shell's status; if a toast appears, record it: apps.json then needs "exit_ok":[1])
 [ ] long-press Browser in Apps -> Close app: it closes; NO "closed unexpectedly" toast
 [ ] while the browser is open, from another machine:  ssh root@<ip> pkill -9 firefox-esr
        -> the shell comes to the front within ~1 s and shows "Browser closed unexpectedly" (red toast, 3 s)
        -> tapping Browser starts a fresh Firefox
 [ ] start Firefox by hand over ssh (DISPLAY=:0 firefox-esr &): Apps shows it as running; Close app closes it
 [ ] an app whose executable is missing (rename /usr/bin/st temporarily) disappears from Apps within ~2 s
MANUAL
exit $FAIL
