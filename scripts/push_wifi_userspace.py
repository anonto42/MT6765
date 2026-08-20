#!/usr/bin/env python3
"""
push_wifi_userspace.py — put a working wpa_supplicant on the tablet.

The ramdisk has no supplicant, and the vendor copy cannot be used at all:
/vendor/bin/hw/wpa_supplicant needs libnl.so, which ships in the product /
system_ext images this firmware does not carry, so the bionic linker refuses it.
data/wifi-userspace/ carries an Alpine aarch64 build that brings its own
libnl-3 plus a closed set of musl libraries, which sidesteps that entirely.

Those binaries name /lib/ld-musl-aarch64.so.1 as their ELF interpreter, which
does not exist on the tablet.  Rather than write to /lib in a ramdisk, this
drops a wrapper in $W/sbin for each binary that invokes the musl loader
explicitly with the right --library-path.  wifi_assoc.sh prefers $W/sbin
whenever it is populated.

Transfer note: this is ~5.7 MB of base64 down a CDC-ACM console.  push_file.py's
fixed 60 ms per 256-byte chunk would take ~25 minutes; here the chunks are
2 KB and flow control is an ack every 32 chunks, which is a few minutes.  If
the USB ethernet link ever gets fixed, use that instead — it is far faster.
"""
import argparse
import base64
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from serial_probe import Console, WindowClosed, repo_root

# The trimmed bundle: stripped, and with iw / wpa_passphrase / libnl-route /
# libpcsclite_real dropped, none of which wpa_supplicant lists as NEEDED.
# Every byte here crosses a 115200-baud console, so the full 4.6 MB build
# is available as wifi-userspace-aarch64.tar.gz if iw is ever wanted.
TARBALL = "data/wifi-userspace/wifi-userspace-min.tar.gz"
DEST = "/mnt/sd/wifi"

# The tty line discipline caps a canonical-mode line at ~4095 bytes, so chunks
# have to stay comfortably under that.
CHUNK = 2048
ACK_EVERY = 32

BINARIES = ["wpa_supplicant", "wpa_cli"]


def prepare(con):
    out, _ = con.run(
        "mkdir -p /mnt/sd; "
        "grep -q ' /mnt/sd ' /proc/mounts || mount /dev/mmcblk1p1 /mnt/sd 2>&1; "
        "mkdir -p %s && printf '%%s%%s\\n' SD OK" % DEST,
        timeout=30.0,
    )
    if "SDOK" not in out:
        raise RuntimeError("could not prepare %s on the SD card:\n%s" % (DEST, out))
    return out


def send(con, b64, remote_tmp):
    con.run(": > %s" % remote_tmp, timeout=15.0)
    total = len(b64)
    sent = 0
    t0 = time.time()
    pending = 0
    for i in range(0, total, CHUNK):
        part = b64[i:i + CHUNK]
        try:
            con.s.write(("printf '%%s' '%s' >> %s\n" % (part, remote_tmp)).encode())
            con.s.flush()
        except Exception:
            raise WindowClosed("write failed mid-transfer")
        sent += len(part)
        pending += 1
        if pending >= ACK_EVERY:
            # round-trip once per batch: cheap flow control that keeps the
            # far-side shell from dropping input, without a per-chunk sleep
            con.run("true", timeout=30.0)
            pending = 0
        rate = sent / max(0.001, time.time() - t0) / 1024.0
        sys.stdout.write("\r  %d/%d KB of base64  (%.0f KB/s)"
                         % (sent // 1024, total // 1024, rate))
        sys.stdout.flush()
    con.run("true", timeout=30.0)
    print()


def unpack(con, remote_tmp, size):
    out, _ = con.run(
        "base64 -d %s > %s/bundle.tar.gz && rm -f %s; "
        "printf '%%s%%s\\n' SI 'ZE='$(wc -c < %s/bundle.tar.gz)"
        % (remote_tmp, DEST, remote_tmp, DEST),
        timeout=120.0,
    )
    got = None
    for tok in out.replace("\r", "\n").split("\n"):
        if tok.strip().startswith("SIZE="):
            got = tok.strip()[5:].strip()
    if not (got and got.isdigit() and int(got) == size):
        raise RuntimeError("size mismatch after decode: sent %d, device says %r" % (size, got))
    print("  decoded and verified: %d bytes" % size)

    # The SD card is vfat, which has no symlinks, so tar dies on the bundle's
    # lib/libc.musl-aarch64.so.1 -> ld-musl-aarch64.so.1 with "Operation not
    # permitted" *after* extracting everything else.  Ignore that one failure
    # and make the link a real copy instead.  bundle.tar.gz is deliberately
    # kept: it is the expensive thing to reproduce, and the card survives
    # reboots, so a re-run never has to re-transfer it.
    out, _ = con.run(
        "tar xzf %s/bundle.tar.gz -C %s 2>&1 | grep -v symlink; "
        "cd %s/lib && cp -f ld-musl-aarch64.so.1 libc.musl-aarch64.so.1 && "
        "chmod +x %s/sbin/* %s/lib/* && "
        "test -x %s/sbin/wpa_supplicant && printf '%%s%%s\\n' UNPA CKED"
        % (DEST, DEST, DEST, DEST, DEST, DEST),
        timeout=180.0,
    )
    if "UNPACKED" not in out:
        raise RuntimeError("unpack failed:\n%s" % out)


def wrappers(con):
    """Wrap each binary so it runs under the bundled musl loader."""
    parts = []
    for b in BINARIES:
        real = "%s/sbin/%s" % (DEST, b)
        parts.append(
            "mv -f %s %s.real 2>/dev/null; "
            "printf '#!/bin/sh\\nexec %s/lib/ld-musl-aarch64.so.1 "
            "--library-path %s/lib %s.real \"$@\"\\n' > %s; chmod +x %s"
            % (real, real, DEST, DEST, real, real, real)
        )
    parts.append("printf '%s%s\\n' WRAP PED")
    out, _ = con.run("; ".join(parts), timeout=60.0)
    if "WRAPPED" not in out:
        raise RuntimeError("wrapper creation failed:\n%s" % out)

    check, _ = con.run(
        "export PATH=%s/sbin:$PATH; "
        "echo '-- wpa_supplicant:'; wpa_supplicant -v 2>&1 | head -3; "
        "echo '-- wpa_cli:'; wpa_cli -v 2>&1 | head -2" % DEST,
        timeout=60.0,
    )
    return check


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tarball", default=None)
    args = ap.parse_args()

    tar = args.tarball or os.path.join(repo_root(), TARBALL)
    if not os.path.isfile(tar):
        print("missing %s — run tools/build_wifi_userspace.sh first" % tar)
        return 1

    with open(tar, "rb") as f:
        data = f.read()
    b64 = base64.b64encode(data).decode()
    print("bundle %s (%.1f MB -> %.1f MB base64)"
          % (os.path.basename(tar), len(data) / 1e6, len(b64) / 1e6))

    try:
        with Console() as con:
            if not con.open(timeout=180):
                return 1
            print(prepare(con).strip())
            send(con, b64, "/tmp/.wifi.b64")
            unpack(con, "/tmp/.wifi.b64", len(data))
            print(wrappers(con).strip())
        print("\ndone — wifi_assoc.sh will pick these up from %s/sbin" % DEST)
        return 0
    except WindowClosed as e:
        print("\nwindow closed mid-transfer: %s" % e)
        print("re-run; the SD card keeps whatever landed, but the transfer restarts")
        return 3
    except Exception as e:
        print("\nerror: %s" % e)
        return 4


if __name__ == "__main__":
    sys.exit(main())
