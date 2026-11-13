# 06 — The Boot Chain (Power Button to Desktop)

Here is what happens step-by-step from the moment you press the power button until you see the tablet's touch UI:

```text
Power Button
   │
   ▼
1. Bootloader (MediaTek LK)
   │  Loads kernel + ramdisk from partition boot_a
   ▼
2. Linux Kernel 4.19.127
   │  Initializes MT6765 CPU, RAM, eMMC, basic drivers
   │  Unpacks initramfs and runs config/boot/init-os
   ▼
3. init-os (Ramdisk)
   │  Mounts userdata (/dev/mmcblk0p44)
   │  Switches root (chroot) into Alpine and runs /sbin/walpad-init
   ▼
4. /sbin/walpad-init (Alpine)
   │  Starts system services (storage, audio, Wi-Fi, SSH, etc.)
   │  Starts walpad-core (device & app daemon)
   ▼
5. Graphical Session
   │  Xvfb (1280x1280) + fbmirror (/dev/fb0) + openbox + walpad-shell
   ▼
Touch UI Ready!
```

---

## Key Things to Know About This Boot Design

1. **Why `chroot` is used at boot too:**
   - The kernel's initial ramdisk (`init-os`) mounts `userdata` and enters Alpine via `chroot`.
   - Kernel helper threads stay rooted in the ramdisk, which is why certain vendor firmware loads (like Wi-Fi) use `/proc/2/root` bind mounts.
2. **Rescue / Recovery Path:**
   - If the UI ever fails to start, `tools/padui` provides a rescue UI, and USB recovery exposes `192.168.42.1` / `192.168.43.1`.
3. **No Reflashing Needed for Normal Updates:**
   - Because the OS lives on `userdata` as normal files, updates to Rust binaries (`walpad-core`, `walpad-shell`) or scripts can be deployed over SSH without touching `boot_a`.

---

**Next:** [07 — Hardware, Devices & Daemons](07-hardware-devices-and-daemons.md)
