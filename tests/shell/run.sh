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

# --- walpad-keys.py launch:/run: dispatch (M5.9 prep); needs python3 on the host
if command -v python3 >/dev/null 2>&1; then
  if python3 -I tests/shell/keys_py_test.py >"$T/keys_py.out" 2>&1; then ok "walpad-keys.py launch:/run: unit tests"; else bad "walpad-keys.py launch:/run: unit tests" "$(tail -5 "$T/keys_py.out")" ok; fi
else
  echo "SKIP walpad-keys.py unit tests (python3 not installed)"
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

# ===== scripts/image_set_wifi.sh: add a Wi-Fi network to a built image without rebuilding =====
if command -v mke2fs >/dev/null 2>&1 && command -v debugfs >/dev/null 2>&1 && command -v e2fsck >/dev/null 2>&1 && command -v python3 >/dev/null 2>&1; then
  IW=$PWD/scripts/image_set_wifi.sh
  bash -n "$IW" && ok "syntax image_set_wifi.sh" || bad "syntax image_set_wifi.sh" error ok
  WD=$T/wifi-img; mkdir -p "$WD/root/etc" "$WD/root/root"; echo x > "$WD/root/etc/hostname"
  mke2fs -q -t ext4 -F -d "$WD/root" "$WD/rootfs.ext4" 8M >/dev/null 2>&1
  # IEEE 802.11i vector: SSID "IEEE", passphrase "password"
  OUT=$(printf '%s\n' 'password' | bash "$IW" "$WD/rootfs.ext4" IEEE 2>&1); eq "image_set_wifi succeeds" "$?" 0
  case $OUT in OK:*) ok "reports OK";; *) bad "reports OK" "$OUT" "OK: ...";; esac
  CONF=$(debugfs -R "cat /etc/wpa_supplicant/wpa_supplicant.conf" "$WD/rootfs.ext4" 2>/dev/null)
  case $CONF in *f42c6fc52df0ebef9ebb4b90b38a5f902e83fe1b135a70e23aed762e9710a12e*) ok "derived key matches the IEEE test vector";; *) bad "derived key matches the IEEE test vector" "$CONF" "...f42c6f...";; esac
  case $CONF in *password*) bad "passphrase is not stored" "found" "absent";; *) ok "passphrase is not stored";; esac
  case $CONF in *"ssid=49454545"*) ok "SSID stored as hex";; *) bad "SSID stored as hex" "$CONF" "ssid=49454545";; esac
  case $(debugfs -R "stat /etc/wpa_supplicant/wpa_supplicant.conf" "$WD/rootfs.ext4" 2>/dev/null) in *"Mode:  0600"*) ok "config mode 0600";; *) bad "config mode 0600" "?" 0600;; esac
  e2fsck -fn "$WD/rootfs.ext4" >/dev/null 2>&1; eq "filesystem is clean afterwards" "$?" 0
  printf '%s\n' 'second-pass-1' | bash "$IW" "$WD/rootfs.ext4" Other >/dev/null 2>&1; eq "running again replaces the config" "$?" 0
  case $(debugfs -R "cat /etc/wpa_supplicant/wpa_supplicant.conf" "$WD/rootfs.ext4" 2>/dev/null) in *"ssid=49454545"*) bad "old network replaced" "still there" gone;; *) ok "old network replaced";; esac
  printf '%s\n' 'short' | bash "$IW" "$WD/rootfs.ext4" X >/dev/null 2>&1; eq "short passphrase rejected" "$?" 2
  bash "$IW" "$WD/missing.ext4" X </dev/null >/dev/null 2>&1; eq "missing image rejected" "$?" 2
  # the last flashed image is the rollback copy: never patch it, even through a different path
  if [ -f build/rootfs/rootfs.ext4 ]; then printf '%s\n' 'password' | bash "$IW" ./build/rootfs/../rootfs/rootfs.ext4 X >/dev/null 2>&1; eq "refuses the flashed image" "$?" 2; fi
else
  echo "SKIP image_set_wifi tests (mke2fs/debugfs/e2fsck/python3 not installed)"
fi

