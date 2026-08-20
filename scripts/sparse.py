#!/usr/bin/env python3
"""
sparse.py — read the Android sparse-image framing that `super` is wrapped in.

Surprise from the device: `/dev/mmcblk0p40` does not contain a raw `super`
image.  It contains an **unexpanded Android sparse image** — magic 0xed26ff3a,
4 KiB blocks, 1048576 blocks (exactly the 4 GiB partition), 14 chunks.  Some
flashing step wrote the sparse file byte-for-byte instead of expanding it.

That matters because every offset in `super`'s own LP metadata is a *logical*
offset into the expanded image, while `losetup` needs a *physical* offset into
the partition.  This module walks the chunk table so the two can be related.

Chunk types:
    RAW        the data is physically present, contiguous
    FILL       a repeated 4-byte pattern, only the pattern is stored
    DONT_CARE  nothing stored at all
    CRC32      a checksum chunk, no logical blocks

Only RAW regions can be loop-mounted in place; the rest are not physically
there to mount.
"""
import struct

SPARSE_MAGIC = 0xED26FF3A
CHUNK_RAW = 0xCAC1
CHUNK_FILL = 0xCAC2
CHUNK_DONT_CARE = 0xCAC3
CHUNK_CRC32 = 0xCAC4
TYPE_NAMES = {
    CHUNK_RAW: "RAW",
    CHUNK_FILL: "FILL",
    CHUNK_DONT_CARE: "DONT_CARE",
    CHUNK_CRC32: "CRC32",
}


class NotSparse(Exception):
    pass


def parse_header(blob):
    if len(blob) < 28:
        raise NotSparse("too short to hold a sparse header")
    (magic, major, minor, file_hdr_sz, chunk_hdr_sz,
     block_size, total_blocks, total_chunks, checksum) = struct.unpack_from(
        "<IHHHHIIII", blob, 0)
    if magic != SPARSE_MAGIC:
        raise NotSparse("magic 0x%08x is not a sparse image" % magic)
    return {
        "major": major, "minor": minor,
        "file_hdr_sz": file_hdr_sz, "chunk_hdr_sz": chunk_hdr_sz,
        "block_size": block_size,
        "total_blocks": total_blocks, "total_chunks": total_chunks,
    }


def walk(hdr, read):
    """Walk the chunk table.

    `read(offset, length) -> bytes` fetches from the *physical* image; only
    chunk headers are ever fetched, so this is 12 bytes per chunk, not data.

    Returns a list of chunk records with both coordinate systems attached.
    """
    chunks = []
    off = hdr["file_hdr_sz"]
    logical_block = 0
    for i in range(hdr["total_chunks"]):
        raw = read(off, hdr["chunk_hdr_sz"])
        if len(raw) < 12:
            raise NotSparse("short read on chunk %d header at 0x%x" % (i, off))
        chunk_type, _, chunk_blocks, total_sz = struct.unpack_from("<HHII", raw, 0)
        data_off = off + hdr["chunk_hdr_sz"]
        chunks.append({
            "index": i,
            "type": chunk_type,
            "type_name": TYPE_NAMES.get(chunk_type, hex(chunk_type)),
            "logical_block": logical_block,
            "blocks": chunk_blocks,
            "logical_offset": logical_block * hdr["block_size"],
            "logical_length": chunk_blocks * hdr["block_size"],
            "data_offset": data_off,
            "total_size": total_sz,
        })
        logical_block += chunk_blocks
        off += total_sz
    return chunks


def to_physical(chunks, block_size, logical_offset, length):
    """Translate a logical byte range to a physical one.

    Returns (physical_offset, note).  `physical_offset` is None when the range
    cannot be mapped — because it is not backed by stored data, or because it
    straddles more than one chunk and so is not contiguous on disk.
    """
    end = logical_offset + length
    covering = [c for c in chunks
                if c["logical_offset"] < end
                and c["logical_offset"] + c["logical_length"] > logical_offset]
    if not covering:
        return None, "no chunk covers logical 0x%x" % logical_offset
    if len(covering) > 1:
        kinds = ", ".join("%s@0x%x" % (c["type_name"], c["logical_offset"])
                          for c in covering)
        return None, "range spans %d chunks (%s) - not contiguous on disk" % (
            len(covering), kinds)

    c = covering[0]
    if c["type"] != CHUNK_RAW:
        return None, ("backed by a %s chunk - that data was never written to "
                      "the partition" % c["type_name"])
    return c["data_offset"] + (logical_offset - c["logical_offset"]), "ok"


def report(hdr, chunks):
    lines = ["sparse image: %d blocks of %d B = %.2f GiB, %d chunks"
             % (hdr["total_blocks"], hdr["block_size"],
                hdr["total_blocks"] * hdr["block_size"] / 1024.0 ** 3,
                hdr["total_chunks"])]
    lines.append("%-3s %-10s %-14s %-10s %s"
                 % ("#", "type", "logical", "size", "physical"))
    for c in chunks:
        phys = "0x%x" % c["data_offset"] if c["type"] == CHUNK_RAW else "-"
        lines.append("%-3d %-10s 0x%-12x %-10s %s"
                     % (c["index"], c["type_name"], c["logical_offset"],
                        _human(c["logical_length"]), phys))
    return "\n".join(lines)


def _human(n):
    for unit in ("B", "K", "M", "G"):
        if n < 1024 or unit == "G":
            return "%.1f%s" % (n, unit)
        n /= 1024.0
