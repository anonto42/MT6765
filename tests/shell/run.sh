#!/bin/sh
# Host tests for walpad-runtime and walpad-supervise (M3.1).  POSIX sh, no device needed.
cd "$(dirname "$0")/../.." || exit 1
S=$PWD/config/rootfs/usr/local/sbin
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
FAIL=0
ok()  { echo "PASS $1"; }
bad() { echo "FAIL $1: got '$2' want '$3'"; FAIL=$((FAIL+1)); }
eq()  { [ "$2" = "$3" ] && ok "$1" || bad "$1" "$2" "$3"; }

export WALPAD_RUN_DIR=$T/run WALPAD_RUNTIME_CONF=$T/runtime.conf
mkdir -p "$T/run" "$T/bin"
fakebin() { printf '#!/bin/sh\n[ "$1" = --capabilities ] && echo "%s"\nexit 0\n' "$2" > "$T/bin/$1"; chmod +x "$T/bin/$1"; }
rt() { PATH=$T/bin:$PATH sh "$S/walpad-runtime" "$1" 2>/dev/null; }

# --- runtime selection
printf 'WALPAD_DEVICE=python\nWALPAD_INPUT=python\nWALPAD_SHELL=python\nWALPAD_SHADOW=0\n' > "$T/runtime.conf"
eq "default conf is python" "$(rt WALPAD_DEVICE)$(rt WALPAD_INPUT)$(rt WALPAD_SHELL)" pythonpythonpython
eq "shadow off" "$(rt WALPAD_SHADOW)" 0
printf 'WALPAD_DEVICE=rust\nWALPAD_SHADOW=1\n' > "$T/runtime.conf"
eq "rust requested but binary missing -> python" "$(rt WALPAD_DEVICE)" python
eq "shadow on" "$(rt WALPAD_SHADOW)" 1
fakebin walpad-core "ipc shadow"
eq "rust requested, capability missing -> python" "$(rt WALPAD_DEVICE)" python
fakebin walpad-core "ipc shadow device input"
eq "rust requested, capability present -> rust" "$(rt WALPAD_DEVICE)" rust
eq "input not requested stays python" "$(rt WALPAD_INPUT)" python
: > "$T/run/fallback-core"
eq "fallback flag forces python" "$(rt WALPAD_DEVICE)" python
rm "$T/run/fallback-core"
printf 'WALPAD_INPUT=rust\n' > "$T/runtime.conf"
eq "input=rust needs device=rust" "$(rt WALPAD_INPUT)" python
printf 'WALPAD_DEVICE=rust\nWALPAD_INPUT=rust\n' > "$T/runtime.conf"
fakebin walpad-core "ipc shadow device input"
eq "input=rust with device=rust" "$(rt WALPAD_INPUT)" rust
printf 'WALPAD_SHELL=rust\n' > "$T/runtime.conf"
fakebin walpad-shell "shell"
eq "shell rust when capable" "$(rt WALPAD_SHELL)" rust
: > "$T/run/fallback-shell"
eq "shell fallback flag" "$(rt WALPAD_SHELL)" python
rm "$T/run/fallback-shell"
printf 'WALPAD_DEVICE=banana\n' > "$T/runtime.conf"
eq "garbage value -> python" "$(rt WALPAD_DEVICE)" python
rm -f "$T/runtime.conf"
eq "missing conf -> python" "$(rt WALPAD_DEVICE)" python

# --- supervision
export WALPAD_MIN_UP=3 WALPAD_RESTART_DELAY=0
printf '#!/bin/sh\necho x >> %s/runs\nexit 1\n' "$T" > "$T/bin/crasher"; chmod +x "$T/bin/crasher"
sh "$S/walpad-supervise" core --on-fallback "echo hook > $T/hook" -- "$T/bin/crasher" 2>/dev/null
eq "instant crasher: 3 runs" "$(wc -l < "$T/runs" | tr -d ' ')" 3
[ -e "$T/run/fallback-core" ] && ok "fallback flag written" || bad "fallback flag written" missing present
eq "fallback hook ran" "$(cat "$T/hook" 2>/dev/null)" hook