# --- walpad-storage: SD card and USB disks mounted, removal unmounts (fake root, no real mount)
SR=$T/storage; mkdir -p "$SR/sys/class/block/mmcblk1/mmcblk1p1" "$SR/proc" "$SR/dev"
ln -s mmcblk1/mmcblk1p1 "$SR/sys/class/block/mmcblk1p1"; echo 179:97 > "$SR/sys/class/block/mmcblk1p1/dev"
echo vfat > "$SR/sys/class/block/mmcblk1p1/fstype"; : > "$SR/proc/mounts"
st() { WALPAD_ROOT=$SR ONCE=1 sh "$S/walpad-storage" 2>/dev/null; }
eq "sd card mounted" "$(st)" "walpad-storage: mounted mmcblk1p1 (vfat) at /mnt/sd"
eq "sd card mounted once" "$(st)" ""
mkdir -p "$SR/sys/class/block/sda"; echo 8:0 > "$SR/sys/class/block/sda/dev"; echo exfat-not > "$SR/sys/class/block/sda/fstype"
eq "usb disk without partitions, unknown fs read-only" "$(st)" "walpad-storage: mounted sda (exfat-not) at /mnt/usb/sda"
grep -q " /mnt/usb/sda exfat-not ro " "$SR/proc/mounts"; eq "unknown fs is read-only" "$?" 0
rm -rf "$SR/sys/class/block/mmcblk1" "$SR/sys/class/block/mmcblk1p1"
eq "sd removed" "$(st)" "walpad-storage: removed mmcblk1p1 (/mnt/sd)"
grep -c " /mnt/sd " "$SR/proc/mounts" | grep -q '^0$'; eq "sd gone from mounts" "$?" 0
# ===== walpad-hotspot / walpad-wifi switch (owner request 2026-11-10): fake root, fake tools =====
HSS=$S/walpad-hotspot
sh -n "$HSS" && ok "syntax walpad-hotspot" || bad "syntax walpad-hotspot" error ok
sh -n "$S/walpad-wifi" && ok "syntax walpad-wifi" || bad "syntax walpad-wifi" error ok
grep -q 'ENABLED_FILE=/var/lib/walpad/wifi-enabled' "$S/walpad-wifi" && ok "walpad-wifi reads the Wi-Fi switch" || bad "walpad-wifi reads the Wi-Fi switch" missing present
for p in hostapd iptables; do case " $(WALPAD_PRINT_PKGS=1 bash scripts/build_rootfs.sh) " in *" $p "*) ok "image has $p";; *) bad "image has $p" missing present;; esac; done
HS=$T/hs; export HS
HR=$HS/root
mkdir -p "$HS/bin" "$HS/daemons" "$HR/etc/walpad" "$HR/run/walpad" "$HR/sys/class/net/ap0" "$HR/proc/sys/net/ipv4" "$HR/var/lib/walpad"
echo 0 > "$HR/proc/sys/net/ipv4/ip_forward"
printf 'nameserver 192.168.0.1\n' > "$HR/etc/resolv.conf"
SLEEP=$(command -v sleep)
cp "$SLEEP" "$HS/daemons/hostapd"; cp "$SLEEP" "$HS/daemons/udhcpd"
cat > "$HS/bin/hostapd" <<'EOF'
#!/bin/sh
echo "hostapd $*" >> "$HS/calls"
[ -f "$HS/hostapd-fail" ] && exit 1
pf=""; conf=""
while [ $# -gt 0 ]; do case $1 in -P) pf=$2; shift;; -B) ;; *) conf=$1;; esac; shift; done
cp -p "$conf" "$HS/hostapd-seen.conf"
"$HS/daemons/hostapd" 300 > /dev/null 2>&1 &
echo $! > "$pf"
EOF
cat > "$HS/bin/udhcpd" <<'EOF'
#!/bin/sh
echo "udhcpd $*" >> "$HS/calls"
cp "$1" "$HS/udhcpd-seen.conf"
"$HS/daemons/udhcpd" 300 > /dev/null 2>&1 &
echo $! > "$(sed -n 's/^pidfile //p' "$1")"
EOF
cat > "$HS/bin/iw" <<'EOF'
#!/bin/sh
echo "iw $*" >> "$HS/calls"
case "$*" in
  list) cat "$HS/iwlist" 2>/dev/null ;;
  "dev wlan0 link") if [ -f "$HS/linked" ]; then printf 'Connected to 02:00:00:00:00:01 (on wlan0)\n\tSSID: Home\n\tfreq: %s\n' "$(cat "$HS/linked")"; else echo "Not connected."; fi ;;
  "dev ap0 station dump") printf 'Station 02:00:00:00:00:0a (on ap0)\n\tinactive time:\t10 ms\nStation 02:00:00:00:00:0b (on ap0)\n' ;;
