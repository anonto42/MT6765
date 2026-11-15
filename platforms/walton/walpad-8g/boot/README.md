# Boot Process: Walton WalPad 8G

## Boot Chain
1. MediaTek BootROM executes at power-on.
2. Preloader (`preloader_k65v1_64_bsp.bin`) initializes DRAM and storage.
3. LK (Little Kernel) runs, verifies partition tables and handles fastboot/recovery flags.
4. Android standard boot image loaded from `boot_a` (or `boot_b` in A/B slotting).
5. Kernel 4.19.127 boots with devicetree compiled for board `k65v1_64_bsp`.
6. Ramdisk initializes minimal proc/sys, mounts `userdata` ext4 partition.
7. `/sbin/walpad-init` chroots into the Alpine Linux rootfs.

## Partition Layout
- `boot_a` / `boot_b`: Kernel + initramfs (32MB)
- `dtbo_a` / `dtbo_b`: Devicetree overlay table
- `vbmeta_a` / `vbmeta_b`: Android Verified Boot signatures
- `userdata`: Root filesystem for WalPad OS (ext4)