rm -f "$T/runs" "$T/run/fallback-x"
# a command that runs long enough resets the counter: 2 quick, 1 slow, 2 quick, then it stops being supervised by us
cat > "$T/bin/mixed" <<MIX
#!/bin/sh
n=\$(cat $T/n 2>/dev/null || echo 0); n=\$((n+1)); echo \$n > $T/n
[ \$n -eq 3 ] && sleep 4
[ \$n -ge 6 ] && exit 1
exit 1
MIX
chmod +x "$T/bin/mixed"
sh "$S/walpad-supervise" x -- "$T/bin/mixed" 2>/dev/null
eq "slow run resets the counter (run 3 slow, then 3 quick: total 6)" "$(cat "$T/n")" 6

printf '#!/bin/sh\nexit 0\n' > "$T/bin/true2"; chmod +x "$T/bin/true2"
sh "$S/walpad-supervise" y --require definitely-not-running-proc -- "$T/bin/true2" 2>/dev/null; eq "--require stops with 3" "$?" 3
sh "$S/walpad-supervise" z 2>/dev/null; eq "no command -> 2" "$?" 2
[ -e "$T/run/fallback-y" ] && bad "no flag when required proc is gone" present absent || ok "no flag when required proc is gone"

# --- no-Python mode (M7.1): config/rootfs-nopython
NP=$PWD/config/rootfs-nopython; NS=$NP/files/usr/local/sbin
rm -rf "$T/run"; mkdir -p "$T/run"
eq "default runtime.conf stays python" "$(grep -c '=python$' config/rootfs/etc/walpad/runtime.conf)" 3
eq "no-python runtime.conf is rust" "$(grep -c '=rust$' "$NP/files/etc/walpad/runtime.conf")" 3
chk() { PATH=$T/bin:$PATH WALPAD_RUNTIME="sh $S/walpad-runtime" sh "$NS/walpad-rust-check" 2>&1; }
rm -f "$T/bin/walpad-core" "$T/bin/walpad-shell"
cp "$NP/files/etc/walpad/runtime.conf" "$T/runtime.conf"
out=$(chk); rc=$?
eq "check: nothing installed -> fail" "$rc" 1
case $out in *"not installed"*) ok "check: reason names the missing binary";; *) bad "check: reason names the missing binary" "$out" "...not installed...";; esac
fakebin walpad-core "ipc shadow device input"; fakebin walpad-shell ""
out=$(chk); rc=$?
eq "check: shell lacks capability -> fail (guard not weakened)" "$rc" 1
case $out in *WALPAD_SHELL*"lacks capability"*) ok "check: reason names the capability";; *) bad "check: reason names the capability" "$out" "WALPAD_SHELL ... lacks capability";; esac
fakebin walpad-core "ipc shadow"
out=$(chk); rc=$?
eq "check: core lacks capability -> fail" "$rc" 1
fakebin walpad-core "ipc shadow device input"; fakebin walpad-shell "shell"
out=$(chk); eq "check: all capable -> ok, silent" "$?:$out" "0:"
: > "$T/run/fallback-shell"
out=$(chk); rc=$?
eq "check: shell fell back this boot -> fail" "$rc" 1
rm "$T/run/fallback-shell"
printf 'WALPAD_DEVICE=rust\nWALPAD_INPUT=rust\nWALPAD_SHELL=python\n' > "$T/runtime.conf"
out=$(chk); rc=$?
eq "check: conf says python -> fail, nothing selects Python" "$rc" 1
case $out in *"not rust in runtime.conf"*) ok "check: reason names the conf";; *) bad "check: reason names the conf" "$out" "...not rust in runtime.conf";; esac

