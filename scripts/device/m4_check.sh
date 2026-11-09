#!/bin/sh
# M4.4: run ON the tablet with walpad-core running (any runtime mode).
#   printf '%s\n' 'YOUR-PASSPHRASE' | ssh root@<ip> sh -s -- 'SSID' < scripts/device/m4_check.sh   (passphrase on stdin)
# Simpler: log in and run:  sh m4_check.sh 'SSID'   then type the passphrase when asked.
# Joins a NEW test network, checks the result, then forgets it.  One PASS/FAIL line per check.
SSID=$1
[ -n "$SSID" ] || { echo "usage: m4_check.sh SSID   (passphrase read from stdin)"; exit 2; }
CTL=walpad-ctl-rs
FAIL=0
ok()  { echo "PASS $1"; }
bad() { echo "FAIL $1${2:+ ($2)}"; FAIL=$((FAIL+1)); }
read -r PASS
[ -n "$PASS" ] || { echo "no passphrase on stdin"; exit 2; }

CONF=/etc/wpa_supplicant/wpa_supplicant.conf
BEFORE=$(grep -c '^network=' $CONF 2>/dev/null)

out=$($CTL wifi-scan 2>&1) && echo "$out" | grep -q "$SSID" && ok "scan lists $SSID" || bad "scan lists $SSID" "$out"
printf '%s\n' "$PASS" | $CTL wifi-connect "$SSID" > /tmp/m4-connect.txt 2>&1
if grep -q "^connected" /tmp/m4-connect.txt; then ok "connect"; else bad "connect" "$(cat /tmp/m4-connect.txt)"; fi

# the passphrase must not be visible in any process command line, now or during the join
if grep -l -F -- "$PASS" /proc/[0-9]*/cmdline 2>/dev/null | grep -v "/proc/$$/" | grep -q .; then bad "passphrase not in any argv" "found in /proc/*/cmdline"; else ok "passphrase not in any argv"; fi
# nor in core/session logs
if grep -q -F -- "$PASS" /var/log/walpad-core.log /var/log/walpad-wifi.log 2>/dev/null; then bad "passphrase not in logs"; else ok "passphrase not in logs"; fi
[ "$(stat -c %a $CONF 2>/dev/null)" = 600 ] && ok "config mode 600" || bad "config mode 600" "$(stat -c %a $CONF 2>/dev/null)"
AFTER=$(grep -c '^network=' $CONF 2>/dev/null)
[ "$AFTER" -gt "$BEFORE" ] 2>/dev/null && ok "network saved ($BEFORE -> $AFTER)" || bad "network saved" "$BEFORE -> $AFTER"
# every other saved network must be enabled again (not left [DISABLED]), so they auto-connect later
wpa_cli -p /run/wpa_supplicant -i wlan0 list_networks 2>/dev/null | grep -q DISABLED && bad "other networks still enabled" "a saved network is [DISABLED]" || ok "other networks still enabled"
sleep 5
state=$(cat /run/walpad/wifi-state 2>/dev/null); case $state in "COMPLETED "[0-9]*) ok "wifi-state ($state)";; *) bad "wifi-state" "$state";; esac
ping -c2 -W3 1.1.1.1 >/dev/null 2>&1 && ok "internet reachable" || bad "internet reachable"

# forget the test network again
id=$($CTL wifi-saved 2>/dev/null | tr -d ' \n' | sed -n "s/.*\"id\":\([0-9]*\),\"ssid\":\"$SSID\".*/\1/p")
[ -n "$id" ] && $CTL wifi-forget "$id" >/dev/null 2>&1 && ok "forget (id $id)" || bad "forget" "id=$id"
cat <<'MANUAL'
--- manual: reboot the tablet (cold boot) and confirm that
 [ ] the networks that were saved BEFORE this test still auto-connect (scripts/device/smoke.sh PASS)
 [ ] re-run this script once more to join the network again after the reboot
MANUAL
exit $FAIL
