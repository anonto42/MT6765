# 05 — Storage, Partitions, SD Cards & USB Drives

On a PC, disks are usually `/dev/sda` or `/dev/nvme0n1`. On this tablet, storage uses **MMC/SD** block devices (`mmcblk*`) and **USB OTG**.

---

## 1. Internal Storage: eMMC (`mmcblk0`)

Your tablet's internal flash chip is `/dev/mmcblk0`. Like most Android-era MediaTek devices, it is split into many partitions:

- **`boot_a`**: Holds the Linux kernel and initial ramdisk (`init-os`).
- **`userdata` (`/dev/mmcblk0p44`)**: The big partition (~53 GiB, `ext4`). Your entire **Alpine Linux** root filesystem and **`/opt/kali`** live here.
- **`super` (`/dev/mmcblk0p40`)**: Holds Android's read-only logical partitions (`system_a`, `vendor_a`, `product_a`), mounted via `dmlinear` so vendor firmware/daemons (like Wi-Fi/modem blobs) are available.
- **NV partitions (`nvram`, `nvdata`, `nvcfg`, `protect1`, `protect2`, `proinfo`, `persist`)**: Store calibration, MAC addresses, and modem/IMEI data.
  - **Rule:** Never touch or flash partitions manually!

---

## 2. SD Card (`mmcblk1`) & USB Drives

- **SD Card:** Appears as `/dev/mmcblk1` (and partitions like `/dev/mmcblk1p1`).
- **USB OTG Host:** The tablet's USB port uses `musb` and switches to host mode when you plug in a USB OTG adapter/hub with a flash drive, keyboard, or USB Wi-Fi dongle.
- **`walpad-storage`:** Automatically mounts SD cards and USB drives (including `exFAT` and `NTFS` via FUSE) under `/mnt`.
- Because `walpad-kali` bind-mounts `/mnt` into `/opt/kali/rootfs/mnt`, any SD card or USB drive you plug in is visible in **both** Terminal and Kali.

---

## Try It on Your Tablet

```sh
lsblk               # Show block devices (mmcblk0, mmcblk1, USB drives)
df -h               # Show mounted filesystems and free space
ls -la /mnt         # See mounted SD card / USB drives
```

---

**Next:** [06 — The Boot Chain (Power Button to Desktop)](06-boot-chain-power-to-desktop.md)
