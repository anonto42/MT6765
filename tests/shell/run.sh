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

exit $FAIL
