#!/bin/sh
# M0.1: run ON the tablet (e.g. `ssh root@<ip> sh -s < scripts/device/smoke.sh`).
# One PASS/FAIL line per check; exit status = number of failures.
FAIL=0
ok()   { echo "PASS $1"; }
bad()  { echo "FAIL $1${2:+ ($2)}"; FAIL=$((FAIL+1)); }
chk()  { n=$1; shift; if "$@" >/dev/null 2>&1; then ok "$n"; else bad "$n"; fi; }

for p in Xvfb fbmirror openbox; do chk "process $p" pidof $p; done
if pidof python3 >/dev/null 2>&1 || pidof walpad-core >/dev/null 2>&1; then ok "device daemon running"; else bad "device daemon running"; fi
chk "/run/walpad-ok exists" test -f /run/walpad-ok

ws=$(cat /run/walpad/wifi-state 2>/dev/null)
case $ws in "COMPLETED "[0-9]*) ok "wifi-state ($ws)";; *) bad "wifi-state" "${ws:-missing}";; esac
chk "ping 1.1.1.1" ping -c2 -W3 1.1.1.1

if netstat -ltn 2>/dev/null | grep -q ':22 '; then ok "dropbear listening on :22"; else bad "dropbear listening on :22"; fi
# telnetd must be bound to the USB addresses only
tl=$(netstat -ltn 2>/dev/null | awk '$4 ~ /:23$/ {print $4}')
if [ -z "$tl" ]; then bad "telnetd bound" "not listening"
else
  badaddr=$(echo "$tl" | grep -v -e '^192\.168\.42\.1:23$' -e '^192\.168\.43\.1:23$')
  [ -z "$badaddr" ] && ok "telnetd only on 192.168.42.1/43.1" || bad "telnetd scope" "$badaddr"
fi

if [ -f /run/walpad/status.json ]; then
  age=$(( $(date +%s) - $(stat -c %Y /run/walpad/status.json) ))
  [ "$age" -lt 5 ] && ok "status.json age ${age}s" || bad "status.json age" "${age}s"
else bad "status.json" missing; fi

echo "--- manual (owner): rotation flicker? rotation direction right? web page in Firefox? typing in st?"
exit $FAIL
