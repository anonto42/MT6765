#!/usr/bin/env python3
"""M2.3: pair what the Python daemons did with what the Rust core WOULD do (shadow mode).

  shadow_diff.sh PYTHON_LOG [PYTHON_LOG...] --core CORE_LOG [--tz-offset-min N]

PYTHON_LOG: walpad-devd / walpad-keys logs (`HH:MM:SS msg`, or HH:MM:SS.mmm if patched).
CORE_LOG:   walpad-core --shadow stderr (`<epoch_ms> WOULD ...`).
Comparable events (the only ones Python logs): orientation, cover, action, low-battery, critical.
Everything else Rust would do (backlight, pointer, powermenu, struts) is only counted.
Window: +-300 ms with millisecond logs, -300..+1300 ms around the second otherwise.
Exit status = number of unexplained divergences.
"""
import re, sys, datetime

def parse_args(a):
    py, core, off = [], None, 0
    i = 0
    while i < len(a):
        if a[i] == "--core": core = a[i + 1]; i += 2
        elif a[i] == "--tz-offset-min": off = int(a[i + 1]); i += 2
        else: py.append(a[i]); i += 1
    if not py or not core: sys.exit(__doc__)
    return py, core, off

PY_RULES = [
    (re.compile(r"orientation -> (\d+)"), lambda m: f"orientation {m[1]}"),
    (re.compile(r"cover (closed|open)"), lambda m: f"cover {m[1]}"),
    (re.compile(r"action: (.+)$"), lambda m: f"action {m[1].strip()}"),
    (re.compile(r"low battery flag -> (True|False)"), lambda m: f"low-battery {m[1].lower()}"),
    (re.compile(r"CRITICAL battery"), lambda m: "poweroff"),
]
CORE_RULES = [
    (re.compile(r"WOULD orientation (\d+)"), lambda m: f"orientation {m[1]}"),
    (re.compile(r"WOULD cover (closed|open)"), lambda m: f"cover {m[1]}"),
    (re.compile(r"WOULD action (.+?)(?: \(rejected.*)?$"), lambda m: f"action {m[1].strip()}"),
    (re.compile(r"WOULD low-battery (true|false)"), lambda m: f"low-battery {m[1]}"),
    (re.compile(r"WOULD poweroff"), lambda m: "poweroff"),
]

def read_python(paths, off):
    out = []
    for p in paths:
        for line in open(p, errors="replace"):
            m = re.match(r"(\d\d):(\d\d):(\d\d)(?:\.(\d{1,3}))?\s+(.*)$", line.rstrip())
            if not m: continue
            h, mi, s, ms, msg = int(m[1]), int(m[2]), int(m[3]), m[4], m[5]
            base = ((h * 60 + mi) * 60 + s) * 1000
            if ms is not None: lo = hi = base + int(ms.ljust(3, "0"))
            else: lo, hi = base, base + 999
            for rx, key in PY_RULES:
                mm = rx.search(msg)
                if mm: out.append((lo, hi, ms is not None, key(mm), line.strip())); break
    return out

def read_core(path, off):
    out, other = [], 0
    day = 86400_000
    for line in open(path, errors="replace"):
        m = re.match(r"(\d{10,})\s+(WOULD .*)$", line.rstrip())
        if not m: continue
        t = (int(m[1]) + off * 60_000) % day          # time of day in the tablet's clock
        for rx, key in CORE_RULES:
            mm = rx.search(m[2])
            if mm: out.append((t, key(mm), line.strip())); break
        else: other += 1
    return out, other

def main():
    py_paths, core_path, off = parse_args(sys.argv[1:])
    py = read_python(py_paths, off)
    core, other = read_core(core_path, off)
    used = set(); bad = 0
    print(f"python events: {len(py)}  rust WOULD events (comparable): {len(core)}  rust-only kinds counted: {other}")
    for lo, hi, has_ms, key, raw in py:
        w = (lo - 300, hi + 300) if not has_ms else (lo - 300, lo + 300)
        hit = next((i for i, (t, k, _) in enumerate(core) if i not in used and k == key and w[0] <= t <= w[1]), None)
        if hit is None:
            bad += 1; print(f"DIVERGE python-only: {raw}")
        else:
            used.add(hit); print(f"PASS    {key}")
    for i, (t, k, raw) in enumerate(core):
        if i not in used:
            bad += 1; print(f"DIVERGE rust-only:   {raw}")
    print(f"{'CLEAN' if bad == 0 else 'DIVERGENCES: %d' % bad}")
    sys.exit(min(bad, 125))

main()
