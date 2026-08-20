#!/usr/bin/env python3
"""
vendor_up.py — make /vendor available on the tablet without dumping `super`.

The problem: the stock kernel has the MTK WiFi driver built in, but
`firmware_class.path=/vendor/firmware` points at a partition that is never
mounted, so the WiFi RAM code (and regulatory.db) can't load and no `wlan0`
is ever created.

The old plan was to dd all 4 GB of `super` to an SD card across ~120 boot
windows.  That isn't needed.  `super` begins with a small LP metadata region
describing where each logical partition sits inside it.  So:

  stage 1  pull ~128 KB of metadata over the serial console (gzip+base64,
           compresses to a few KB because it is mostly zeros)
  stage 2  parse it here to get vendor's byte offset inside mmcblk0p40
  stage 3  losetup that offset on the tablet and mount it read-only at
           /vendor — the kernel's firmware path now resolves for real
  stage 4  report what WiFi firmware actually showed up, and how the wifi
           platform driver is bound

Every stage caches to data/super/, so a window that dies mid-way costs nothing.
"""
import argparse
import base64
import gzip
import io
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lpmeta
import sparse
from serial_probe import Console, WindowClosed, repo_root, wait_for_window

SUPER_DEV = "/dev/mmcblk0p40"
SD_DIR = "/mnt/sd"
META_BYTES = 128 * 1024
# reserved block + 2 geometry blocks + the first 64 KB metadata slot.  Only
# slot 0 has to be readable; the backup slots sit past it and aren't parsed.
LP_REGION_BYTES = 4096 * 3 + 65536
B0, B1 = "__WPD_B64_START__", "__WPD_B64_END__"


def outdir():
    d = os.path.join(repo_root(), "data", "super")
    os.makedirs(d, exist_ok=True)
    return d


def carve(text):
    """Pull the payload out from between the sentinels, ignoring shell echo.

    Both sentinels appear twice: once in the shell's echo of the command line
    we sent, and once in its actual output.  Anchor on the *last* start marker
    before the *first* end marker, so the echoed command text — which is made
    of perfectly valid base64 characters and would otherwise be spliced onto
    the front of the payload — is left behind.
    """
    if B0 not in text or B1 not in text:
        return None
    head = text.split(B1, 1)[0]
    if B0 not in head:
        return None
    body = head.rsplit(B0, 1)[1]
    return re.sub(r"[^A-Za-z0-9+/=]", "", body)


# ---------------------------------------------------------------- stage 1

