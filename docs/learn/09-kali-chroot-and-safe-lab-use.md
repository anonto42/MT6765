# 09 — Kali Chroot & Safe Lab Practice

The **Kali** app on your tablet is powered by `/usr/local/sbin/walpad-kali`, which manages an official **Kali NetHunter (`arm64` minimal)** rootfs inside `/opt/kali/rootfs`.

---

## 1. All `walpad-kali` Commands

You can run these from your normal **Alpine Terminal** (or over SSH):

| Command | What It Does |
| :--- | :--- |
| `walpad-kali status` | Shows if Kali is installed, its disk size, OS version, and whether it's mounted |
| `walpad-kali install` | Downloads the official NetHunter minimal `arm64` tarball (~126 MB), verifies SHA256, and unpacks to `/opt/kali/rootfs` |
| `walpad-kali setup` | Runs `apt-get update` and installs baseline tools (`kali-defaults`, `curl`, `iproute2`, `iputils-ping`, `nmap`, `fish`) |
| `walpad-kali shell` | Opens an interactive shell (`fish` if installed, else `bash`) inside Kali — this is what the **Kali** app icon runs |
| `walpad-kali run CMD...` | Runs a single command inside Kali and exits (e.g., `walpad-kali run nmap --version`) |
| `walpad-kali mount` | Bind-mounts `/dev`, `/dev/pts`, `/proc`, `/sys`, `/tmp`, and `/mnt` into Kali |
| `walpad-kali umount` | Unmounts those bind-mounts cleanly |

---

## 2. How Kali Shares Resources with Alpine

When `walpad-kali` starts, it automatically binds:
- **`/dev`, `/dev/pts`, `/proc`, `/sys`** → Hardware & kernel visibility
- **`/tmp` + `DISPLAY=:0`** → GUI tools launched in Kali can display on your tablet's `Xvfb` screen
- **`/mnt`** → SD card and USB drives are accessible at `/mnt` inside Kali too
- **`/etc/resolv.conf`** → Copies DNS settings from Alpine so internet works

---

## 3. Safe Learning & Lab Habits

- **Only test networks and devices you own** (or have explicit permission to test), such as your own home router, test lab, or local VMs.
- **Keep `walpad-kali` updated** with `apt update && apt upgrade` inside Kali when you have Wi-Fi.
- **Check disk space** (`df -h`) before installing large Kali metapackages — `userdata` has plenty of space (~53 GiB), but keeping track of what you install is a great habit.
- **Use SSH from your PC** (`ssh -F build/ssh/config walpad` → `walpad-kali shell`) when doing longer terminal sessions.

---

**Congratulations!** You now have the complete map of how your WalPad 8G works — from the `aarch64` MT6765 chip up to Alpine Linux, `walpad-shell`, and Kali.
