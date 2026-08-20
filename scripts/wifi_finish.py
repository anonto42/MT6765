#!/usr/bin/env python3
"""
wifi_finish.py — arm once, then walk the whole bring-up the moment the tablet
appears: stage files, raise the CONNSYS stack, push the supplicant bundle if it
is not already on the SD card, associate, and report the address.

This exists because the tablet needs a physical power-cycle after a failed
power-on attempt (the console wedges: writes to the shell endpoint time out and
the other endpoint is silent).  Waiting for that by hand and then typing four
scripts inside a short boot window does not work, so run this first and plug in
afterwards.

The expensive step is the 4.3 MB bundle, which crosses the console as base64 at
a few KB/s.  It lands on the SD card, which survives reboots, so it is pushed
only when $DEST/sbin/wpa_supplicant.real is missing.
"""
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from serial_probe import Console, WindowClosed, repo_root
import push_wifi_userspace as pw

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = repo_root()
SD = "/mnt/sd"
W = SD + "/wifi"


def put(con, local, remote, mode=None):
    """Copy a small text file across as base64 — no quoting surprises."""
    import base64
    with open(local, "rb") as f:
        b64 = base64.b64encode(f.read()).decode()
    con.run(": > /tmp/.put.b64", timeout=15.0)
    for i in range(0, len(b64), 2048):
        con.s.write(("printf '%%s' '%s' >> /tmp/.put.b64\n" % b64[i:i + 2048]).encode())
        con.s.flush()
        # Round-trip after every chunk.  Without this the tty input buffer
        # overruns, a chunk is truncated mid-string, and the unbalanced quote
        # parks the shell at a `> ` continuation prompt where everything sent
        # afterwards is silently swallowed.
        con.run("true", timeout=30.0)
    out, _ = con.run("base64 -d /tmp/.put.b64 > %s && rm -f /tmp/.put.b64 && "
                     "printf '%%s%%s\\n' PUT _OK" % remote, timeout=60.0)
    if "PUT_OK" not in out:
        raise RuntimeError("could not write %s:\n%s" % (remote, out))
    if mode:
        con.run("chmod %s %s" % (mode, remote), timeout=15.0)


def stage(con):
    """Mount the SD card, then refresh the files everything else is driven from.

    The SD block device enumerates a few seconds after the console answers, so
    this waits for it.  Pushing before the mount succeeds silently fills the
    ramdisk instead, and the real SD card then shadows it — which cost a whole
    debugging session once already.
    """
    out, _ = con.run(
        "mkdir -p %s; i=0; "
        "while [ $i -lt 30 ] && [ ! -b /dev/mmcblk1p1 ]; do sleep 1; i=$((i+1)); done; "
        "grep -q ' %s ' /proc/mounts || mount /dev/mmcblk1p1 %s; "
        "grep -q ' %s ' /proc/mounts && mkdir -p %s && printf '%%s%%s\\n' SD OK"
        % (SD, SD, SD, SD, W),
        timeout=60.0)
    if "SDOK" not in out:
        raise RuntimeError("SD card never mounted at %s:\n%s" % (SD, out))
    put(con, os.path.join(HERE, "mtk_stack_up.sh"), SD + "/mtk_stack_up.sh", "755")
    put(con, os.path.join(HERE, "wifi_assoc.sh"), SD + "/wifi_assoc.sh", "755")
    put(con, os.path.join(ROOT, "data", "wifi", "wifi.conf"), W + "/wifi.conf", "600")
    put(con, os.path.join(ROOT, "data", "wifi", "libwpdprops.so"), W + "/libwpdprops.so", "755")
    print("  staged scripts, wifi.conf and the property shim")


def have_bundle(con):
    out, _ = con.run("test -x %s/sbin/wpa_supplicant.real && "
                     "printf '%%s%%s\\n' HAVE _BUNDLE" % W, timeout=20.0)
    return "HAVE_BUNDLE" in out


def bundle(con):
    if have_bundle(con):
        print("  supplicant bundle already on the SD card")
        return
    tar = os.path.join(ROOT, pw.TARBALL)
    with open(tar, "rb") as f:
        data = f.read()
    import base64
    b64 = base64.b64encode(data).decode()
    print("  pushing %s (%.1f MB -> %.1f MB base64), this is the slow part"
          % (os.path.basename(tar), len(data) / 1e6, len(b64) / 1e6))
    pw.send(con, b64, "/tmp/.wifi.b64")
    pw.unpack(con, "/tmp/.wifi.b64", len(data))
    print(pw.wrappers(con).strip())


def stack(con):
    out, _ = con.run("sh %s/mtk_stack_up.sh 2>&1" % SD, timeout=180.0)
    print(out)
    # The marker cannot appear literally in the command, or the console's own
    # echo of the command satisfies the test regardless of the result.
    chk, _ = con.run("ls /sys/class/net | grep -qx wlan0; "
                     "printf '%s%s\\n' WLAN CHK=$?", timeout=20.0)
    return "WLANCHK=0" in chk


def assoc(con):
    out, _ = con.run("sh %s/wifi_assoc.sh 2>&1" % SD, timeout=180.0)
    print(out)
    ip, _ = con.run("ip -4 addr show wlan0 | "
                    "sed -n 's/.*inet \\([0-9.]*\\).*/ADDR=\\1/p'", timeout=20.0)
    # The echoed command carries a literal "ADDR=\1", so match only a real dotted
    # quad rather than trusting the prefix.
    m = re.search(r"ADDR=(\d+\.\d+\.\d+\.\d+)", ip)
    return m.group(1) if m else None


def main():
    print("waiting for the tablet — power-cycle it now")
    try:
        with Console() as con:
            if not con.open(timeout=3600):
                return 1
            print("[console on %s]" % con.port)
            stage(con)
            if not stack(con):
                print("!! stack did not produce wlan0 — see the trace above")
                return 2
            print("  wlan0 is up")
            bundle(con)
            addr = assoc(con)
            if not addr:
                print("!! associated check failed — no IPv4 on wlan0")
                return 3
            print("\n*** wlan0 has %s ***" % addr)
            out, _ = con.run("dropbear -R -p %s:22 2>&1; sleep 1; "
                             "netstat -ltn 2>/dev/null | grep ':22 '" % addr, timeout=30.0)
            print(out)
            print("\nssh root@%s" % addr)
            return 0
    except WindowClosed as e:
        print("window closed: %s" % e)
        return 4


if __name__ == "__main__":
    sys.exit(main())
