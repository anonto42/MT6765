#!/bin/sh
# walpad-thumb with fake tools, mpv config sanity and package-name checks (change
# media-apps-capture).  Prints PASS/FAIL lines; run.sh counts the FAILs.  POSIX sh, no device.
cd "$(dirname "$0")/../.." || exit 1
TH=$PWD/config/rootfs/usr/local/sbin/walpad-thumb
M=$(mktemp -d); trap 'rm -rf "$M"' EXIT
FAIL=0
ok()  { echo "PASS $1"; }
bad() { echo "FAIL $1: got '$2' want '$3'"; FAIL=$((FAIL+1)); }
eq()  { [ "$2" = "$3" ] && ok "$1" || bad "$1" "$2" "$3"; }

sh -n "$TH" && ok "syntax walpad-thumb" || bad "syntax walpad-thumb" error ok

mkdir -p "$M/bin" "$M/root/Pictures" "$M/root/Videos" "$M/cache" "$M/outside"
CALLS=$M/calls
# fake ImageMagick, ffmpegthumbnailer, ffmpeg, ffprobe: they log argv and write a small "PNG"
cat > "$M/bin/convert" <<'F'
#!/bin/sh
echo "convert $*" >> "$FAKE_CALLS"
for a; do last=$a; done
[ -e "$FAKE_DIR/convert-fail" ] && exit 1
case $last in png:*) printf 'PNGDATA' > "${last#png:}";; esac
F
cat > "$M/bin/ffmpegthumbnailer" <<'F'
#!/bin/sh
echo "ffmpegthumbnailer $*" >> "$FAKE_CALLS"
[ -e "$FAKE_DIR/ftn-fail" ] && exit 1
while [ $# -gt 0 ]; do [ "$1" = -o ] && out=$2; shift; done
printf 'PNGDATA' > "$out"
F
cat > "$M/bin/ffmpeg" <<'F'
#!/bin/sh
echo "ffmpeg $*" >> "$FAKE_CALLS"
for a; do last=$a; done
[ -e "$FAKE_DIR/ffmpeg-fail" ] && exit 1
printf 'PNGDATA' > "$last"
F
cat > "$M/bin/ffprobe" <<'F'
#!/bin/sh
echo "ffprobe $*" >> "$FAKE_CALLS"
printf 'duration=187.520000\nTAG:title=Song\001 Name\nTAG:artist=Band\n'
F
cat > "$M/bin/slow" <<'F'
#!/bin/sh
sleep 30
F
chmod +x "$M/bin"/*
ln -s convert "$M/bin/magick"   # the host may have the real one; the fake must win
export FAKE_CALLS=$CALLS FAKE_DIR=$M
export WALPAD_THUMB_DIR=$M/cache WALPAD_THUMB_ROOTS=$M/root WALPAD_THUMB_TIMEOUT=2
# walpad-thumb resets PATH itself: run a copy whose PATH is the fakes first, then the system
sed "s|^PATH=.*|PATH=$M/bin:/usr/bin:/bin|" "$TH" > "$M/th.sh"
th() { sh "$M/th.sh" "$@"; }

echo x > "$M/root/Pictures/a.jpg"
echo x > "$M/root/Pictures/B.PNG"
echo x > "$M/root/Videos/v.mp4"
echo x > "$M/root/Videos/s.mp3"
echo x > "$M/root/Videos/notes.txt"
echo secret > "$M/outside/s.jpg"
ln -s "$M/outside/s.jpg" "$M/root/Pictures/escape.jpg"
ln -s "$M/root/Pictures/a.jpg" "$M/root/Pictures/inside.jpg"

out=$(th thumb "$M/root/Pictures/a.jpg" 2>&1); rc=$?
eq "image: exit 0" "$rc" 0
case $out in "$M"/cache/*.png) ok "image: prints the cache path";; *) bad "image: prints the cache path" "$out" "$M/cache/*.png";; esac
[ -s "$out" ] && ok "image: thumbnail file exists" || bad "image: thumbnail file exists" missing present
grep -q '^convert .*-limit memory' "$CALLS" && grep -q -- '-thumbnail 192x192^' "$CALLS" && ok "image: ImageMagick with limits and -thumbnail" || bad "image: ImageMagick call" "$(cat "$CALLS")" "limits and -thumbnail"
n=$(wc -l < "$CALLS" | tr -d ' ')
out2=$(th thumb "$M/root/Pictures/a.jpg" 2>&1); eq "image: second call is a cache hit (same path)" "$out2" "$out"
eq "image: a cache hit runs no tool" "$(wc -l < "$CALLS" | tr -d ' ')" "$n"
th thumb "$M/root/Pictures/B.PNG" >/dev/null 2>&1; eq "upper-case extension is recognised" "$?" 0
th thumb "$M/root/Pictures/inside.jpg" >/dev/null 2>&1; eq "symlink inside the media folders works" "$?" 0

th thumb "$M/root/Videos/v.mp4" >/dev/null 2>&1; eq "video: exit 0" "$?" 0
grep -q '^ffmpegthumbnailer -i .*v.mp4' "$CALLS" && ok "video: ffmpegthumbnailer is used" || bad "video: ffmpegthumbnailer is used" "$(cat "$CALLS")" call
touch "$M/ftn-fail"
echo y > "$M/root/Videos/v2.mp4"
th thumb "$M/root/Videos/v2.mp4" >/dev/null 2>&1; eq "video: ffmpeg fallback when ffmpegthumbnailer fails" "$?" 0
grep -q '^ffmpeg -nostdin .*v2.mp4' "$CALLS" && ok "video: fallback call made" || bad "video: fallback call made" "$(tail -2 "$CALLS")" call
rm -f "$M/ftn-fail"

th thumb "$M/root/Videos/s.mp3" >/dev/null 2>&1; eq "audio: cover art through ffmpeg" "$?" 0

# --- refusals
th thumb "$M/root/Pictures/escape.jpg" >/dev/null 2>&1; eq "symlink escape refused (2)" "$?" 2
th thumb "$M/outside/s.jpg" >/dev/null 2>&1; eq "outside the roots refused (2)" "$?" 2
th thumb "$M/root/Pictures/../../outside/s.jpg" >/dev/null 2>&1; eq "dot-dot refused (2)" "$?" 2
th thumb "-rf" >/dev/null 2>&1; eq "option-looking refused (2)" "$?" 2
th thumb "relative.jpg" >/dev/null 2>&1; eq "relative refused (2)" "$?" 2
th thumb "$M/root/Pictures/missing.jpg" >/dev/null 2>&1; eq "missing file refused (2)" "$?" 2
th thumb "$M/root/Pictures" >/dev/null 2>&1; eq "directory refused (2)" "$?" 2
th thumb "$M/root/Videos/notes.txt" >/dev/null 2>&1; eq "unsupported type (5)" "$?" 5
th bogus "$M/root/Pictures/a.jpg" >/dev/null 2>&1; eq "unknown command refused (2)" "$?" 2
th thumb >/dev/null 2>&1; eq "no file refused (2)" "$?" 2

# --- size limit: over the limit is skipped, no tool runs, remembered
echo big > "$M/root/Pictures/big.jpg"
before=$(wc -l < "$CALLS" | tr -d ' ')
WALPAD_THUMB_MAXMB=0 th thumb "$M/root/Pictures/big.jpg" >/dev/null 2>&1; eq "file over the size limit (3)" "$?" 3
eq "over the limit: no tool was run" "$(wc -l < "$CALLS" | tr -d ' ')" "$before"
ls "$M/cache" | grep -q '\.none$' && ok "failures are remembered (.none)" || bad "failures are remembered" none "a .none file"

# --- a failing tool: exit 5 and remembered, so it is not retried
echo f > "$M/root/Pictures/f.jpg"
touch "$M/convert-fail"
th thumb "$M/root/Pictures/f.jpg" >/dev/null 2>&1; eq "failing tool (5)" "$?" 5
before=$(wc -l < "$CALLS" | tr -d ' ')
th thumb "$M/root/Pictures/f.jpg" >/dev/null 2>&1
eq "failure is not retried" "$(wc -l < "$CALLS" | tr -d ' ')" "$before"
rm -f "$M/convert-fail"

# --- a hanging tool is killed at the timeout
cat > "$M/bin/convert" <<'F'
#!/bin/sh
exec sleep 30
F
echo h > "$M/root/Pictures/h.jpg"
t0=$(date +%s)
th thumb "$M/root/Pictures/h.jpg" >/dev/null 2>&1; rc=$?
t1=$(date +%s)
eq "hanging tool: exit 5" "$rc" 5
[ $((t1 - t0)) -le 8 ] && ok "hanging tool: killed within the timeout" || bad "hanging tool: killed within the timeout" "$((t1 - t0)) s" "<= 8 s"

# --- info
out=$(th info "$M/root/Videos/s.mp3" 2>&1)
eq "info: duration, title, artist (control characters stripped)" "$out" "duration=187
title=Song Name
artist=Band"
th info "$M/root/Pictures/a.jpg" >/dev/null 2>&1; eq "info: images have none (5)" "$?" 5

# --- no tool installed is not remembered as a failure (PATH has the basics only)
echo z > "$M/root/Pictures/z.jpg"
none_before=$(ls "$M/cache" | grep -c '\.none$')
mkdir -p "$M/basics"
for t in sh readlink stat sha1sum tr cut ls grep mkdir touch rm mv sed date sleep timeout flock wc tail; do
  p=$(command -v $t) && ln -sf "$p" "$M/basics/$t"
done
sed "s|^PATH=.*|PATH=$M/basics|" "$TH" > "$M/th-notool.sh"
"$M/basics/sh" "$M/th-notool.sh" thumb "$M/root/Pictures/z.jpg" >/dev/null 2>&1; eq "no tool installed (5)" "$?" 5
eq "no tool installed: no failure marker" "$(ls "$M/cache" | grep -c '\.none$')" "$none_before"

# --- the cache is bounded
mkdir -p "$M/cache2"
i=0; while [ $i -lt 12 ]; do : > "$M/cache2/old$i.png"; i=$((i + 1)); done
sed "s|^KEEP=.*|KEEP=5|" "$TH" > "$M/th-keep.sh"
sed -i "s|^PATH=.*|PATH=$M/bin:/usr/bin:/bin|" "$M/th-keep.sh"
echo k > "$M/root/Pictures/k.jpg"
rm -f "$M/convert-fail"; printf '#!/bin/sh\nfor a; do last=$a; done\ncase $last in png:*) printf PNGDATA > "${last#png:}";; esac\n' > "$M/bin/convert"
WALPAD_THUMB_DIR=$M/cache2 sh "$M/th-keep.sh" thumb "$M/root/Pictures/k.jpg" >/dev/null 2>&1
eq "cache keeps at most KEEP files" "$(ls "$M/cache2" | wc -l | tr -d ' ')" 5

# --- repo wiring
S=config/rootfs/usr/local/sbin
[ -x "$S/walpad-thumb" ] && ok "walpad-thumb is executable" || bad "walpad-thumb is executable" no yes

# every package named anywhere for media exists in the shipped catalogue or the image list
python3 -I - config/rootfs/etc/walpad/catalogue.json > "$M/pk.out" <<'PY'
import json, sys
def r(ok, msg):
    print(("PASS " if ok else "FAIL ") + msg)
cat = json.load(open(sys.argv[1]))
ids = {e["id"]: e for e in cat["entries"]}
need = {"mpv": ["mpv"], "gpicview": ["gpicview"], "media-tools": ["imagemagick", "ffmpeg", "ffmpegthumbnailer"]}
for i, pk in need.items():
    e = ids.get(i)
    r(bool(e) and e["packages"] == pk, "catalogue entry " + i + " names " + ",".join(pk))
for i in ("mpv", "gpicview"):
    r(bool(ids.get(i, {}).get("open")), "catalogue entry " + i + " declares open")
apps = {a["id"]: a for a in json.load(open("config/rootfs/etc/walpad/apps.json"))}
a = apps.get("gpicview", {})
r(bool(a.get("open")) and "{path}" in a.get("open_args", []), "apps.json gpicview opens files")
d = json.load(open("config/rootfs/etc/walpad/defaults.json"))["roles"]
for role, app in (("video", "mpv"), ("music", "mpv"), ("image", "gpicview")):
    r(d.get(role) == app and app in ids and role in ids[app]["roles"], "default " + role + " = " + app)
PY
cat "$M/pk.out"
grep -q '^FAIL' "$M/pk.out" && FAIL=$((FAIL + 1))
# the packages walpad-thumb documents are in the catalogue (optional) and gpicview in the image
for p in imagemagick ffmpeg ffmpegthumbnailer; do
  grep -q "\"$p\"" config/rootfs/etc/walpad/catalogue.json && grep -q "^# Packages:.*$p" "$TH" && ok "walpad-thumb package $p is in the catalogue" || bad "walpad-thumb package $p" missing present
done
case " $(WALPAD_PRINT_PKGS=1 bash scripts/build_rootfs.sh) " in *" gpicview "*) ok "image has gpicview";; *) bad "image has gpicview" missing present;; esac

# mpv config: software output, pulse at 44.1 kHz, touch bindings point at a script that exists
C=config/rootfs/root/.config/mpv
for kv in vo=x11 hwdec=no ao=pulse audio-samplerate=44100 osc=yes audio-display=embedded-first framedrop=vo sub-font=DejaVu; do
  grep -q "^$kv" "$C/mpv.conf" && ok "mpv.conf has $kv" || bad "mpv.conf has $kv" missing present
done
grep -q '^script-opts=.*osc-layout=bottombar.*osc-scalewindow=2.*osc-vidscale=no' "$C/mpv.conf" && ok "mpv.conf: large touch OSC" || bad "mpv.conf: large touch OSC" missing present
grep -q 'vo=gpu\|hwdec=auto\|ao=alsa' "$C/mpv.conf" && bad "mpv.conf avoids gpu/hwdec/raw alsa" present absent || ok "mpv.conf avoids gpu/hwdec/raw alsa"
for b in tap double back; do
  grep -q "script-binding touch/$b" "$C/input.conf" && grep -q "add_key_binding(nil, '$b'" "$C/scripts/touch.lua" && ok "touch/$b is bound and defined" || bad "touch/$b" missing present
done
if command -v luac >/dev/null 2>&1; then luac -p "$C/scripts/touch.lua" && ok "touch.lua compiles" || bad "touch.lua compiles" error ok; fi

exit $FAIL