esac
exit 0
EOF
cat > "$HS/bin/ip" <<'EOF'
#!/bin/sh
echo "ip $*" >> "$HS/calls"
case "$*" in
  "route show default") [ -f "$HS/uplink" ] && echo "default via 192.168.0.1 dev $(cat "$HS/uplink") metric 10" ;;
  "addr flush dev wlan0") rm -f "$HS/uplink" ;;
esac
exit 0
EOF
cat > "$HS/bin/iptables" <<'EOF'
#!/bin/sh
echo "$(basename "$0") $*" >> "$HS/calls"
[ -f "$HS/ipt-fail" ] && exit 1
exit 0
EOF
cp "$HS/bin/iptables" "$HS/bin/iptables-legacy"
printf '#!/bin/sh\necho "$(basename "$0") $*" >> "$HS/calls"\nexit 0\n' > "$HS/bin/wpa_cli"
cp "$HS/bin/wpa_cli" "$HS/bin/pkill"
chmod +x "$HS/bin/"*
TAB=$(printf '\t')
excl_combo() { printf 'Wiphy phy1\n%svalid interface combinations:\n%s%s * #{ managed, AP, P2P-client, P2P-GO } <= 1,\n%s%s   total <= 1, #channels <= 1\n%sHT Capabilities\n' "$TAB" "$TAB" "$TAB" "$TAB" "$TAB" "$TAB"; }
conc_combo() { printf 'Wiphy phy1\n%svalid interface combinations:\n%s%s * #{ managed } <= 1, #{ AP, P2P-client, P2P-GO } <= 1,\n%s%s   total <= 2, #channels <= 1\n%sHT Capabilities\n' "$TAB" "$TAB" "$TAB" "$TAB" "$TAB" "$TAB"; }
hs() { WALPAD_HS_ROOT=$HR PATH=$HS/bin:$PATH sh "$HSS" "$1" 2>&1; }
kv() { printf '%s\n' "$1" | sed -n "s/^$2=//p" | head -n 1; }
HSECRET='hs-Secret pass 42'

OUT=$(hs start); eq "hotspot start without a password fails" "$?" 1
eq "  reason" "$(kv "$OUT" reason)" "no hotspot password set"
eq "  state failed" "$(kv "$OUT" state)" failed
( umask 077; printf '# x\nssid2=57616c506164\nwpa_passphrase=%s\n' "$HSECRET" > "$HR/etc/walpad/hotspot.conf" )

# no STA+AP concurrency, client connected with an uplink: the client is paused, local network only
excl_combo > "$HS/iwlist"; echo 2437 > "$HS/linked"; echo wlan0 > "$HS/uplink"; : > "$HS/calls"
OUT=$(hs start); eq "exclusive start succeeds" "$?" 0
eq "  state on" "$(kv "$OUT" state)" on
eq "  exclusive" "$(kv "$OUT" exclusive)" 1
eq "  channel 6" "$(kv "$OUT" channel)" 6
eq "  no uplink once the client is paused" "$(kv "$OUT" internet)" no_uplink
eq "  clients from the station dump" "$(kv "$OUT" clients)" 2
[ -e "$HR/run/walpad/hotspot-exclusive" ] && ok "  exclusive flag for walpad-wifi" || bad "  exclusive flag for walpad-wifi" missing present
grep -q 'wpa_cli.*terminate' "$HS/calls" && ok "  supplicant terminated" || bad "  supplicant terminated" "$(cat "$HS/calls")" terminate
case $(stat -c %a "$HS/hostapd-seen.conf") in 600) ok "  hostapd.conf mode 600";; *) bad "  hostapd.conf mode 600" "$(stat -c %a "$HS/hostapd-seen.conf")" 600;; esac
grep -qx "wpa_passphrase=$HSECRET" "$HS/hostapd-seen.conf" && ok "  passphrase copied file to file" || bad "  passphrase copied file to file" missing present
grep -qx 'ssid2=57616c506164' "$HS/hostapd-seen.conf" && grep -qx 'interface=ap0' "$HS/hostapd-seen.conf" && grep -qx 'wpa=2' "$HS/hostapd-seen.conf" && ok "  hostapd.conf: ap0, WPA2, name" || bad "  hostapd.conf: ap0, WPA2, name" "$(grep -v pass "$HS/hostapd-seen.conf")" ok
grep -qx 'opt router 192.168.50.1' "$HS/udhcpd-seen.conf" && grep -qx 'interface ap0' "$HS/udhcpd-seen.conf" && ok "  udhcpd serves 192.168.50.0/24 on ap0" || bad "  udhcpd serves 192.168.50.0/24 on ap0" "$(cat "$HS/udhcpd-seen.conf")" ok
grep -q 'ip addr add 192.168.50.1/24 dev ap0' "$HS/calls" && ok "  static address" || bad "  static address" missing present
case "$(cat "$HS/calls") $(cat "$HR/var/log/walpad-hotspot.log" 2>/dev/null)" in *"$HSECRET"*|*"pass 42"*) bad "  passphrase never in argv or the log" found absent;; *) ok "  passphrase never in argv or the log";; esac
OUT=$(hs status); eq "  status on" "$(kv "$OUT" state)/$(kv "$OUT" clients)" on/2
OUT=$(hs start); eq "  second start is a no-op" "$?/$(grep -c '^hostapd' "$HS/calls")" 0/1
OUT=$(hs stop); eq "stop succeeds" "$?/$(kv "$OUT" state)" 0/off
[ -e "$HR/run/walpad/hotspot-exclusive" ] && bad "  client resumes" "flag still there" removed || ok "  client resumes"
[ -e "$HR/run/walpad/hostapd.conf" ] && bad "  hostapd.conf removed" present removed || ok "  hostapd.conf removed"
[ -e "$HR/run/walpad/hostapd.pid" ] && bad "  hostapd stopped" running stopped || ok "  hostapd stopped"
eq "  no fake daemons left" "$(ps -eo comm | grep -cx 'hostapd\|udhcpd')" 0

