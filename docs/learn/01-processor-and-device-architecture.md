# 01 — Processor & Device Architecture (PC vs. Mobile)

Before diving into Linux commands or Kali, it helps to understand **what hardware is inside your tablet** and how it differs from a regular PC.

---

## 1. What Is a Processor Architecture?

Every CPU understands a specific set of binary instructions called an **ISA (Instruction Set Architecture)** — think of it as the CPU's native language.

If a program is compiled for one CPU architecture, it **cannot** run directly on a different architecture without emulation.

| Architecture | Common Names | Where It's Used | Main Strength |
| :--- | :--- | :--- | :--- |
| **x86-64** | `x86_64`, `amd64` | Windows/Linux PCs, laptops, servers | High raw speed, high power & cooling |
| **ARM (64-bit)** | `arm64`, `aarch64` | Smartphones, tablets, Apple M-series, Raspberry Pi | High efficiency per watt (battery-friendly) |
| **ARM (32-bit)** | `armv7`, `armhf` | Older phones, routers, older embedded boards | Legacy 32-bit mobile/embedded |
| **RISC-V** | `riscv64`, `rv32` | Newer microcontrollers, research, open hardware | Open-source instruction set |
| **Microcontrollers** | `ARM Cortex-M`, `Xtensa` (ESP32) | Sensors, smart plugs, keyboards | Tiny, ultra-low power, no full OS |

---

## 2. PC Hardware vs. Mobile/Tablet Hardware

Even when both a PC and a tablet run Linux, they are built very differently:

### PC (Modular Design)
- CPU, RAM, GPU, Wi-Fi card, and SSD are separate parts plugged into a motherboard.
- Uses **BIOS / UEFI** to start up, making it easy to boot standard OS installers.

### Mobile / Tablet (SoC — System on a Chip)
- Almost everything is integrated into **one main chip** called an **SoC**: CPU cores, GPU, memory controller, display controller, Wi-Fi/Bluetooth block, and cellular modem.
- Instead of BIOS/UEFI, mobile devices use a device-specific **bootloader** and fixed **flash partitions** on soldered **eMMC** storage.
- Drivers are tightly coupled to the vendor's Linux kernel.

---

## 3. Your Tablet: Walton WalPad 8G

From the project hardware notes (`.agents/shared/memory/hardware.md`):
- **SoC:** MediaTek **MT6765** (Helio P35)
- **CPU Architecture:** **`aarch64`** (64-bit ARM)
- **Linux Kernel:** `4.19.127` (built for this MediaTek chip)
- **Internal Storage:** eMMC (`mmcblk0`)

### Why This Matters for You
Because your tablet is **`aarch64` (`arm64`)**:
1. Both **Alpine Linux** (your main OS) and **Kali** (your chroot) must use **`aarch64` / `arm64`** software.
2. When you install packages with `apk` or `apt-get`, they automatically download `aarch64`/`arm64` binaries.
3. Programs compiled only for a PC (`x86_64` / `amd64`, or Windows `.exe` files) will not run natively here.

---

## Try It on Your Tablet

Open the normal **Terminal** app and run:

```sh
uname -m        # Prints: aarch64
lscpu           # Shows CPU architecture, cores, and details
```

---

**Next:** [02 — Tablet Software Layers & Your Two Terminals](02-tablet-software-layers.md)