# rescue: fake pkill/pidof/padui, never touches real processes
printf '#!/bin/sh\necho "$*" >> %s/pkill.log\nexit 0\n' "$T" > "$T/bin/pkill"
printf '#!/bin/sh\n[ -e %s/padui-running ]\n' "$T" > "$T/bin/pidof"
printf '#!/bin/sh\necho started >> %s/padui.marker\n' "$T" > "$T/bin/padui"
chmod +x "$T/bin/pkill" "$T/bin/pidof" "$T/bin/padui"
mkdir -p "$T/log"
export WALPAD_OK_FILE=$T/ok WALPAD_LOG_DIR=$T/log WALPAD_PADUI_ONCE=1
resc() { PATH=$T/bin:$PATH sh "$NS/walpad-rescue" "$@" 2>/dev/null; }
rm -f "$T/ok"
resc first reason
eq "rescue: reason recorded" "$(cat "$T/run/rescue.d/reason")" "first reason"
grep -q 'walpad-supervise' "$T/pkill.log" && grep -q 'Xvfb' "$T/pkill.log" && ok "rescue: supervisors and X stopped" || bad "rescue: supervisors and X stopped" "$(cat "$T/pkill.log")" "walpad-supervise + Xvfb"
[ -e "$T/padui.marker" ] && bad "rescue before health: no second padui" started "ramdisk starts it" || ok "rescue before health: leaves padui to the ramdisk"
n=$(wc -l < "$T/pkill.log")
resc second reason
eq "rescue: idempotent (first reason kept)" "$(cat "$T/run/rescue.d/reason")" "first reason"
eq "rescue: idempotent (nothing killed again)" "$(wc -l < "$T/pkill.log")" "$n"
rm -rf "$T/run/rescue.d"; : > "$T/ok"
resc healthy then broken
eq "rescue after healthy start: padui started once" "$(wc -l < "$T/padui.marker" 2>/dev/null | tr -d ' ')" 1
rm -rf "$T/run/rescue.d" "$T/padui.marker"; : > "$T/padui-running"
resc again
[ -e "$T/padui.marker" ] && bad "rescue: no second padui if one runs" started none || ok "rescue: no second padui if one runs"
unset WALPAD_OK_FILE WALPAD_LOG_DIR WALPAD_PADUI_ONCE

