#!/bin/sh
# M8.3 measurement: Firefox memory with/without zram swap and with/without low-memory prefs.
# Run ON the tablet as root (busybox sh):   sh measure_firefox_mem.sh [url ...]
# Four configurations, each = start headless firefox-esr on a throw-away profile with 3 tabs, wait, record
#   RSS of all firefox processes (sum of VmRSS of the launched process and its descendants), MemAvailable, swap used.
#   A defaults, no zram      B defaults + zram      C low-memory prefs, no zram      D low-memory prefs + zram
# Changes made (all undone on exit, also on Ctrl-C): a temporary zram swap device (only if /dev/zram0 is unused
# and no swap is active), temp profiles and pages under /tmp/walpad-ffmem.*.  Nothing in /etc or the image is touched.
# Refuses to run while another firefox is running (it would disturb the numbers and the UI) unless ALLOW_RUNNING=1.
# Env: WAIT=40 (seconds to settle per configuration), ZRAM_MB=512, ZRAM_ALGO=lz4 (falls back to the default).
# URLs given as arguments replace the generated local pages (they need Wi-Fi).  The generated pages are a synthetic
# load (DOM + array allocations), NOT a real-site measurement; run again with real URLs for the numbers that matter.
WAIT=${WAIT:-40}
ZRAM_MB=${ZRAM_MB:-512}
ZRAM_ALGO=${ZRAM_ALGO:-lz4}
FF=${FIREFOX:-firefox-esr}
T=$(mktemp -d /tmp/walpad-ffmem.XXXXXX) || exit 2
FFPID=""; ZRAM_ON=0

command -v "$FF" >/dev/null 2>&1 || { echo "$FF not found"; exit 2; }
if [ "$ALLOW_RUNNING" != 1 ] && pidof "$FF" firefox >/dev/null 2>&1; then
  echo "firefox is already running; close it first or set ALLOW_RUNNING=1"; rm -rf "$T"; exit 2
fi

stop_ff() {
  [ -n "$FFPID" ] || return 0
  kill "$FFPID" 2>/dev/null
  i=0; while [ $i -lt 20 ] && kill -0 "$FFPID" 2>/dev/null; do sleep 1; i=$((i+1)); done
  kill -9 "$FFPID" 2>/dev/null
  pkill -f "$T/" 2>/dev/null
  FFPID=""
}
zram_off() {
  [ "$ZRAM_ON" = 1 ] || return 0
  swapoff /dev/zram0 2>/dev/null
  echo 1 > /sys/block/zram0/reset 2>/dev/null
  ZRAM_ON=0
}
cleanup() { stop_ff; zram_off; rm -rf "$T"; }
trap cleanup EXIT
trap 'exit 130' INT TERM

zram_on() {
  if [ ! -e /sys/block/zram0 ]; then modprobe zram 2>/dev/null; fi
  [ -e /sys/block/zram0/disksize ] || { echo "zram unavailable (no /sys/block/zram0)"; return 1; }
  [ "$(cat /sys/block/zram0/disksize 2>/dev/null)" = 0 ] || { echo "zram0 already configured, not touching it"; return 1; }
  [ "$(wc -l < /proc/swaps)" -le 1 ] || { echo "swap already active, not adding zram"; return 1; }
  echo "$ZRAM_ALGO" > /sys/block/zram0/comp_algorithm 2>/dev/null
  echo $((ZRAM_MB * 1024 * 1024)) > /sys/block/zram0/disksize || return 1
  mkswap /dev/zram0 >/dev/null 2>&1 && swapon -p 100 /dev/zram0 || { echo 1 > /sys/block/zram0/reset 2>/dev/null; return 1; }
  ZRAM_ON=1
}