def pull_metadata(con, path):
    """dd the front of `super`, gzip it on-device, base64 it over the wire."""
    cmd = (
        "echo %s; "
        "dd if=%s bs=4096 count=%d 2>/dev/null | gzip -9 | base64; "
        "echo %s" % (B0, SUPER_DEV, META_BYTES // 4096, B1)
    )
    out, done = con.run(cmd, timeout=90.0)
    payload = carve(out)
    if not payload:
        raise RuntimeError(
            "no base64 payload came back (window may have closed).\n--- got ---\n"
            + out[-800:]
        )
    raw = base64.b64decode(payload)
    blob = gzip.GzipFile(fileobj=io.BytesIO(raw)).read()
    with open(path, "wb") as f:
        f.write(blob)
    return blob


def read_at(con, offset, length):
    """Read an arbitrary byte range off the tablet's `super` partition.

    dd is asked for whole 4 KiB blocks (bs=1 with a large skip is byte-at-a-time
    on busybox and would take minutes) and the slice is taken here.
    """
    start = (offset // 4096) * 4096
    end = ((offset + length + 4095) // 4096) * 4096
    cmd = ("echo %s; dd if=%s bs=4096 skip=%d count=%d 2>/dev/null | base64; echo %s"
           % (B0, SUPER_DEV, start // 4096, (end - start) // 4096, B1))
    out, _ = con.run(cmd, timeout=60.0)
    payload = carve(out)
    if not payload:
        raise RuntimeError("no data for read at 0x%x\n--- got ---\n%s" % (offset, out[-400:]))
    blob = base64.b64decode(payload)
    return blob[offset - start: offset - start + length]


# ---------------------------------------------------------------- stage 2b

def map_sparse(con, blob):
    """`super` is an unexpanded sparse image; walk its chunk table.

    Only chunk *headers* are fetched — 12 bytes each, 14 of them — so this is a
    handful of round trips, not a data transfer.
    """
    hdr = sparse.parse_header(blob)
    chunks = sparse.walk(hdr, lambda off, n: (
        blob[off:off + n] if off + n <= len(blob) else read_at(con, off, n)))
    return hdr, chunks


def logical_view(blob, hdr, chunks, length):
    """Rebuild the first `length` logical bytes from the sparse framing.

    The LP metadata lives in the first blocks of the logical image, which the
    first RAW chunk covers, so this is a slice rather than a reconstruction.
    """
    first = chunks[0]
    if first["type"] != sparse.CHUNK_RAW or first["logical_offset"] != 0:
        raise RuntimeError("first chunk is %s, not a RAW chunk at logical 0 - "
                           "cannot read LP metadata" % first["type_name"])
    # The sparse framing costs 40 bytes off the front, so the logical view is
    # always slightly shorter than the raw pull.  That is fine: the LP metadata
    # region is ~76 KB, far inside what the first chunk carries.
    avail = min(first["logical_length"], len(blob) - first["data_offset"])
    if avail < LP_REGION_BYTES:
        raise RuntimeError("only %d logical bytes available, need at least %d "
                           "to cover the LP metadata region"
                           % (avail, LP_REGION_BYTES))
    return blob[first["data_offset"]: first["data_offset"] + min(length, avail)]


# ---------------------------------------------------------------- stage 2c

def assembly_plan(part, chunks):
    """Work out how to rebuild one logical partition as a file on the SD card.

    A logical partition can straddle several sparse chunks, so it is not
    contiguous on the eMMC and `losetup -o` alone cannot reach it.  But the
    tablet can copy the RAW pieces into one file on its own SD card at eMMC
    speed, with the DONT_CARE gaps left as holes — nothing crosses the serial
    line.  Returns the list of copy operations.
    """
    lo = part["extents"][0]["byte_offset"]
    size = part["size"]
    ops = []
    for c in chunks:
        cs = c["logical_offset"]
        ce = cs + c["logical_length"]
        if ce <= lo or cs >= lo + size:
            continue
        a, b = max(cs, lo), min(ce, lo + size)
        if c["type_name"] != "RAW":
            # DONT_CARE is unallocated filesystem space; leaving it as zeros is
            # what expanding the sparse image would have produced anyway.
            ops.append({"kind": c["type_name"], "seek": a - lo, "length": b - a})
            continue
        ops.append({
            "kind": "RAW",
            "phys": c["data_offset"] + (a - cs),
            "seek": a - lo,
            "length": b - a,
        })
    return lo, size, ops


def assemble(con, name, size, ops, img):
    """Run the copy on the tablet.

    Each RAW piece is reached with `losetup -o <phys>`, which — unlike `dd`'s
    skip= — does not require the offset to be block aligned.  The sparse chunk
    data offsets are misaligned by the accumulated 12-byte chunk headers, so
    this is what makes a fast bs=1M copy possible at all.
    """
    con.run("rm -f %s; : > %s" % (img, img), timeout=30.0)
    for op in ops:
        if op["kind"] != "RAW":
            print("   hole   %8.1f KB at 0x%x (left as zeros)"
                  % (op["length"] / 1024.0, op["seek"]))
            continue
        blocks = (op["length"] + 1048575) // 1048576
        script = "; ".join([
            "L=$(losetup -f 2>/dev/null)",
            '[ -n "$L" ] || L=/dev/loop0',
            "losetup -r -o %d $L %s 2>&1" % (op["phys"], SUPER_DEV),
            "dd if=$L of=%s bs=1M count=%d seek=%d conv=notrunc 2>&1 | tail -1"
            % (img, blocks, op["seek"] // 1048576),
            "losetup -d $L 2>/dev/null",
            "echo rc=$?",
        ])
        print("   copy   %8.1f MB from phys 0x%x -> img 0x%x"
              % (op["length"] / 1e6, op["phys"], op["seek"]))
        out, _ = con.run(script, timeout=600.0)
        print("      " + out.strip().replace("\n", "\n      "))

    out, _ = con.run(
        "truncate -s %d %s 2>/dev/null || dd if=/dev/zero of=%s bs=1 count=0 seek=%d 2>/dev/null; "
        "ls -l %s" % (size, img, img, size, img), timeout=60.0)
    return out


# ---------------------------------------------------------------- stage 3

def mount_logical(con, name, offset, mountpoint, device=SUPER_DEV):
    """losetup the extent and mount it read-only.

    `device` is the partition itself for a contiguous partition, or the
    reassembled image on the SD card for one that straddles sparse chunks.
    """
    script = "; ".join([
        "mkdir -p %s" % mountpoint,
        # busybox needs the loop nodes to exist; make one if udev didn't
        "[ -e /dev/loop0 ] || for i in 0 1 2 3 4 5 6 7; do "
        "mknod /dev/loop$i b 7 $i 2>/dev/null; done",
        "L=$(losetup -f 2>/dev/null)",
        '[ -n "$L" ] || L=/dev/loop0',
        "losetup -r -o %d $L %s 2>&1" % (offset, device),
        'echo "loop=$L"',
        "mount -t ext4 -o ro $L %s 2>&1" % mountpoint,
        "echo rc=$?",
    ])
    out, _ = con.run(script, timeout=30.0)

    if "rc=0" not in out:
        # fall back to letting busybox mount do the loop setup itself
        out2, _ = con.run(
            "mount -t ext4 -o ro,loop,offset=%d %s %s 2>&1; echo rc=$?"
            % (offset, device, mountpoint),
            timeout=30.0,
        )
        out += "\n-- fallback --\n" + out2

    check, _ = con.run("ls %s 2>&1 | head -20" % mountpoint, timeout=15.0)
    return out, check


# ---------------------------------------------------------------- stage 4

WIFI_REPORT = "; ".join([
    "echo '== mounts'",
    "grep -E 'vendor|system|loop' /proc/mounts",
    "echo '== /vendor/firmware wifi blobs'",
    "ls -l /vendor/firmware 2>&1 | grep -iE 'wifi|wlan|mt6|mt7|regulatory|conn' "
    "|| ls /vendor/firmware 2>&1 | head -40",
    "echo '== wifi platform driver binding'",
    "ls -l /sys/bus/platform/devices/18000000.wifi/driver 2>&1",
    "echo '== phy0 owner'",
    "ls -l /sys/class/ieee80211/phy0/device 2>&1",
    "echo '== netdevs'",
    "ls /sys/class/net",
    "echo '== wmt chardevs'",
    "ls -l /dev/wmtWifi /dev/stpwmt /dev/conninfra_dev 2>&1",
])


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--refresh", action="store_true",
                    help="re-pull the metadata even if it is already cached")
    ap.add_argument("--parts", default="vendor,system",
                    help="comma-separated logical partitions to mount (default: vendor,system)")
    ap.add_argument("--no-mount", action="store_true",
                    help="stop after mapping the layout; don't touch the tablet's mounts")
    ap.add_argument("--watch", action="store_true",
                    help="keep waiting for boot windows until the job completes")
    args = ap.parse_args()

    meta_path = os.path.join(outdir(), "metadata.bin")
    layout_path = os.path.join(outdir(), "layout.json")
    sparse_path = os.path.join(outdir(), "sparse.json")

    # ---- can we skip straight past the device for stages 1-2?
    have_meta = os.path.exists(meta_path) and not args.refresh

    while True:
        try:
            if not have_meta:
                with Console() as con:
                    if not con.open(timeout=180 if args.watch else 60):
                        return 1
                    print("pulling %d KB of super metadata..." % (META_BYTES // 1024))
                    pull_metadata(con, meta_path)
                    print("saved %s (%d bytes)"
                          % (meta_path, os.path.getsize(meta_path)))
                have_meta = True

            with open(meta_path, "rb") as f:
                blob = f.read()

            # `super` on this device is an unexpanded Android sparse image, so
            # LP's offsets (logical) and losetup's offsets (physical) are not
            # the same number.  Detect that and build the translation.
            sparse_hdr = sparse_chunks = None
            try:
                sparse_hdr = sparse.parse_header(blob)
            except sparse.NotSparse:
                pass

            if sparse_hdr:
                if os.path.exists(sparse_path) and not args.refresh:
                    with open(sparse_path) as f:
                        cached = json.load(f)
                    sparse_hdr, sparse_chunks = cached["header"], cached["chunks"]
                else:
                    with Console() as con:
                        if not con.open(timeout=180 if args.watch else 60):
                            return 1
                        print("super is a sparse image - walking its chunk table")
                        sparse_hdr, sparse_chunks = map_sparse(con, blob)
                    with open(sparse_path, "w") as f:
                        json.dump({"header": sparse_hdr, "chunks": sparse_chunks}, f, indent=2)
                print()
                print(sparse.report(sparse_hdr, sparse_chunks))
                meta = lpmeta.parse(logical_view(blob, sparse_hdr, sparse_chunks, 128 * 1024))
            else:
                meta = lpmeta.parse(blob)

            with open(layout_path, "w") as f:
                json.dump(meta, f, indent=2)
            print()
            print(lpmeta.report(meta))
            print()
            print("layout cached -> %s" % layout_path)

            wanted = [p.strip() for p in args.parts.split(",") if p.strip()]
            targets = []
            for name in wanted:
                hit = next((p for p in meta["partitions"]
                            if p["name"] in (name, name + "_a", name + "_b")), None)
                if not hit:
                    print("!! no logical partition named %r in super" % name)
                    continue
                if not hit["contiguous"]:
                    print("!! %s spans %d extents - needs dm-linear, not a plain "
                          "loop offset; skipping" % (hit["name"], len(hit["extents"])))
                    continue

                logical = hit["extents"][0]["byte_offset"]
                if not sparse_chunks:
                    targets.append({"name": hit["name"], "mount": "/" + name,
                                    "device": SUPER_DEV, "offset": logical})
                    continue

                phys, note = sparse.to_physical(
                    sparse_chunks, sparse_hdr["block_size"], logical, hit["size"])
                if phys is not None:
                    print("   %-10s logical 0x%-11x -> physical 0x%x (mount in place)"
                          % (hit["name"], logical, phys))
                    targets.append({"name": hit["name"], "mount": "/" + name,
                                    "device": SUPER_DEV, "offset": phys})
                    continue

                # Straddles sparse chunks, so it is not contiguous on the eMMC.
                # Rebuild it as one file on the tablet's own SD card instead.
                print("   %-10s %s" % (hit["name"], note))
                lo, size, ops = assembly_plan(hit, sparse_chunks)
                targets.append({"name": hit["name"], "mount": "/" + name,
                                "device": "%s/%s.img" % (SD_DIR, name),
                                "offset": 0, "assemble": (size, ops)})

            if args.no_mount:
                return 0

            if not targets:
                print("nothing to mount")
                return 2

            with Console() as con:
                if not con.open(timeout=180 if args.watch else 60):
                    return 1

                if any("assemble" in t for t in targets):
                    out, _ = con.run(
                        "mkdir -p %s; grep -q ' %s ' /proc/mounts || "
                        "mount /dev/mmcblk1p1 %s 2>&1; df -h %s | tail -1"
                        % (SD_DIR, SD_DIR, SD_DIR, SD_DIR), timeout=60.0)
                    print("SD card: %s" % out.strip())

                for t in targets:
                    if "assemble" in t:
                        size, ops = t["assemble"]
                        # the SD card survives reboots even though the ramdisk
                        # does not; a correctly sized image from an earlier run
                        # is reusable as is.
                        have, _ = con.run(
                            "S=$(wc -c < %s 2>/dev/null); echo have=${S:-0}"
                            % t["device"], timeout=20.0)
                        if ("have=%d" % size) in have:
                            print("\n=== %s already staged at %s (%.1f MB), reusing"
                                  % (t["name"], t["device"], size / 1e6))
                            continue
                        print("\n=== reassembling %s -> %s (%.1f MB)"
                              % (t["name"], t["device"], size / 1e6))
                        print(assemble(con, t["name"], size, ops, t["device"]).strip())

                for t in targets:
                    name, mp, dev, off = t["name"], t["mount"], t["device"], t["offset"]
                    print("\n=== mounting %s at %s (%s +0x%x)" % (name, mp, dev, off))
                    out, check = mount_logical(con, name, off, mp, dev)
                    print(out.strip())
                    print("-- contents of %s:" % mp)
                    print(check.strip())

                print("\n=== post-mount WiFi state")
                rep, _ = con.run(WIFI_REPORT, timeout=45.0)
                print(rep.strip())
                with open(os.path.join(outdir(), "wifi-after-mount.txt"), "w") as f:
                    f.write(rep)
            return 0

        except WindowClosed as e:
            print("window closed: %s" % e)
            if not args.watch:
                return 3
            print("waiting for the next one...")
            if not wait_for_window(300):
                return 3
        except Exception as e:
            print("error: %s" % e)
            return 4


if __name__ == "__main__":
    sys.exit(main())
