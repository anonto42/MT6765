#!/bin/sh
# Host tests for walpad-gps (owner request 2026-11-11): fake root, fake linker / insmod / walpad-gnss.
# Run by tests/shell/run.sh; prints PASS/FAIL lines.
cd "$(dirname "$0")/../.." || exit 1
S=$PWD/config/rootfs/usr/local/sbin
GS=$S/walpad-gps
T=$(mktemp -d)
GR=$T/root
cleanup() {
  for f in "$GR/run/walpad/gnss.pid"; do [ -f "$f" ] && kill "$(cat "$f")" 2>/dev/null; done
  pkill -f "$GR/run/walpad/android-bin/mnld" 2>/dev/null
  rm -rf "$T"
}
trap cleanup EXIT
FAIL=0
ok()  { echo "PASS $1"; }
bad() { echo "FAIL $1: got '$2' want '$3'"; FAIL=$((FAIL+1)); }
eq()  { [ "$2" = "$3" ] && ok "$1" || bad "$1" "$2" "$3"; }
kv()  { printf '%s\n' "$1" | sed -n "s/^$2=//p" | head -n 1; }

sh -n "$GS" && ok "syntax walpad-gps" || bad "syntax walpad-gps" error ok

FB=$T/bin
mkdir -p "$FB" "$GR/proc" "$GR/run/walpad" "$GR/var/log" "$GR/dev"
: > "$T/calls"
cat > "$FB/insmod" <<EOF
#!/bin/sh
echo "insmod \$*" >> "$T/calls"
EOF
cat > "$FB/mountpoint" <<'EOF'
#!/bin/sh
exit 0
EOF
cat > "$FB/walpad-gnss" <<EOF
#!/bin/sh
echo "walpad-gnss started" >> "$T/calls"
printf '{"v":1,"state":"searching"}\n' > "$GR/run/walpad/gnss.json"
trap 'echo "walpad-gnss got TERM" >> "$T/calls"; printf "{\"state\":\"off\"}\n" > "$GR/run/walpad/gnss.json"; exit 0' TERM
while :; do sleep 0.2; done
EOF
chmod +x "$FB/"*
# the "bionic linker": run under the name mnld, so it must stay alive with the right command line
mkdir -p "$GR/system/system/bin/bootstrap"
printf '#!/bin/sh\nsleep 30\n' > "$GR/system/system/bin/bootstrap/linker"
chmod +x "$GR/system/system/bin/bootstrap/linker"

gps() { WALPAD_GPS_ROOT=$GR WALPAD_GPS_MNLD_PATTERN="$GR/run/walpad/android-bin/mnld /vendor/bin/mnld" PATH=$FB:$PATH sh "$GS" "$@" 2>&1; }

# --- failures first
rm -rf "$GR/system/system/bin/bootstrap/linker.keep"
mv "$GR/system/system/bin/bootstrap/linker" "$T/linker.hold"
OUT=$(gps start); eq "start without /system fails" "$?" 1
eq "  state" "$(kv "$OUT" state)" failed
case $(kv "$OUT" reason) in *"/system not mounted"*) ok "  reason names /system";; *) bad "  reason names /system" "$(kv "$OUT" reason)" "...not mounted";; esac
mv "$T/linker.hold" "$GR/system/system/bin/bootstrap/linker"

OUT=$(gps start); eq "start without the Wi-Fi stack fails" "$?" 1
case $(kv "$OUT" reason) in *"Wi-Fi on first"*) ok "  tells the owner to turn Wi-Fi on";; *) bad "  tells the owner to turn Wi-Fi on" "$(kv "$OUT" reason)" "Wi-Fi on first";; esac

printf 'wmt_drv 1 0 - Live 0x0\nwlan_drv_gen4m 1 0 - Live 0x0\n' > "$GR/proc/modules"
# --- start: gps_drv is loaded, mnld and walpad-gnss run, state=on
OUT=$(gps start); eq "start succeeds" "$?" 0
eq "  state=on" "$(kv "$OUT" state)" on
grep -q "insmod /vendor/lib/modules/gps_drv.ko" "$T/calls" && ok "  gps_drv loaded" || bad "  gps_drv loaded" missing present
[ -L "$GR/run/walpad/android-bin/mnld" ] && ok "  mnld runs under its own name (symlink to the linker)" || bad "  mnld symlink" missing present
OUT=$(gps status); eq "status: on" "$(kv "$OUT" state)" on
eq "  mnld running" "$(kv "$OUT" mnld)" running
eq "  daemon running" "$(kv "$OUT" gnssd)" running
eq "  fix state from the daemon" "$(kv "$OUT" fix_state)" searching
P1=$(cat "$GR/run/walpad/gnss.pid")
OUT=$(gps start); eq "second start is idempotent" "$(kv "$OUT" state)" on
eq "  the daemon was not restarted" "$(cat "$GR/run/walpad/gnss.pid")" "$P1"
eq "  started once" "$(grep -c 'walpad-gnss started' "$T/calls")" 1

# --- stop: the daemon gets SIGTERM (it tells mnld to stop), then mnld goes
OUT=$(gps stop); eq "stop succeeds" "$?" 0
eq "  state=off" "$(kv "$OUT" state)" off
grep -q "walpad-gnss got TERM" "$T/calls" && ok "  daemon was asked to stop" || bad "  daemon was asked to stop" missing present
kill -0 "$P1" 2>/dev/null && bad "  daemon gone" alive dead || ok "  daemon gone"
OUT=$(gps status); eq "status: off" "$(kv "$OUT" state)" off
eq "  mnld stopped" "$(kv "$OUT" mnld)" stopped
[ ! -e "$GR/run/walpad/gnss.pid" ] && ok "  pid file removed" || bad "  pid file removed" present absent

# --- a missing daemon binary is reported, not hidden
mv "$FB/walpad-gnss" "$T/gnss.hold"
OUT=$(gps start); eq "start without walpad-gnss fails" "$?" 1
case $(kv "$OUT" reason) in *"not installed"*) ok "  reason";; *) bad "  reason" "$(kv "$OUT" reason)" "not installed";; esac
gps stop > /dev/null
mv "$T/gnss.hold" "$FB/walpad-gnss"

# --- usage and secrets
gps bogus > /dev/null; eq "unknown verb exits 2" "$?" 2
if grep -q -E 'passphrase|PIN|IMEI' "$GS"; then bad "no secrets in walpad-gps" found absent; else ok "no secrets in walpad-gps"; fi

echo "GPS_FAILS=$FAIL" > /dev/null
exit $FAIL