# ---- load pages (synthetic unless URLs are given) ----
if [ $# -gt 0 ]; then URLS="$*"; else
  for n in 1 2 3; do
    cat > "$T/p$n.html" <<EOF
<!doctype html><meta charset=utf-8><title>load $n</title><body><script>
var a=[];for(var i=0;i<60000;i++){var d=document.createElement('div');d.textContent='row '+i+' '+Math.random();document.body.appendChild(d);}
var big=new Array(1500000);for(var i=0;i<big.length;i++)big[i]=i*$n;
</script>
EOF
  done
  URLS="file://$T/p1.html file://$T/p2.html file://$T/p3.html"
fi

# ---- low-memory prefs (UNTESTED defaults, see docs/rust/reports/M8.md) ----
cat > "$T/lowmem.js" <<'EOF'
user_pref("dom.ipc.processCount", 1);
user_pref("dom.ipc.processCount.webIsolated", 1);
user_pref("fission.autostart", false);
user_pref("browser.cache.memory.capacity", 16384);
user_pref("browser.sessionhistory.max_total_viewers", 0);
user_pref("browser.tabs.unloadOnLowMemory", true);
user_pref("javascript.options.mem.high_water_mark", 64);
user_pref("image.mem.decode_bytes_at_a_time", 32768);
user_pref("gfx.canvas.accelerated", false);
user_pref("media.memory_cache_max_size", 8192);
user_pref("extensions.pocket.enabled", false);
user_pref("browser.startup.homepage", "about:blank");
EOF

mem_kb() { sed -n "s/^$1: *\([0-9]*\) kB.*/\1/p" /proc/meminfo; }

# all descendants of $1 (including $1), by scanning /proc/*/stat
tree() {
  want=" $1 "; changed=1
  while [ $changed = 1 ]; do
    changed=0
    for s in /proc/[0-9]*/stat; do
      line=$(sed 's/^.*) //' "$s" 2>/dev/null) || continue
      set -- $line; pp=$2
      p=${s#/proc/}; p=${p%/stat}
      case "$want" in *" $pp "*) case "$want" in *" $p "*) ;; *) want="$want$p "; changed=1;; esac;; esac
    done
  done
  echo $want
}

run_cfg() {   # $1 = label, $2 = lowmem 0/1
  prof="$T/prof-$1"; mkdir -p "$prof"
  [ "$2" = 1 ] && cp "$T/lowmem.js" "$prof/user.js"
  swap0=$(mem_kb SwapFree)
  env LIBGL_ALWAYS_SOFTWARE=1 MOZ_DISABLE_CONTENT_SANDBOX=1 MOZ_DISABLE_GMP_SANDBOX=1 MOZ_DISABLE_RDD_SANDBOX=1 MOZ_DISABLE_SOCKET_PROCESS_SANDBOX=1 MOZ_WEBRENDER=0 MOZ_ACCELERATED=0 "$FF" --headless --new-instance --no-remote --profile "$prof" $URLS >"$T/ff-$1.log" 2>&1 &
  FFPID=$!
  sleep "$WAIT"
  if ! kill -0 "$FFPID" 2>/dev/null; then printf '%-3s %s\n' "$1" "firefox exited early (see $T/ff-$1.log)"; tail -n3 "$T/ff-$1.log"; FFPID=""; return; fi
  sum=0; n=0
  for p in $(tree "$FFPID"); do
    r=$(sed -n 's/^VmRSS: *\([0-9]*\) kB.*/\1/p' /proc/$p/status 2>/dev/null)
    [ -n "$r" ] && { sum=$((sum + r)); n=$((n + 1)); }
  done
  avail=$(mem_kb MemAvailable); st=$(mem_kb SwapTotal); sf=$(mem_kb SwapFree)
  printf '%-3s %-6s %-5s %6d %12d %14d %12d\n' "$1" "$([ "$2" = 1 ] && echo lowmem || echo default)" "$([ "$ZRAM_ON" = 1 ] && echo zram || echo none)" \
    "$n" $((sum / 1024)) $((avail / 1024)) $(( (st - sf) / 1024 ))
  stop_ff
  sleep 3
}

echo "MemTotal $(( $(mem_kb MemTotal) / 1024 )) MB, MemAvailable before: $(( $(mem_kb MemAvailable) / 1024 )) MB, $WAIT s per configuration"
echo "pages: $URLS"
printf '%-3s %-6s %-5s %6s %12s %14s %12s\n' cfg prefs swap procs firefox_RSS_MB MemAvailable_MB swap_used_MB
run_cfg A 0
run_cfg C 1
if zram_on; then
  run_cfg B 0
  run_cfg D 1
  zram_off
else
  echo "B, D skipped: zram could not be set up (see message above)"
fi
echo
echo "RSS = sum of VmRSS over the firefox process tree (shared pages are counted once per process, so it over-counts)."
echo "MemAvailable is the better number: higher is better.  Compare A vs B (zram) and A vs C (prefs), then D."
echo "Everything was undone: zram swap off, temp files removed."
