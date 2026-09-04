#!/usr/bin/env python3
"""Wait for WalPad-Server, request bootloader mode, and verify USB mode.

This is deliberately non-destructive: it only writes reboot commands to the
root console and never invokes a fastboot write/flash operation.
"""
import glob, os, subprocess, sys, time

try:
    import serial
except ImportError:
    print("pyserial is required (python3 -m pip install --user pyserial)", file=sys.stderr)
    raise SystemExit(2)

BAUD = 115200

def usb_ids():
    try:
        out = subprocess.check_output(["lsusb"], text=True, stderr=subprocess.DEVNULL)
    except Exception:
        return set()
    return {line.split()[5] for line in out.splitlines() if len(line.split()) > 5 and line.split()[5].startswith("0e8d:")}

def send_reboot():
    for port in sorted(glob.glob("/dev/ttyACM*")):
        try:
            s = serial.Serial(port, BAUD, timeout=.2)
            s.reset_input_buffer()
            s.write(b"stty -echo 2>/dev/null\n")
            s.write(b"sync\n")
            s.write(b"/bin/reboot bootloader\n")
            s.flush()
            time.sleep(.8)
            s.close()
            print("reboot bootloader sent on", port)
            return True
        except Exception:
            continue
    return False

print("Waiting for WalPad-Server (0e8d:2008)...", flush=True)
deadline = time.time() + 180
while time.time() < deadline:
    ids = usb_ids()
    if "0e8d:2008" in ids:
        print("WalPad-Server detected", flush=True)
        if not send_reboot():
            print("No usable serial console", file=sys.stderr)
            raise SystemExit(1)
        break
    time.sleep(.2)
else:
    print("Timed out waiting for server mode", file=sys.stderr)
    raise SystemExit(1)

print("Waiting for fastboot/BROM USB mode...", flush=True)
deadline = time.time() + 20
while time.time() < deadline:
    ids = usb_ids()
    if "0e8d:0003" in ids:
        print("BROM detected (0e8d:0003)")
        raise SystemExit(0)
    if any(x in ids for x in ("0e8d:2000", "0e8d:2001", "0e8d:20ff")):
        print("Preloader detected; fastboot is not available")
        raise SystemExit(3)
    time.sleep(.2)
print("Bootloader request was ignored; device returned to/kept WalPad-Server mode", file=sys.stderr)
raise SystemExit(4)
