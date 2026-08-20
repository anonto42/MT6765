#!/usr/bin/env python3
"""
flash_b_slot.py — clone slot-A images onto slot-B for the A/B layout.

Android 11 on the WalPad 8G uses A/B partitions (boot_a/boot_b, vbmeta_a/vbmeta_b,
dtbo_a/dtbo_b, ...).  If only slot A has valid images, this reads each slot-A
partition and writes it to its slot-B counterpart via mtkclient.

Usage:
    sudo python3 scripts/flash_b_slot.py                 # default partition list
    sudo python3 scripts/flash_b_slot.py boot vbmeta dtbo
    sudo python3 scripts/flash_b_slot.py --dry-run

Wraps mtkclient's read/write; the window-race is handled by instant_catch.py.
"""
import os
import sys
import argparse
import subprocess
import tempfile

DEFAULT_PAIRS = [
    ("boot", "boot"),
    ("vbmeta", "vbmeta"),
    ("dtbo", "dtbo"),
    ("logo", "logo"),
    ("vendor_boot", "vendor_boot"),
]

repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
venv = os.path.join(repo, "tools", "mtk", ".venv", "bin", "python")
mtk = os.path.join(repo, "tools", "mtk", "mtk.py")
work = os.path.join(repo, "build")


def mtk(args):
    return subprocess.run([venv, mtk] + args, capture_output=True, text=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("parts", nargs="*", help="partition names (without _a/_b suffix)")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    pairs = [(p, p) for p in args.parts] if args.parts else DEFAULT_PAIRS
    os.makedirs(work, exist_ok=True)

    for pa, pb in pairs:
        a, b = f"{pa}_a", f"{pb}_b"
        img = os.path.join(work, f"{pa}_a.img")
        print(f"== {a} -> {b} ==")
        if args.dry_run:
            print("  (dry run) read", a, "write", b)
            continue
        r = mtk(["r", a, img])
        print("  read:", "ok" if r.returncode == 0 else f"FAIL {r.stderr[-200:]}")
        if r.returncode != 0:
            continue
        w = mtk(["w", b, img])
        print("  write:", "ok" if w.returncode == 0 else f"FAIL {w.stderr[-200:]}")
    print("done")


if __name__ == "__main__":
    main()