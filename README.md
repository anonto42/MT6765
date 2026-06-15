# Walton WalPad 8G — MT6765 Recovery & Low-Level Learning Project

A real-world repair of a bricked **Walton WalPad 8G** tablet
(**MediaTek MT6765 / Helio A22**, eMMC, Android 11, A/B partitions) — and, along
the way, a hands-on course in how mobile firmware, bootloaders, and the boot
chain actually work.

This repo holds the stock firmware images, the recovery tooling that was
written to drive the repair, and a documentation set that goes deep on every
concept involved — from "what is a preloader" to "how to patch a Thumb-2
instruction in the bootloader by hand."

> **Status:** Firmware fully reflashed; LK (bootloader) repaired and *patched* to
> raise its low-battery boot gate. The remaining blocker has been a hardware
> **dead-battery lockout**, worked around via Kernel Power-Off Charging (KPOC).
> See [docs/99-status-log.md](docs/99-status-log.md) for the live status.

---

## Repository layout

```
walton/
├── README.md                  ← you are here
├── docs/                      ← the deep-dive documentation (start here)
│   ├── 00-getting-started.md          project init + environment setup
│   ├── 01-device-reference.md         the WalPad 8G / MT6765 hardware facts
│   ├── 02-boot-chain-and-partitions.md  preloader→LK→kernel, A/B, AVB
│   ├── 03-tooling.md                  mtkclient, fastboot, capstone, scatter
│   ├── 04-recovery-journey.md         the actual repair, step by step
│   ├── 05-lk-reverse-engineering.md   patching the LK battery gate (ARM/Thumb)
│   ├── 06-charging-lockout.md         the dead-battery problem + KPOC fix
│   ├── 07-low-level-learning-path.md  how to learn this from scratch
│   ├── 08-rust-bare-metal-arm64.md    writing bare-metal Rust for this SoC
│   ├── 99-status-log.md               running status / field notes
│   └── appendix-a-original-recovery-flow.md   the first-draft plan (kept for history)
├── scripts/                   ← tooling written for this recovery
│   ├── instant_catch.py               race the 2s preloader window, flash in-process
│   ├── flash_b_slot.py                clone slot A images onto slot B
│   ├── boot_kernel.sh                 RAM-boot the stock kernel via fastboot
│   └── charge_keeper.sh               hold the device in KPOC so it charges
├── firmware/MT6765/           ← stock firmware images (boot, lk, super, vbmeta…)
│   ├── lk_a.bin / lk_a.bin.orig / lk_a_patched.bin   bootloader + backups
│   ├── MT6765_Android_scatter.txt     partition map
│   └── … (boot_a.img, super.img, vbmeta_*.img, preloader, …)
├── tools/platform-tools/      ← Android adb + fastboot
├── archive/                   ← original vendor stock-firmware zip (zip password is in the filename)
├── MT6765         → firmware/MT6765        (back-compat symlink)
└── platform-tools → tools/platform-tools   (back-compat symlink)
```

The two top-level symlinks exist only so older absolute paths (and any
long-running script) keep resolving after the restructure. New work should use
the real `firmware/` and `tools/` paths.

> A `charge_keeper.sh` may still exist at the repo root: it is a live copy used
> by an in-progress charging run and relies on the `platform-tools` symlink. The
> canonical, location-independent version is `scripts/charge_keeper.sh`.

## Quick start

New to the project? Read the docs in order, starting with
[docs/00-getting-started.md](docs/00-getting-started.md).

Just need to *do* something:

| I want to… | Run |
|---|---|
| Charge the dead battery | `scripts/charge_keeper.sh 3600` (then plug device, send to KPOC) |
| RAM-boot the stock kernel | `scripts/boot_kernel.sh` (device in fastboot) |
| Flash patched LK | `python3 scripts/instant_catch.py` (replug device into preloader) |
| Mirror images to slot B | `python3 scripts/flash_b_slot.py` |

All scripts now locate the repo from their own path, so they work no matter
where the repo lives.

## ⚠️ Safety

- **Never** flash a `preloader` or `lk` you are unsure of — a bad one means
  BROM-only recovery (or a paperweight). Keep `lk_a.bin.orig` as the escape hatch.
- **Never** do blind PMIC charger pokes on a lithium cell. See
  [docs/06-charging-lockout.md](docs/06-charging-lockout.md).
- Keep the full stock images intact; they are the only known-good restore point.
# walpad.8g-
