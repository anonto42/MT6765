#!/usr/bin/env python3
"""
lpmeta.py — minimal parser for Android's `super` (dynamic partition) metadata.

Why this exists: the previous plan was to dd all 4 GB of `super` off the tablet
across ~120 boot windows.  That is unnecessary.  `super` starts with a small
LP metadata region (geometry + a partition/extent table, a few KB) that says
exactly where each logical partition — `vendor`, `system`, `product` — lives
inside it.  Pull those few KB, parse them here, and you get byte offsets you
can hand straight to `losetup -o`.

No dependency on AOSP's lpdump/lpunpack; the on-disk format is stable and
small enough to read directly.

Layout of the front of `super`:
    0x0000  PARTITION_RESERVED_BYTES (4096)
    0x1000  primary geometry   (4096)
    0x2000  backup geometry    (4096)
    0x3000  metadata slot 0    (geometry.metadata_max_size)
    ...     further slots
"""
import struct
import sys
import json

GEOMETRY_MAGIC = 0x616C4467
HEADER_MAGIC = 0x414C5030
RESERVED = 4096
GEOMETRY_SIZE = 4096

TARGET_TYPE_LINEAR = 0
SECTOR = 512


class BadMetadata(Exception):
    pass


def parse_geometry(blob):
    """Read the primary geometry, falling back to the backup copy."""
    for off in (RESERVED, RESERVED + GEOMETRY_SIZE):
        if len(blob) < off + 52:
            continue
        magic, struct_size = struct.unpack_from("<II", blob, off)
        if magic != GEOMETRY_MAGIC:
            continue
        # magic, struct_size, checksum[32], metadata_max_size,
        # metadata_slot_count, logical_block_size
        metadata_max_size, slot_count, logical_block_size = struct.unpack_from(
            "<III", blob, off + 8 + 32
        )
        return {
            "metadata_max_size": metadata_max_size,
            "metadata_slot_count": slot_count,
            "logical_block_size": logical_block_size,
            "geometry_offset": off,
        }
    raise BadMetadata("no valid LP geometry found (is mmcblk0p40 really `super`?)")


def _descriptor(blob, off):
    o, n, sz = struct.unpack_from("<III", blob, off)
    return {"offset": o, "num_entries": n, "entry_size": sz}


def parse_metadata(blob, geom):
    """Parse the first metadata slot that has a valid header."""
    slot_size = geom["metadata_max_size"]
    base = RESERVED + 2 * GEOMETRY_SIZE
    for slot in range(max(1, geom["metadata_slot_count"])):
        off = base + slot * slot_size
        if len(blob) < off + 128:
            break
        if struct.unpack_from("<I", blob, off)[0] != HEADER_MAGIC:
            continue

        header_size = struct.unpack_from("<I", blob, off + 8)[0]
        # magic(4) major(2) minor(2) header_size(4) header_checksum(32)
        # tables_size(4) tables_checksum(32) then four table descriptors
        d = off + 4 + 2 + 2 + 4 + 32 + 4 + 32
        partitions = _descriptor(blob, d)
        extents = _descriptor(blob, d + 12)
        groups = _descriptor(blob, d + 24)
        block_devices = _descriptor(blob, d + 36)

        tables = off + header_size
        return _read_tables(blob, tables, partitions, extents, block_devices, slot)
    raise BadMetadata("no valid LP metadata header in any slot")


def _read_tables(blob, tables, partitions, extents, block_devices, slot):
    # LpMetadataExtent is {u64 num_sectors; u32 target_type; u64 target_data;
    # u32 target_source}.  Whether the compiler padded it to 32 bytes or packed
    # it to 24 is not something to guess at — the table descriptor carries the
    # real entry_size, so pick the layout from that.
    esz = extents["entry_size"]
    if esz >= 28:
        ext_fmt = "<QI4xQI"      # padded: 4 bytes of alignment after target_type
    elif esz >= 24:
        ext_fmt = "<QIQI"        # packed
    else:
        raise BadMetadata("extent entry_size %d matches no known layout" % esz)

    ext = []
    for i in range(extents["num_entries"]):
        o = tables + extents["offset"] + i * esz
        num_sectors, target_type, target_data, target_source = struct.unpack_from(
            ext_fmt, blob, o
        )
        ext.append(
            {
                "num_sectors": num_sectors,
                "target_type": target_type,
                "target_data": target_data,
                "target_source": target_source,
            }
        )

    devs = []
    for i in range(block_devices["num_entries"]):
        o = tables + block_devices["offset"] + i * block_devices["entry_size"]
        first_logical_sector, alignment, alignment_offset, size = struct.unpack_from(
            "<QIIQ", blob, o
        )
        name = blob[o + 24 : o + 24 + 36].split(b"\x00")[0].decode(errors="replace")
        devs.append(
            {
                "name": name,
                "first_logical_sector": first_logical_sector,
                "size": size,
            }
        )

    parts = []
    for i in range(partitions["num_entries"]):
        o = tables + partitions["offset"] + i * partitions["entry_size"]
        name = blob[o : o + 36].split(b"\x00")[0].decode(errors="replace")
        attributes, first_extent, num_extents, group_index = struct.unpack_from(
            "<IIII", blob, o + 36
        )

        pieces = []
        total = 0
        for e in ext[first_extent : first_extent + num_extents]:
            if e["target_type"] != TARGET_TYPE_LINEAR:
                pieces.append({"type": "non-linear", "sectors": e["num_sectors"]})
                continue
            pieces.append(
                {
                    "type": "linear",
                    "byte_offset": e["target_data"] * SECTOR,
                    "byte_length": e["num_sectors"] * SECTOR,
                }
            )
            total += e["num_sectors"] * SECTOR

        parts.append(
            {
                "name": name,
                "attributes": attributes,
                "extents": pieces,
                "size": total,
                # contiguous single-extent partitions are the easy case: they
                # can be mapped with a plain `losetup -o`, no dm-linear needed
                "contiguous": len(pieces) == 1 and pieces[0]["type"] == "linear",
            }
        )

    return {"slot": slot, "block_devices": devs, "partitions": parts}


def parse(blob):
    geom = parse_geometry(blob)
    meta = parse_metadata(blob, geom)
    meta["geometry"] = geom
    return meta


def human(n):
    for unit in ("B", "K", "M", "G"):
        if n < 1024 or unit == "G":
            return "%.1f%s" % (n, unit)
        n /= 1024.0


def report(meta):
    lines = []
    lines.append("metadata slot %d, logical block size %d"
                 % (meta["slot"], meta["geometry"]["logical_block_size"]))
    for d in meta["block_devices"]:
        lines.append("block device %-12s first_logical_sector=%d size=%s"
                     % (d["name"], d["first_logical_sector"], human(d["size"])))
    lines.append("")
    lines.append("%-20s %-10s %-14s %s" % ("partition", "size", "offset", "map"))
    for p in meta["partitions"]:
        if p["contiguous"]:
            off = "0x%x" % p["extents"][0]["byte_offset"]
            how = "losetup -o"
        else:
            off = "(%d extents)" % len(p["extents"])
            how = "dm-linear"
        lines.append("%-20s %-10s %-14s %s" % (p["name"], human(p["size"]), off, how))
    return "\n".join(lines)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit("usage: lpmeta.py <super-metadata.bin> [--json]")
    with open(sys.argv[1], "rb") as f:
        meta = parse(f.read())
    if "--json" in sys.argv:
        print(json.dumps(meta, indent=2))
    else:
        print(report(meta))