# STA+AP concurrency on the client's channel, NAT to the uplink
conc_combo > "$HS/iwlist"; echo 2462 > "$HS/linked"; echo wlan0 > "$HS/uplink"; : > "$HS/calls"
OUT=$(hs start); eq "concurrent start succeeds" "$?" 0
eq "  same channel as the client (11)" "$(kv "$OUT" channel)" 11
eq "  exclusive 0" "$(kv "$OUT" exclusive)" 0
eq "  internet shared" "$(kv "$OUT" internet)" shared
grep -q 'iptables-legacy -t nat -A WALPAD_HS -s 192.168.50.0/24 -o wlan0 -j MASQUERADE' "$HS/calls" && ok "  MASQUERADE to wlan0 (legacy iptables)" || bad "  MASQUERADE to wlan0 (legacy iptables)" "$(grep iptables "$HS/calls")" MASQUERADE
eq "  forwarding on" "$(cat "$HR/proc/sys/net/ipv4/ip_forward")" 1
grep -qx 'opt dns 192.168.0.1 ' "$HS/udhcpd-seen.conf" && ok "  clients get the uplink DNS" || bad "  clients get the uplink DNS" "$(grep dns "$HS/udhcpd-seen.conf")" "opt dns 192.168.0.1"
grep -q 'terminate' "$HS/calls" && bad "  client left alone" terminated untouched || ok "  client left alone"
hs stop >/dev/null
eq "  forwarding off after stop" "$(cat "$HR/proc/sys/net/ipv4/ip_forward")" 0
grep -q 'iptables-legacy -t nat -D POSTROUTING -j WALPAD_HS' "$HS/calls" && ok "  NAT rules removed" || bad "  NAT rules removed" missing present

# NAT unavailable: hotspot without internet sharing
: > "$HS/ipt-fail"; echo wlan0 > "$HS/uplink"
OUT=$(hs start); eq "start without NAT still succeeds" "$?/$(kv "$OUT" internet)" 0/unavailable
rm -f "$HS/ipt-fail"
# the access point dies: status reports it and resumes the client
kill "$(cat "$HR/run/walpad/hostapd.pid")" 2>/dev/null; sleep 0.3
OUT=$(hs status); eq "dead hostapd is reported" "$(kv "$OUT" state)/$(kv "$OUT" reason)" "failed/the access point stopped"
hs stop >/dev/null
# hostapd refuses: failure is reported and everything is cleaned up
: > "$HS/hostapd-fail"; rm -f "$HS/linked"
OUT=$(hs start); eq "hostapd failure exits 1" "$?" 1
eq "  reason" "$(kv "$OUT" reason)" "hostapd could not start the access point"
[ -e "$HR/run/walpad/hostapd.conf" ] && bad "  no hostapd.conf left" present removed || ok "  no hostapd.conf left"
rm -f "$HS/hostapd-fail"
OUT=$(hs bogus); eq "unknown verb" "$?" 2
eq "no fake daemons left at the end" "$(ps -eo comm | grep -cx 'hostapd\|udhcpd')" 0

exit $FAIL
