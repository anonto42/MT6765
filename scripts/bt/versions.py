#!/usr/bin/env python3
"""Print "symbol 0xcrc" for every entry of the __versions section of ELF64 LE kernel modules.

Each entry is struct modversion_info { unsigned long crc; char name[56]; } (64 bytes).
With several files, the first CRC seen for a symbol wins; conflicting CRCs go to stderr.
"""
import struct
import sys


def versions(path):
    d = open(path, "rb").read()
    assert d[:4] == b"\x7fELF" and d[4] == 2 and d[5] == 1, f"{path}: not ELF64 LE"
    shoff, = struct.unpack_from("<Q", d, 0x28)
    shentsize, shnum, shstrndx = struct.unpack_from("<HHH", d, 0x3A)

    def sh(i):
        return struct.unpack_from("<IIQQQQIIQQ", d, shoff + i * shentsize)

    stroff = sh(shstrndx)[4]
    for i in range(shnum):
        name_off, _, _, _, off, size, *_ = sh(i)
        end = d.index(b"\0", stroff + name_off)
        if d[stroff + name_off:end] == b"__versions":
            for e in range(off, off + size, 64):
                crc, = struct.unpack_from("<Q", d, e)
                name = d[e + 8:e + 64].split(b"\0", 1)[0].decode()
                yield name, crc & 0xFFFFFFFF


def main():
    seen = {}
    for path in sys.argv[1:]:
        for name, crc in versions(path):
            if name in seen and seen[name] != crc:
                print(f"conflict {name}: {seen[name]:#010x} vs {crc:#010x} ({path})", file=sys.stderr)
            seen.setdefault(name, crc)
    for name in sorted(seen):
        print(f"{name} {seen[name]:#010x}")


if __name__ == "__main__":
    main()
