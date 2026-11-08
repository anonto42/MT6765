#!/usr/bin/env python3
"""Replace the ramdisk of an Android boot image (header v2) and fix up header fields."""
import struct, sys, hashlib
src, rd_path, dst = sys.argv[1:4]
d = open(src, "rb").read()
assert d[:8] == b"ANDROID!"
f = list(struct.unpack("<IIIIIIIIII", d[8:48]))
ks, ka, rs, ra, ss, sa, tag, pg, hv, osv = f
assert hv == 2
dtbo_sz, dtbo_off, hdr_sz, dtb_sz = struct.unpack("<IQII", d[1632:1632 + 20])
up = lambda n: (n + pg - 1) // pg * pg
kern = d[pg:pg + ks]
o = pg + up(ks)
old_rd = d[o:o + rs]; o += up(rs)
second = d[o:o + ss]; o += up(ss)
dtbo = d[o:o + dtbo_sz]; o += up(dtbo_sz)
dtb = d[o:o + dtb_sz]
rd = open(rd_path, "rb").read()
hdr = bytearray(d[:pg])
struct.pack_into("<I", hdr, 16, len(rd))
if dtbo_sz:
    struct.pack_into("<Q", hdr, 1636, pg + up(ks) + up(len(rd)) + up(ss))
sha = hashlib.sha1()
for blob in (kern, rd, second):
    sha.update(blob); sha.update(struct.pack("<I", len(blob)))
sha.update(struct.pack("<I", dtbo_sz)) if False else None
sha.update(dtbo); sha.update(struct.pack("<I", len(dtbo)))
sha.update(dtb); sha.update(struct.pack("<I", len(dtb)))
hdr[576:608] = sha.digest().ljust(32, b"\0")
pad = lambda b: b + b"\0" * (up(len(b)) - len(b))
out = bytes(hdr) + pad(kern) + pad(rd) + pad(second) + pad(dtbo) + pad(dtb)
open(dst, "wb").write(out)
print("kernel", ks, "ramdisk", len(old_rd), "->", len(rd), "dtbo", dtbo_sz, "dtb", dtb_sz, "out", len(out))
