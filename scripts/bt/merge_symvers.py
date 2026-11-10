#!/usr/bin/env python3
"""Write Module.symvers for the symbols a module needs.

usage: merge_symvers.py needed.txt vendor-crcs.txt computed-crcs.txt Module.symvers report.txt

Vendor CRCs (read from modules that load on the running kernel) are authoritative; computed CRCs
(genksyms against the running kernel's headers) fill the gaps.  A symbol may have several computed
CRCs when more than one source file exports it (e.g. mm/slab.c and mm/slub.c; only one is built).
Fails if a needed symbol has no CRC, if vendor and computed CRCs disagree for a needed symbol, or
if a computed-only symbol has more than one candidate CRC.  Every other symbol both sources know
is cross-checked and reported as a wider sample of the method.
"""
import sys


def table(path):
    out = {}
    for line in open(path):
        parts = line.split()
        if len(parts) >= 2:
            out.setdefault(parts[0], set()).add(int(parts[1], 16) & 0xFFFFFFFF)
    return out


def main():
    needed_path, vendor_path, computed_path, symvers_path, report_path = sys.argv[1:6]
    needed = sorted({l.strip() for l in open(needed_path) if l.strip()} | {"module_layout"})
    vendor = {k: min(v) for k, v in table(vendor_path).items()}
    computed = table(computed_path)
    rows, report, bad = [], [], []
    for sym in needed:
        v, c = vendor.get(sym), computed.get(sym, set())
        if v is not None and c:
            if v in c:
                tag = "vendor+computed agree"
            else:
                tag = "MISMATCH"
                bad.append(f"{sym}: vendor {v:#010x} computed {sorted(map(hex, c))}")
            crc = v
        elif v is not None:
            tag, crc = "vendor", v
        elif len(c) == 1:
            tag, crc = "computed", next(iter(c))
        elif c:
            bad.append(f"{sym}: several computed CRCs {sorted(map(hex, c))} and no vendor CRC")
            continue
        else:
            bad.append(f"{sym}: no CRC")
            continue
        rows.append(f"{crc:#010x}\t{sym}\tvmlinux\tEXPORT_SYMBOL\n")
        report.append(f"{sym:32} {crc:#010x}  {tag}\n")
    extra = sorted(s for s in computed if s in vendor and s not in needed)
    agree = [s for s in extra if vendor[s] in computed[s]]
    report.append(f"-- wider cross-check (computed vs vendor, symbols not needed): {len(agree)}/{len(extra)} agree\n")
    for s in extra:
        if vendor[s] not in computed[s]:
            report.append(f"   differs: {s} vendor {vendor[s]:#010x} computed {sorted(map(hex, computed[s]))}\n")
    with open(symvers_path, "w") as f:
        f.writelines(rows)
    with open(report_path, "w") as f:
        f.writelines(report)
    if bad:
        sys.stdout.writelines(report)
        print("FAIL:\n  " + "\n  ".join(bad))
        sys.exit(1)


if __name__ == "__main__":
    main()
