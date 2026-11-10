# walpad-pkg against a fake apk (change app-catalogue-manager).  Sourced by run.sh: uses its ok/bad/eq
# helpers and $T, never touches a real package database.
PK=$PWD/config/rootfs/usr/local/sbin/walpad-pkg
CATJ=$PWD/config/rootfs/etc/walpad/catalogue.json
sh -n "$PK" && ok "syntax walpad-pkg" || bad "syntax walpad-pkg" error ok
P=$T/pkg; mkdir -p "$P/root/etc/walpad" "$P/bin"
cp "$CATJ" "$P/root/etc/walpad/catalogue.json"
: > "$P/db"
cat > "$P/bin/apk" <<'FAKEAPK'
#!/bin/sh
# fake apk: state in $FAKE_APK_DIR (db = installed names, calls = argv log, offline/addfail/hang switches)
D=$FAKE_APK_DIR
echo "$*" >> "$D/calls"
cmd=$1; shift
case $cmd in
  update)
    if [ -e "$D/offline" ]; then echo "ERROR: https://dl-cdn.alpinelinux.org/alpine/v3.22/main: network error (check Internet connection and firewall)"; exit 1; fi
    echo "v3.22 main"; exit 0 ;;
  add)
    [ "$1" = -u ] && shift
    if [ -e "$D/hang" ]; then exec sleep 30; fi
    if [ -e "$D/addfail" ]; then cat "$D/addfail"; exit 1; fi
    n=$#; i=0
    for p in "$@"; do i=$((i+1)); echo "($i/$n) Installing $p (1.0-r0)"; grep -qx "$p" "$D/db" || echo "$p" >> "$D/db"; done
    echo "OK: 100 MiB in 200 packages"; exit 0 ;;
  del)
    n=$#; i=0
    for p in "$@"; do i=$((i+1)); echo "($i/$n) Purging $p (1.0-r0)"; grep -vx "$p" "$D/db" > "$D/db.new"; mv "$D/db.new" "$D/db"; done
    exit 0 ;;
  info)
    case $1 in
      -r) echo "$2-1.0-r0 is required by:"; cat "$D/rdeps.$2" 2>/dev/null; exit 0 ;;
      -s) printf '%s-1.0-r0 installed size:\n700 KiB\n\n' "$2"; exit 0 ;;
    esac ;;
esac
exit 0
FAKEAPK
chmod +x "$P/bin/apk"
pk() { FAKE_APK_DIR=$P WALPAD_PKG_ROOT=$P/root WALPAD_PKG_APK=$P/bin/apk WALPAD_PKG_RESERVE_KB=${RESERVE:-0} WALPAD_PKG_TIMEOUT=${TMO:-20} TMPDIR=$T sh "$PK" "$@" 2>/dev/null; }
calls() { cat "$P/calls" 2>/dev/null; }
reset_pkg() { : > "$P/calls"; : > "$P/db"; rm -f "$P/offline" "$P/addfail" "$P/hang" "$P"/rdeps.*; }

reset_pkg
OUT=$(pk install --need-kb 700 mousepad mousepad); eq "pkg install: exit 0" "$?" 0
eq "pkg install: ends with OK" "$(echo "$OUT" | tail -n 1)" OK
eq "pkg install: apk called with the catalogue package only" "$(calls)" "update
add mousepad"
eq "pkg install: package is installed" "$(cat "$P/db")" mousepad
case $OUT in *"PROGRESS 100 Done"*) ok "pkg install: progress reaches 100";; *) bad "pkg install: progress reaches 100" "$OUT" "PROGRESS 100 Done";; esac
prev=-1; mono=1
for v in $(echo "$OUT" | sed -n 's/^PROGRESS \([0-9]*\) .*/\1/p'); do [ "$v" -ge "$prev" ] || mono=0; prev=$v; done
eq "pkg install: progress never goes back" "$mono" 1
eq "pkg install: apk progress line is shown" "$(echo "$OUT" | grep -c 'PROGRESS 90 Installing mousepad')" 1
reset_pkg
OUT=$(pk install zathura zathura zathura-pdf-mupdf); eq "pkg install: two packages" "$?:$(calls | tail -n 1)" "0:add zathura zathura-pdf-mupdf"

