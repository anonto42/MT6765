#!/usr/bin/env python3
"""
console.py — talk to the WalPad-Server root shell over /dev/ttyACM0.

The tablet's server Linux exposes a busybox root shell ('/ #') on ttyACM0.
This tool opens that console during a 2008 window.

Usage:
    # one-shot command (non-interactive):
    sudo python3 scripts/console.py --exec "uname -a; cat /proc/version"

    # interactive shell (exit with Ctrl-] ):
    sudo python3 scripts/console.py

    # keep running across 2008 windows, send the command each window:
    sudo python3 scripts/console.py --exec "date" --watch

Needs root to open /dev/ttyACM*.
"""
import os
import sys
import time
import argparse
import select
import serial
import subprocess
import tty
import termios

PORT = "/dev/ttyACM0"
BAUD = 115200


def detect_server():
    try:
        out = subprocess.run(["lsusb"], capture_output=True, text=True, timeout=2).stdout
        return "0e8d:2008" in out
    except Exception:
        return False


def wait_for_server(timeout=180):
    print("Waiting for WalPad-Server window...")
    t0 = time.time()
    while time.time() - t0 < timeout:
        if detect_server():
            print("[%s] server window open" % time.strftime("%H:%M:%S"))
            return True
        time.sleep(0.2)
    print("timed out waiting for 2008 window")
    return False


def read_all(s, seconds=1.0):
    out = b""
    t0 = time.time()
    while time.time() - t0 < seconds:
        if s.in_waiting:
            out += s.read(s.in_waiting)
        time.sleep(0.05)
    return out


def one_shot(cmd, wait_for=True, read_time=6.0):
    if wait_for:
        if not wait_for_server():
            return 1
    try:
        s = serial.Serial(PORT, BAUD, timeout=1, write_timeout=2)
    except Exception as e:
        print("open %s failed: %s" % (PORT, e))
        return 1
    time.sleep(0.3)
    s.reset_input_buffer()
    s.write(b"stty -echo 2>/dev/null\n")
    s.flush()
    time.sleep(0.2)
    s.reset_input_buffer()
    s.write((cmd + "\n").encode())
    s.flush()
    print(read_all(s, read_time).decode(errors="replace"))
    s.close()
    return 0


def interactive():
    if not wait_for_server():
        return 1
    try:
        s = serial.Serial(PORT, BAUD, timeout=1, write_timeout=2)
    except Exception as e:
        print("open %s failed: %s" % (PORT, e))
        return 1
    print("=== WalPad-Server console on %s. Ctrl-] to exit. ===" % PORT)
    old = termios.tcgetattr(sys.stdin.fileno())
    tty.setraw(sys.stdin.fileno())
    try:
        while True:
            r, _, _ = select.select([sys.stdin, s], [], [], 0.2)
            if sys.stdin in r:
                data = os.read(sys.stdin.fileno(), 1024)
                if not data or b"\x1d" in data:  # Ctrl-]
                    break
                try:
                    s.write(data)
                except Exception:
                    break
            if s in r:
                try:
                    data = s.read(s.in_waiting or 1)
                except Exception:
                    break
                if data:
                    os.write(sys.stdout.fileno(), data)
    finally:
        termios.tcsetattr(sys.stdin.fileno(), termios.TCSADRAIN, old)
        s.close()
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exec", help="run one command and print output")
    ap.add_argument("--read-time", type=float, default=6.0, help="seconds to read output (default 6)")
    ap.add_argument("--watch", action="store_true", help="with --exec: run on every window")
    args = ap.parse_args()

    if args.exec:
        while True:
            rc = one_shot(args.exec, wait_for=True, read_time=args.read_time)
            if not args.watch or rc != 0:
                return rc
    return interactive()


if __name__ == "__main__":
    sys.exit(main())