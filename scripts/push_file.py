#!/usr/bin/env python3
"""
push_file.py — copy a host file onto the tablet over the serial console.

The SD card lives in the tablet, so the only way to get a script over there
without pulling the card is to type it down the console. This base64-encodes
the file and pastes it in chunks, then decodes on the far side and verifies
the byte count.

Usage:
    scripts/push_file.py scripts/wifi_up.sh /mnt/sd/wifi_up.sh
    scripts/push_file.py --mount scripts/wifi_up.sh /mnt/sd/wifi_up.sh
        (--mount remounts the SD card first, which a reboot always loses)
"""
import os
import sys
import time
import base64
import argparse
import serial

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from console import PORT, BAUD, wait_for_server  # noqa: E402

CHUNK = 256          # base64 chars per write — busybox line buffers are small
MOUNT_CMD = "mkdir -p /mnt/sd; mount /dev/mmcblk1p1 /mnt/sd 2>/dev/null; echo MOUNTED"


def read_until(s, marker, timeout):
    buf = b""
    t0 = time.time()
    mb = marker.encode()
    while time.time() - t0 < timeout:
        if s.in_waiting:
            buf += s.read(s.in_waiting)
            if mb in buf:
                return buf.decode(errors="replace"), True
        else:
            time.sleep(0.02)
    return buf.decode(errors="replace"), False


def push(src, dst, do_mount):
    with open(src, "rb") as f:
        data = f.read()
    b64 = base64.b64encode(data).decode()

    s = serial.Serial(PORT, BAUD, timeout=1, write_timeout=5)
    try:
        time.sleep(0.3)
        s.reset_input_buffer()
        s.write(b"stty -echo 2>/dev/null\n")
        s.flush()
        time.sleep(0.3)
        s.reset_input_buffer()

        if do_mount:
            s.write((MOUNT_CMD + "\n").encode())
            s.flush()
            out, ok = read_until(s, "MOUNTED", 15)
            print("sd mount: %s" % ("ok" if ok else "no response"))

        tmp = "/tmp/.push.b64"
        s.write(("mkdir -p %s; : > %s\n" % (os.path.dirname(dst) or "/", tmp)).encode())
        s.flush()
        time.sleep(0.3)

        total = len(b64)
        for i in range(0, total, CHUNK):
            part = b64[i:i + CHUNK]
            s.write(("printf '%%s' '%s' >> %s\n" % (part, tmp)).encode())
            s.flush()
            time.sleep(0.06)          # let the far-side shell keep up
            sys.stdout.write("\r  sent %d/%d bytes of base64" % (min(i + CHUNK, total), total))
            sys.stdout.flush()
        print()

        s.write(("base64 -d %s > %s 2>/dev/null || uudecode %s; "
                 "chmod +x %s; echo SIZE=$(wc -c < %s)\n"
                 % (tmp, dst, tmp, dst, dst)).encode())
        s.flush()
        out, ok = read_until(s, "SIZE=", 20)
        time.sleep(0.5)
        if s.in_waiting:
            out += s.read(s.in_waiting).decode(errors="replace")

        got = None
        for tok in out.replace("\r", "\n").split("\n"):
            if tok.strip().startswith("SIZE="):
                got = tok.strip()[5:].strip()
        if got is None:
            print("!! no size reply; transfer unverified")
            return 1
        if got.isdigit() and int(got) == len(data):
            print("ok: %s -> %s (%d bytes verified)" % (src, dst, len(data)))
            return 0
        print("!! size mismatch: sent %d, device reports %s" % (len(data), got))
        return 1
    finally:
        s.close()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("src")
    ap.add_argument("dst")
    ap.add_argument("--mount", action="store_true", help="remount the SD card first")
    ap.add_argument("--no-wait", action="store_true", help="skip waiting for the 2008 window")
    args = ap.parse_args()

    if not os.path.isfile(args.src):
        print("no such file: %s" % args.src)
        return 1
    if not args.no_wait and not wait_for_server():
        return 1
    try:
        return push(args.src, args.dst, args.mount)
    except Exception as e:
        print("push failed: %s" % e)
        return 1


if __name__ == "__main__":
    sys.exit(main())