# --- only what the catalogue names
reset_pkg
for args in "mousepad mousepad evil-package" "mousepad geany" "nosuchid mousepad" "mousepad ../../tmp/x" "mousepad /tmp/x.apk" "mousepad x.apk" "mousepad -X" "mousepad --allow-untrusted" "mousepad Mousepad" "--allow-untrusted mousepad mousepad" "-X mousepad"; do
  OUT=$(pk install $args); rc=$?
  eq "pkg install refused ($args)" "$rc:$(echo "$OUT" | tail -n 1 | cut -d' ' -f1-2)" "3:ERROR refused"
done
eq "pkg install refused: apk never ran" "$(calls)" ""
OUT=$(pk install mousepad --allow-untrusted mousepad); case $OUT in *"Untrusted packages are not installed"*) ok "pkg: --allow-untrusted is refused and says why";; *) bad "pkg: --allow-untrusted refusal" "$OUT" untrusted;; esac
OUT=$(pk install mousepad); eq "pkg install without packages -> usage" "$?" 2
OUT=$(pk frobnicate); eq "pkg unknown verb -> usage" "$?" 2
OUT=$(pk install --need-kb abc mousepad mousepad); eq "pkg bad --need-kb -> usage" "$?" 2

# --- space: free space may never drop below the reserve
reset_pkg
OUT=$(RESERVE=999999999999 pk install --need-kb 700 mousepad mousepad); rc=$?
eq "pkg install: not enough space" "$rc:$(echo "$OUT" | tail -n 1 | cut -d' ' -f1-2)" "4:ERROR space"
eq "pkg install: refused before apk ran" "$(calls)" ""
FREE=$(pk free | sed -n 's/^free_kb=//p')
case $FREE in ''|*[!0-9]*) bad "pkg free prints a number" "$FREE" digits;; *) ok "pkg free prints a number";; esac
OUT=$(RESERVE=$((FREE - 1000)) pk install --need-kb 3000 mousepad mousepad); eq "pkg install: need + reserve above free -> refused" "$?" 4
OUT=$(RESERVE=$((FREE - 100000)) pk install --need-kb 3000 mousepad mousepad); eq "pkg install: enough space -> ok" "$?" 0

# --- failures leave the database as it was
reset_pkg; : > "$P/offline"
OUT=$(pk install mousepad mousepad); rc=$?
eq "pkg install offline" "$rc:$(echo "$OUT" | tail -n 1 | cut -d' ' -f1-2)" "5:ERROR net"
eq "pkg install offline: nothing installed" "$(cat "$P/db")" ""
reset_pkg; echo "ERROR: unable to select packages: mousepad (no such package)" > "$P/addfail"
OUT=$(pk install mousepad mousepad); rc=$?
eq "pkg install apk error" "$rc:$(echo "$OUT" | tail -n 1 | cut -d' ' -f1-2)" "6:ERROR apk"
case $OUT in *"unable to select packages"*) ok "pkg install apk error: shows apk's message";; *) bad "pkg install apk error message" "$OUT" "unable to select";; esac
reset_pkg; : > "$P/hang"
OUT=$(TMO=1 pk install mousepad mousepad); rc=$?
eq "pkg install timeout" "$rc:$(echo "$OUT" | tail -n 1 | cut -d' ' -f1-2)" "7:ERROR timeout"

# --- upgrade and size
reset_pkg
OUT=$(pk upgrade mousepad mousepad); eq "pkg upgrade" "$?:$(calls | tail -n 1)" "0:add -u mousepad"
eq "pkg size sums the packages" "$(pk size zathura zathura-pdf-mupdf)" "size_kb=1400"
OUT=$(pk size "bad name"); eq "pkg size refuses odd names" "$?" 3

