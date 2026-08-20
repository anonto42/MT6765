#!/usr/bin/env python3
"""
instant_catch.py — race the WalPad 8G boot window and run an mtkclient action
the instant a flashable state (BROM/preloader) appears.

On a flat battery the tablet cycles BROM(0003) -> preloader(2000/2008) -> off.
Each flashable window is short, so we watch USB tightly and launch mtkclient
the moment the target PID appears; mtkclient's own retry loop then grabs it.

Usage:
    sudo python3 scripts/instant_catch.py [--target 0003|2000|2008] [--retries N] \
        [--] <mtkclient-args...>

Examples:
    # flash the patched LK the instant any flashable mode appears
    sudo python3 scripts/instant_catch.py w lk_a firmware/MT6765/lk_a_patched.bin

    # dump the GPT table once BROM appears
    sudo python3 scripts/instant_catch.py --target 0003 printgpt
"""
import os
import sys
import time
import subprocess
import argparse

VID = 0x0E8D
TARGETS = {
    "0003": "BROM",
    "2000": "preloader",
    "2001": "preloader",
    "2008": "WalPad-Server",
    "20ff": "preloader",
}


def detect():
    try:
        import usb.core
        for d in usb.core.find(find_all=True):
            if d.idVendor == VID:
                return format(d.idProduct, "04x")
    except Exception:
        pass
    try:
        out = subprocess.run(["lsusb"], capture_output=True, text=True, timeout=2).stdout
        for line in out.splitlines():
            if "0e8d:" in line:
                pid = line.split()[5].split(":")[1].lower()
                return pid
    except Exception:
        pass
    return None


def main():
    ap = argparse.ArgumentParser(add_help=False)
    ap.add_argument("--target", default="any")
    ap.add_argument("--retries", type=int, default=30)
    ap.add_argument("--help", action="store_true")
    args, rest = ap.parse_known_args()

    if args.help or not rest:
        print(__doc__)
        sys.exit(0)

    targets = ["0003", "2000", "2001", "2008", "20ff"] if args.target == "any" else [args.target]

    repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    venv = os.path.join(repo, "tools", "mtk", ".venv", "bin", "python")
    mtk = os.path.join(repo, "tools", "mtk", "mtk.py")

    print(f"Watching for flashable window ({','.join(targets)}); running:", " ".join(rest))
    seen = set()
    for i in range(args.retries * 10):
        pid = detect()
        if pid not in seen:
            seen.add(pid)
            print(f"[{time.strftime('%H:%M:%S')}] mode={TARGETS.get(pid, pid)}")
        if pid in targets:
            print(f"[{time.strftime('%H:%M:%S')}] window! launching mtkclient: {venv} {mtk} {' '.join(rest)}")
            cmd = [venv, mtk] + rest
            try:
                subprocess.run(cmd, timeout=180)
            except subprocess.TimeoutExpired:
                print("mtkclient timed out")
            print(f"[{time.strftime('%H:%M:%S')}] done; watching for next window...")
        time.sleep(0.1)
    print("gave up")


if __name__ == "__main__":
    main()