# the overlay applied to a copy of config/rootfs
for f in "$NS"/* "$NP/apply.sh" "$NP/files/sbin/walpad-init"; do sh -n "$f" 2>/dev/null && ok "syntax $(basename "$f")" || bad "syntax $(basename "$f")" error ok; done
cp -a config/rootfs "$T/img"
if sh "$NP/apply.sh" "$T/img" 2>"$T/apply.err"; then ok "apply.sh succeeds"; else bad "apply.sh succeeds" "$(cat "$T/apply.err")" ok; fi
eq "no .py left in the image" "$(find "$T/img" -name '*.py' | wc -l | tr -d ' ')" 0
eq "no python word in sbin, usr/local, etc/walpad (except walpad-runtime)" "$(cd "$T/img" && grep -rIlE 'python|\.py([^a-z]|$)' sbin usr/local etc/walpad | grep -vc 'walpad-runtime$')" 0
eq "wifi Tk app not in apps.json" "$(grep -c 'wifi' "$T/img/etc/walpad/apps.json")" 0
[ -x "$T/img/usr/local/sbin/walpad-rescue" ] && ok "rescue installed executable" || bad "rescue installed executable" no yes
grep -q 'walpad-rescue' "$T/img/sbin/walpad-init" && grep -q 'walpad-rescue' "$T/img/usr/local/sbin/walpad-session" && ok "init and session hand over to walpad-rescue" || bad "init and session hand over to walpad-rescue" no yes
cp -a config/rootfs "$T/img2"; echo 'exec python3 x' >> "$T/img2/usr/local/sbin/walpad-wifi"
sh "$NP/apply.sh" "$T/img2" 2>/dev/null && bad "apply.sh rejects a leftover python reference" accepted rejected || ok "apply.sh rejects a leftover python reference"
# build script package selection (dry run, needs bash)
if command -v bash >/dev/null 2>&1; then
  d=$(WALPAD_PRINT_PKGS=1 bash scripts/build_rootfs.sh | tr -s " " "\n" | tr "\n" " "); n=$(WALPAD_NO_PYTHON=1 WALPAD_PRINT_PKGS=1 bash scripts/build_rootfs.sh | tr -s " " "\n" | tr "\n" " ")
  keep=0; for p in python3 python3-tkinter tk py3-xlib xdotool; do case " $d " in *" $p "*) keep=$((keep+1));; esac; done; eq "default packages keep python3 tk py3-xlib xdotool" "$keep" 5
  gone=0; for p in python3 python3-tkinter tk py3-xlib xdotool; do case " $n " in *" $p "*) gone=1;; esac; done
  eq "no-python packages drop python3 tk py3-xlib xdotool" "$gone" 0
  for p in matchbox-keyboard alsa-utils st firefox-esr xvfb openbox; do case " $n " in *" $p "*) ;; *) bad "no-python keeps $p" missing present;; esac; done
  eq "only 5 packages differ" "$(for p in $d; do case " $n " in *" $p "*) ;; *) echo $p;; esac; done | wc -l | tr -d ' ')" 5
  WALPAD_NO_PYTHON=2 bash scripts/build_rootfs.sh >/dev/null 2>&1; eq "bad WALPAD_NO_PYTHON rejected" "$?" 2
fi


# ===== M9.1 log caps: walpad-logcap soak (a long-lived `>>` writer across several cap cycles) =====
LC=$PWD/config/rootfs/usr/local/sbin/walpad-logcap
sh -n "$LC" && ok "syntax walpad-logcap" || bad "syntax walpad-logcap" error ok
LD=$T/logcap; mkdir -p "$LD"
# unit behaviour on a small file
printf 'a\n' > "$LD/walpad-small.log"; printf 'keep me\n' > "$LD/other.log"
head -c 5000 /dev/zero | tr '\0' x > "$LD/other-big.log"
n=0; : > "$LD/walpad-big.log"; while [ $n -lt 300 ]; do echo "line $n ................................................" >> "$LD/walpad-big.log"; n=$((n+1)); done
WALPAD_LOG_DIR=$LD WALPAD_LOG_MAX=10000 WALPAD_LOG_KEEP=2000 sh "$LC"; eq "logcap exit status" "$?" 0
eq "small log untouched" "$(cat "$LD/walpad-small.log")" a
eq "log outside the list untouched" "$(wc -c < "$LD/other-big.log" | tr -d ' ')" 5000
[ "$(wc -c < "$LD/walpad-big.log")" -le 2000 ] && ok "big log trimmed to KEEP" || bad "big log trimmed to KEEP" "$(wc -c < "$LD/walpad-big.log")" "<=2000"
eq "newest line survives the trim" "$(tail -n 1 "$LD/walpad-big.log" | cut -d' ' -f1-2)" "line 299"
case $(head -n 1 "$LD/walpad-big.log") in "line "*) ok "no cut first line";; *) bad "no cut first line" "$(head -n 1 "$LD/walpad-big.log")" "line N";; esac
[ -s "$LD/walpad-big.log.1" ] && ok "previous generation kept as .1" || bad "previous generation kept as .1" missing present
WALPAD_LOG_MAX=abc WALPAD_LOG_DIR=$LD sh "$LC" 2>/dev/null; eq "bad WALPAD_LOG_MAX rejected" "$?" 2
# soak: one writer keeps ONE descriptor opened with >> (the way walpad-init/session start the services)
# and writes > 1 MiB while logcap runs every 0.1 s with MAX=100000 KEEP=20000.
if sleep 0.01 2>/dev/null; then
  SD=$T/soak; mkdir -p "$SD"; : > "$SD/walpad-core.log"
  PAD=...........................................................................
  ( i=0; while [ $i -lt 12000 ]; do echo "soak line $i $PAD"; i=$((i+1)); [ $((i % 100)) -eq 0 ] && sleep 0.02; done; echo "soak done" ) >> "$SD/walpad-core.log" &
  WP=$!; PEAK=0; CYCLES=0
  while kill -0 $WP 2>/dev/null; do
    sz=$(wc -c < "$SD/walpad-core.log"); [ "$sz" -gt "$PEAK" ] && PEAK=$sz
    WALPAD_LOG_DIR=$SD WALPAD_LOG_MAX=100000 WALPAD_LOG_KEEP=20000 sh "$LC"
    CYCLES=$((CYCLES+1)); sleep 0.1
  done
  wait $WP
  WALPAD_LOG_DIR=$SD WALPAD_LOG_MAX=100000 WALPAD_LOG_KEEP=20000 sh "$LC"
  FINAL=$(wc -c < "$SD/walpad-core.log")
  [ "$CYCLES" -ge 3 ] && ok "soak ran $CYCLES cap cycles" || bad "soak cap cycles" "$CYCLES" ">=3"
  [ "$PEAK" -le 200000 ] && ok "soak: peak size $PEAK <= cap + slack (200000)" || bad "soak peak size" "$PEAK" "<=200000"
  [ "$FINAL" -le 100000 ] && ok "soak: final size $FINAL below the cap" || bad "soak final size" "$FINAL" "<=100000"
  eq "soak: newest line survives" "$(tail -n 1 "$SD/walpad-core.log")" "soak done"
  eq "soak: line before it is the last one written" "$(tail -n 2 "$SD/walpad-core.log" | head -n 1 | cut -d' ' -f1-3)" "soak line 11999"
  eq "soak: no sparse NUL holes after truncation" "$(tr -d '\0' < "$SD/walpad-core.log" | wc -c | tr -d ' ')" "$FINAL"
  [ -f "$SD/walpad-core.log.1" ] && ok "soak: .1 generation exists" || bad "soak: .1 generation" missing present
  echo "PASS soak stats: cycles=$CYCLES peak=$PEAK final=$FINAL (written ~$((12000 * 90)) bytes)"
fi
# every service redirection in init/session appends (>>), otherwise truncation would leave sparse holes
for f in config/rootfs/sbin/walpad-init config/rootfs-nopython/files/sbin/walpad-init config/rootfs/usr/local/sbin/walpad-session \
         config/rootfs-nopython/files/usr/local/sbin/walpad-session config/rootfs/usr/local/sbin/walpad-python-daemons; do
  eq "append redirections only in $f" "$(grep -E '[^>0-9]> *"?/var/log/(walpad-|Xvfb|fbmirror|openbox|dropbear)' "$f" | wc -l | tr -d ' ')" 0
done
for f in config/rootfs/sbin/walpad-init config/rootfs-nopython/files/sbin/walpad-init; do
  eq "init runs logcap at start and in its main loop ($f)" "$(grep -c "walpad-logcap" "$f")" 2
done

# ===== M9.2 safe mode decision (walpad-safemode-check, fake walpad-ctl-rs) =====
SM=$PWD/config/rootfs/usr/local/sbin/walpad-safemode-check
sh -n "$SM" && ok "syntax walpad-safemode-check" || bad "syntax walpad-safemode-check" error ok
mkdir -p "$T/smbin"
cat > "$T/smbin/walpad-ctl-rs" <<'FAKE'
#!/bin/sh
echo "$*" >> "$SM_LOG"
[ -n "$FAKE_SLEEP" ] && sleep "$FAKE_SLEEP"
exit "${FAKE_RC:-1}"
FAKE
chmod +x "$T/smbin/walpad-ctl-rs"
export SM_LOG=$T/sm.log; : > "$SM_LOG"
sm() { WALPAD_CTL=$T/smbin/walpad-ctl-rs sh "$SM"; echo $?; }
eq "safe mode when the key is held (rc 0)" "$(FAKE_RC=0 sm)" 0
eq "ctl asked for key 114" "$(tail -n 1 "$SM_LOG")" "key-held 114"
eq "not held (rc 1) -> normal boot" "$(FAKE_RC=1 sm)" 1
eq "ctl error (rc 2) -> normal boot" "$(FAKE_RC=2 sm)" 1
eq "ctl killed/crashed (rc 139) -> normal boot" "$(FAKE_RC=139 sm)" 1
eq "ctl missing -> normal boot" "$(WALPAD_CTL=$T/smbin/nope sh "$SM"; echo $?)" 1
if command -v timeout >/dev/null 2>&1; then
  t0=$(date +%s); r=$(FAKE_RC=0 FAKE_SLEEP=5 WALPAD_SAFEMODE_TIMEOUT=1 sm); t1=$(date +%s)
  eq "hanging ctl -> normal boot" "$r" 1
  [ $((t1 - t0)) -le 3 ] && ok "hanging ctl is cut off after about 1 s" || bad "hanging ctl timeout" "$((t1 - t0)) s" "<=3"
fi
# init wiring: decided early, core and session only inside the not-safe branch, padui + ok marker in safe mode
for f in config/rootfs/sbin/walpad-init config/rootfs-nopython/files/sbin/walpad-init; do
  a=$(grep -n 'walpad-safemode-check' "$f" | head -n 1 | cut -d: -f1); b=$(grep -n 'if \[ -z "\$SAFE" \]' "$f" | head -n 1 | cut -d: -f1)
  c=$(grep -n 'walpad-session >>' "$f" | head -n 1 | cut -d: -f1); d=$(grep -n 'walpad-wifi >>' "$f" | head -n 1 | cut -d: -f1)
  e=$(grep -n 'walpad-core \$CORE_ARGS' "$f" | head -n 1 | cut -d: -f1); g=$(grep -n '^fi  # SAFE' "$f" | cut -d: -f1)
  [ -n "$a" ] && [ -n "$b" ] && [ "$a" -lt "$b" ] && [ "$b" -lt "$e" ] && [ "$e" -lt "$c" ] && [ "$c" -lt "$g" ] && ok "safe-mode gate wraps core+session ($f)" || bad "safe-mode gate wraps core+session ($f)" "$a $b $e $c $g" "ordered"
  [ -n "$d" ] && [ "$d" -lt "$b" ] && ok "wifi (and ssh above it) start before the gate, also in safe mode ($f)" || bad "wifi before the gate ($f)" "$d vs $b" "wifi < gate"
  grep -q 'padui >> /var/log/padui.log' "$f" && grep -q ': > /run/walpad-ok' "$f" && ok "safe mode starts padui and marks the session healthy ($f)" || bad "safe mode starts padui ($f)" missing present
done
for f in "$SM" "$LC"; do case $(grep -cE 'python|\.py([^a-z]|$)' "$f") in 0) ok "no python reference in $(basename "$f")";; *) bad "no python reference in $(basename "$f")" found none;; esac; done


# ===== M9.3 reproducible build: the Alpine release is pinned in one variable, manifests are written =====
eq "build_rootfs.sh has no floating alpine:latest" "$(grep -c 'alpine:latest' scripts/build_rootfs.sh)" 0
eq "ALPINE_REL defined exactly once" "$(grep -c '^ALPINE_REL=' scripts/build_rootfs.sh)" 1
case $(sed -n 's/^ALPINE_REL=\([0-9.]*\).*/\1/p' scripts/build_rootfs.sh) in 3.[0-9][0-9]) ok "ALPINE_REL is an explicit 3.NN release";; *) bad "ALPINE_REL is an explicit 3.NN release" "$(grep '^ALPINE_REL=' scripts/build_rootfs.sh)" "3.NN";; esac
for m in packages.txt config-hashes.txt image.sha256; do grep -q "$m" scripts/build_rootfs.sh && ok "build writes $m" || bad "build writes $m" missing present; done

exit $FAIL
