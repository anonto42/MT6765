#!/bin/sh
# Host tests for walpad-audio's "jack" subcommand (owner request 2026-11-12, change
# headphone-mic-jack): a fake amixer on PATH records every call and lists only some controls. The
# script must flip only switches that exist, never guess a control, and never run a PCM tool
# (speaker-test/raw hw:0,x hang the legacy driver). Run by tests/shell/run.sh.
cd "$(dirname "$0")/../.." || exit 1
S=$PWD/config/rootfs/usr/local/sbin
A=$S/walpad-audio
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
FAIL=0
ok()  { echo "PASS $1"; }
bad() { echo "FAIL $1: got '$2' want '$3'"; FAIL=$((FAIL+1)); }
eq()  { [ "$2" = "$3" ] && ok "$1" || bad "$1" "$2" "$3"; }

sh -n "$A" && ok "syntax walpad-audio" || bad "syntax walpad-audio" error ok
[ -x "$A" ] && ok "walpad-audio is executable" || bad "walpad-audio is executable" no yes

B=$T/bin; R=$T/root
mkdir -p "$B" "$R/dev/snd"
: > "$R/dev/snd/controlC0"
export WALPAD_AUDIO_ROOT=$R
# the fake card has every control walpad-audio names, except Headset_PGAL_GAIN (not on every
# board): a missing control must be skipped, never guessed.
cat > "$B/amixer" <<'EOF'
#!/bin/sh
echo "amixer $*" >> "$T_CALLS"
case "$3" in
  controls)
    for n in Speaker_Amp_Switch Ext_Speaker_Amp_Switch Audio_Amp_L_Switch Audio_Amp_R_Switch \
             Lineout_PGAL_GAIN Lineout_PGAR_GAIN Headset_Speaker_Amp_Switch Headset_PGAR_GAIN \
             Audio_ADC_1_Switch Audio_ADC_2_Switch Audio_Preamp1_Switch Audio_Preamp2_Switch \
             Audio_MIC1_Mode_Select Audio_MIC2_Mode_Select Audio_MicSource1_Setting; do
      echo "numid=1,iface=MIXER,name='$n'"
    done ;;
esac
exit 0
EOF
chmod +x "$B/amixer"
for t in aplay arecord speaker-test; do
  printf '#!/bin/sh\necho %s >> "$T_BAD"\n' "$t" > "$B/$t"; chmod +x "$B/$t"
done
export T_CALLS=$T/calls T_BAD=$T/bad
run() { : > "$T_CALLS"; PATH=$B:$PATH sh "$A" "$@" 2>&1; }

# --- default (no args): unchanged boot behaviour
OUT=$(run)
echo "$OUT" | grep -q 'speaker on' && ok "default: speaker on" || bad "default: speaker on" "$OUT" "speaker on"
grep -q "name=Speaker_Amp_Switch On" "$T_CALLS" && ok "default: speaker amp on" || bad "default: speaker amp on" "$(cat "$T_CALLS")" set

# --- jack headphone + headset mic: speaker off, headset amp on, ADC2/preamp2 path, mic source ADC2
OUT=$(run jack headphone headset)
echo "$OUT" | grep -q 'Speaker_Amp_Switch = Off' && ok "headphone: speaker off" || bad "headphone: speaker off" "$OUT" Off
echo "$OUT" | grep -q 'Audio_Amp_L_Switch = On' && ok "headphone: headset L amp on" || bad "headphone: headset L amp on" "$OUT" On
echo "$OUT" | grep -q 'Audio_Amp_R_Switch = On' && ok "headphone: headset R amp on" || bad "headphone: headset R amp on" "$OUT" On
echo "$OUT" | grep -q 'no control Headset_PGAL_GAIN (skipped)' && ok "headphone: missing control skipped, not guessed" || bad "headphone: missing control skipped" "$OUT" skipped
! grep -q "name=Headset_PGAL_GAIN" "$T_CALLS" && ok "headphone: never csets a control that does not exist" || bad "headphone: never csets a missing control" set skipped
echo "$OUT" | grep -q 'Audio_ADC_2_Switch = On' && ok "headset mic: ADC2 on" || bad "headset mic: ADC2 on" "$OUT" On
echo "$OUT" | grep -q 'Audio_Preamp2_Switch = IN_ADC2' && ok "headset mic: preamp2 -> IN_ADC2" || bad "headset mic: preamp2" "$OUT" IN_ADC2
echo "$OUT" | grep -q 'Audio_MicSource1_Setting = ADC2' && ok "headset mic: MicSource1 -> ADC2" || bad "headset mic: MicSource1" "$OUT" ADC2
echo "$OUT" | grep -q 'Audio_ADC_1_Switch = Off' && ok "headset mic: built-in ADC1 off" || bad "headset mic: ADC1 off" "$OUT" Off
echo "$OUT" | grep -q 'jack route=headphone mic=headset' && ok "headphone+headset: summary line" || bad "summary line" "$OUT" "route=headphone mic=headset"

# --- jack speaker + builtin mic: back to normal
OUT=$(run jack speaker builtin)
echo "$OUT" | grep -q 'Audio_Amp_L_Switch = Off' && ok "speaker: headset L amp off" || bad "speaker: headset L amp off" "$OUT" Off
echo "$OUT" | grep -q 'Headset_Speaker_Amp_Switch = Off' && ok "speaker: headset+speaker amp off" || bad "speaker: headset+speaker amp off" "$OUT" Off
echo "$OUT" | grep -q 'Speaker_Amp_Switch = On' && ok "speaker: speaker amp on" || bad "speaker: speaker amp on" "$OUT" On
echo "$OUT" | grep -q 'Audio_ADC_1_Switch = On' && ok "builtin mic: ADC1 on" || bad "builtin mic: ADC1 on" "$OUT" On
echo "$OUT" | grep -q 'Audio_MicSource1_Setting = ADC1' && ok "builtin mic: MicSource1 -> ADC1" || bad "builtin mic: MicSource1" "$OUT" ADC1
echo "$OUT" | grep -q 'Audio_ADC_2_Switch = Off' && ok "builtin mic: headset ADC2 off" || bad "builtin mic: ADC2 off" "$OUT" Off

# --- bad args
run jack loudspeaker builtin > "$T/o"; rc=$?
eq "bad route is refused" "$rc" 2
run jack headphone loudspeaker > "$T/o"; rc=$?
eq "bad mic is refused" "$rc" 2
run badcmd > "$T/o"; rc=$?
eq "unknown subcommand is refused" "$rc" 2

# --- no sound card: nothing is run, exit 0
rm "$R/dev/snd/controlC0"
OUT=$(run jack headphone headset); rc=$?
eq "no card: exit 0" "$rc" 0
echo "$OUT" | grep -q 'no sound card' && ok "no card: says so" || bad "no card: says so" "$OUT" "no sound card"
[ ! -s "$T_CALLS" ] && ok "no card: amixer never called" || bad "no card: amixer never called" called none

# --- never a PCM tool, never a raw hw:0,x device
[ ! -e "$T_BAD" ] && ok "no PCM tool is ever run" || bad "no PCM tool is ever run" "$(cat "$T_BAD")" none
! sed 's/#.*$//' "$A" | grep -q -E 'aplay|arecord|speaker-test|hw:0' && ok "script has no PCM commands" || bad "script has no PCM commands" present absent

exit $FAIL
