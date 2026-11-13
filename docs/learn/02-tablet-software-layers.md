# 02 — Tablet Software Layers & Your Two Terminals

Your WalPad 8G is a standalone Linux tablet. Here is how its software is stacked from the bottom (hardware) to the top (the apps you tap).

---

## 1. The 4 Software Layers

```text
+---------------------------------------------------+
| 4. Apps (Browser, Terminal, Files, Kali, Code)    |  <- /etc/walpad/apps.json
+---------------------------------------------------+
| 3. Display & Touch UI                             |  <- Xvfb + fbmirror + openbox + walpad-shell
+---------------------------------------------------+
| 2. Main OS (Alpine Linux 3.24 + walpad-core)      |  <- Runs on userdata (p44)
+---------------------------------------------------+
| 1. Linux Kernel (4.19.127) & Hardware (MT6765)    |  <- Booted from boot_a
+---------------------------------------------------+
```

1. **Kernel & Hardware:** The Linux `4.19.127` kernel talks directly to the MediaTek MT6765 hardware (screen, touch, Wi-Fi, audio, storage, modem).
2. **Main OS (Alpine Linux 3.24):** A lightweight Linux system stored on `userdata`. It runs `walpad-core`, the background daemon that manages hardware and apps.
3. **Display & Touch UI:**
   - `Xvfb` creates a `1280x1280` virtual screen.
   - `fbmirror` copies it to the real screen (`/dev/fb0`).
   - `openbox` manages app windows.
   - `walpad-shell` draws the touch launcher, dock, on-screen keyboard, mouse pad, and navigation gestures.
4. **Apps:** Configured in `/etc/walpad/apps.json`:
   - `browser` (`firefox-esr`)
   - `terminal` (`st` running `tmux` on Alpine)
   - `files` (`pcmanfm`)
   - `kali` (`st` running `/usr/local/sbin/walpad-kali shell`)
   - `code` (`lite-xl`)
   - `scanner` (`st` running `walpad-scan` / ClamAV)

---

## 2. Why You Have 2 Terminals (Normal vs. Kali)

Both **Terminal** and **Kali** open the same terminal window program (`st`), but they place you inside **two different Linux userlands** sharing the same kernel:

| | **Terminal** (Normal) | **Kali** |
| :--- | :--- | :--- |
| **OS Environment** | **Alpine Linux 3.24** (the host OS) | **Kali NetHunter** (`arm64` chroot) |
| **Where it lives** | Main root `/` | Folder `/opt/kali/rootfs` |
| **Default shell / multiplexer** | `tmux` (with system shell) | `fish` (if installed) or `bash` |
| **Package manager** | `apk` | `apt-get` / `apt` |
| **Best used for** | Managing the tablet, checking hardware, system tasks | Learning/running Kali security & network tools |

### What Is a `chroot`?
Kali is **not** a virtual machine and does **not** replace Alpine.  
A `chroot` ("change root") runs a program while pretending `/opt/kali/rootfs` is its `/` directory:
- **Separate:** Kali has its own libraries, shells, and `apt` packages, so installing tools in Kali will not break Alpine.
- **Shared:** Both use the exact same Linux kernel, hardware (`/dev`), screen (`DISPLAY=:0`), and external drives (`/mnt`).

---

## Try It on Your Tablet

Run this in **Terminal** and then in **Kali** to see the difference:

```sh
cat /etc/os-release
```

- In **Terminal**, it prints **Alpine Linux**.
- In **Kali**, it prints **Kali GNU/Linux**.

---

**Next:** [03 — Linux Filesystem & Basic Commands](03-linux-filesystem-and-basic-commands.md)
