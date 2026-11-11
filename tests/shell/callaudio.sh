#!/bin/sh
# Host tests for walpad-callaudio (owner request 2026-11-11, change phone-dialer): a fake amixer on
# PATH records every call and lists only some controls.  The script must flip only switches that
# exist, never run a PCM tool, and never invent a control.  Run by tests/shell/run.sh.
cd "$(dirname "$0")/../.." || exit 1
S=$PWD/config/rootfs/usr/local/sbin
CA=$S/walpad-callaudio
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
FAIL=0
ok()  { echo "PASS $1"; }
bad() { echo "FAIL $1: got '$2' want '$3'"; FAIL=$((FAIL+1)); }
eq()  { [ "$2" = "$3" ] && ok "$1" || bad "$1" "$2" "$3"; }

sh -n "$CA" && ok "syntax walpad-callaudio" || bad "syntax walpad-callaudio" error ok
[ -x "$CA" ] && ok "walpad-callaudio is executable" || bad "walpad-callaudio is executable" no yes

B=$T/bin; R=$T/root
mkdir -p "$B" "$R/dev/snd" "$R/run/walpad"
: > "$R/dev/snd/controlC0"
export WALPAD_CALLAUDIO_ROOT=$R
# the fake card has the speaker switches and Voice_Amp_Switch, but no Handset_Amp_Switch
cat > "$B/amixer" <<'EOF'
#!/bin/sh
echo "amixer $*" >> "$T_CALLS"
case "$3" in
  controls)
    for n in Speaker_Amp_Switch Ext_Speaker_Amp_Switch Audio_Amp_L_Switch Audio_Amp_R_Switch \
             Voice_Amp_Switch Lineout_PGAL_GAIN Lineout_PGAR_GAIN Voice_MD1_Playback_Volume; do
      echo "numid=1,iface=MIXER,name='$n'"
    done ;;
  cget) echo "  : values=on" ;;
esac
exit 0
EOF
chmod +x "$B/amixer"
# tools that must never run: a PCM tool would be a hang risk on the legacy driver
for t in aplay arecord speaker-test; do
  printf '#!/bin/sh\necho %s >> "$T_BAD"\n' "$t" > "$B/$t"; chmod +x "$B/$t"
done
export T_CALLS=$T/calls T_BAD=$T/bad
run() { : > "$T_CALLS"; PATH=$B:$PATH sh "$CA" "$@" 2>&1; }

# --- earpiece: speaker off, handset on; a missing control is skipped, not guessed
OUT=$(run start earpiece)
echo "$OUT" | grep -q 'Voice_Amp_Switch = On' && ok "earpiece: Voice_Amp_Switch on" || bad "earpiece: Voice_Amp_Switch on" "$OUT" "= On"
echo "$OUT" | grep -q 'Speaker_Amp_Switch = Off' && ok "earpiece: speaker amp off" || bad "earpiece: speaker amp off" "$OUT" "= Off"
echo "$OUT" | grep -q 'no control Handset_Amp_Switch (skipped)' && ok "earpiece: unknown control skipped" || bad "earpiece: unknown control skipped" "$OUT" skipped
! grep -q 'name=Handset_Amp_Switch' "$T_CALLS" && ok "earpiece: never csets a control that does not exist" || bad "earpiece: never csets a missing control" set skipped
eq "route is remembered" "$(cat "$R/run/walpad/callaudio")" earpiece

# --- speaker route: speaker on, earpiece off, line-out gain 0 dB (8) like walpad-audio
OUT=$(run start speaker)
echo "$OUT" | grep -q 'Speaker_Amp_Switch = On' && ok "speaker: amp on" || bad "speaker: amp on" "$OUT" "= On"
echo "$OUT" | grep -q 'Voice_Amp_Switch = Off' && ok "speaker: earpiece off" || bad "speaker: earpiece off" "$OUT" "= Off"
grep -q 'name=Lineout_PGAL_GAIN 8' "$T_CALLS" && ok "speaker: line-out gain 8" || bad "speaker: line-out gain 8" none 8
eq "route follows the switch" "$(cat "$R/run/walpad/callaudio")" speaker

# --- stop: normal playback state (speaker on, earpiece off), marker gone
OUT=$(run stop)
echo "$OUT" | grep -q 'back to normal playback' && ok "stop: says so" || bad "stop: says so" "$OUT" normal
grep -q 'name=Voice_Amp_Switch Off' "$T_CALLS" && grep -q 'name=Speaker_Amp_Switch On' "$T_CALLS" && ok "stop: speaker on, earpiece off" || bad "stop: restores playback" "$(cat "$T_CALLS")" ""
[ ! -e "$R/run/walpad/callaudio" ] && ok "stop: marker removed" || bad "stop: marker removed" present absent

# --- bad route, usage, status and controls
run start loudspeaker > "$T/o"; rc=$?
eq "bad route is refused" "$rc" 2
run > "$T/o"; rc=$?
eq "usage exit code" "$rc" 2
OUT=$(run status)
echo "$OUT" | grep -q '^route: none' && ok "status: route none after stop" || bad "status: route none" "$OUT" none
echo "$OUT" | grep -q 'Handset_Amp_Switch = (no such control)' && ok "status: missing control labelled" || bad "status: missing control labelled" "$OUT" label
OUT=$(run controls)
echo "$OUT" | grep -q 'Voice_MD1_Playback_Volume' && ok "controls: lists voice controls" || bad "controls: lists voice controls" "$OUT" listed

# --- no sound card: nothing is run, exit 0
rm "$R/dev/snd/controlC0"
OUT=$(run start speaker); rc=$?
eq "no card: exit 0" "$rc" 0
echo "$OUT" | grep -q 'no sound card' && ok "no card: says so" || bad "no card: says so" "$OUT" "no sound card"
[ ! -s "$T_CALLS" ] && ok "no card: amixer never called" || bad "no card: amixer never called" called none

# --- the PCM tools never ran, and the script never mentions a PCM device to open
[ ! -e "$T_BAD" ] && ok "no PCM tool is ever run" || bad "no PCM tool is ever run" "$(cat "$T_BAD")" none
! sed 's/#.*$//' "$CA" | grep -q -E 'aplay|arecord|speaker-test|hw:0' && ok "script has no PCM commands" || bad "script has no PCM commands" present absent

exit $FAIL