# --- remove: only catalogue packages, never system ones, never what others need
reset_pkg; echo mousepad > "$P/db"
OUT=$(pk remove mousepad mousepad); eq "pkg remove: ok" "$?:$(echo "$OUT" | tail -n 1)" "0:OK"
eq "pkg remove: apk del called with the package" "$(calls | tail -n 1)" "del mousepad"
eq "pkg remove: package is gone" "$(cat "$P/db")" ""
reset_pkg
for args in "terminal st" "browser firefox-esr" "files pcmanfm" "mousepad busybox" "mousepad musl" "mousepad apk-tools"; do
  OUT=$(pk remove $args); eq "pkg remove refused ($args)" "$?:$(echo "$OUT" | tail -n 1 | cut -d' ' -f1-2)" "3:ERROR refused"
done
eq "pkg remove refused: apk del never ran" "$(calls | grep -c '^del')" 0
printf 'claws-mail-3.0-r0\n' > "$P/rdeps.mousepad"
OUT=$(pk remove mousepad mousepad); rc=$?
eq "pkg remove: needed by another package" "$rc:$(echo "$OUT" | tail -n 1 | cut -d' ' -f1-2)" "3:ERROR refused"
case $OUT in *"needed by claws-mail"*) ok "pkg remove: names the dependent";; *) bad "pkg remove: names the dependent" "$OUT" claws-mail;; esac
eq "pkg remove: apk del did not run" "$(calls | grep -c '^del')" 0
printf 'zathura-pdf-mupdf-1.0-r0\n' > "$P/rdeps.zathura"; : > "$P/rdeps.mousepad"
OUT=$(pk remove zathura zathura zathura-pdf-mupdf); eq "pkg remove: a dependent that is removed too is fine" "$?" 0

# --- the shipped catalogue
if command -v python3 >/dev/null 2>&1; then
  python3 -I - "$CATJ" <<'PYCAT' > "$T/catalogue.out" 2>&1 && ok "catalogue.json is valid and complete" || bad "catalogue.json" "$(cat "$T/catalogue.out")" ok
import json, re, sys
c = json.load(open(sys.argv[1]))
ids = [e["id"] for e in c["entries"]]
assert len(ids) == len(set(ids)), "duplicate ids"
need = {"id", "name", "category", "packages", "size_kb", "summary", "icon", "exec", "wm_class", "desktop", "roles", "system"}
for e in c["entries"]:
    assert need <= set(e), (e["id"], need - set(e))
    assert re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,31}", e["id"]), e["id"]
    assert e["packages"] and all(re.fullmatch(r"[a-z0-9][a-z0-9._+-]{0,63}", p) and not p.endswith(".apk") for p in e["packages"]), e["id"]
    assert isinstance(e["size_kb"], int) and e["size_kb"] > 0, e["id"]
    assert all(isinstance(a, str) for a in e["exec"]), e["id"]
    assert set(e["roles"]) <= {"browser", "files", "terminal", "video", "music", "image", "pdf"}, e["id"]
want = {"mpv": "mpv", "evince": "evince", "zathura": "zathura", "imv": "imv", "mousepad": "mousepad", "vlc": "vlc",
        "ristretto": "ristretto", "gthumb": "gthumb", "abiword": "abiword", "gnumeric": "gnumeric", "audacious": "audacious"}
for i, p in want.items():
    e = c["entries"][ids.index(i)]
    assert p in e["packages"], i
assert c["reserve_kb"] == 2 * 1024 * 1024
assert [e for e in c["entries"] if e["id"] == "mousepad"][0]["size_kb"] == 700
assert [e for e in c["entries"] if e["id"] == "vlc"][0]["size_kb"] == 28000
d = json.load(open(sys.argv[1].replace("catalogue", "defaults")))
for role, app in d["roles"].items():
    assert app in ids and role in c["entries"][ids.index(app)]["roles"], (role, app)
assert set(d["roles"]) == {"browser", "files", "terminal", "video", "music", "image", "pdf"}
PYCAT
else
  echo "SKIP catalogue.json validation (python3 not installed)"
fi
