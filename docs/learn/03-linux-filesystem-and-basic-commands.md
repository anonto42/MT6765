# 03 — Linux Filesystem & Basic Commands

Unlike Windows (`C:\`, `D:\`), Linux puts everything into **one single tree** starting at `/` (called **root**).

---

## 1. Important Folders on Your Tablet

| Folder | What Lives Here |
| :--- | :--- |
| `/` | Top of the entire filesystem |
| `/bin`, `/usr/bin`, `/sbin` | Programs and system commands (`ls`, `ip`, `walpad-kali`) |
| `/etc` | Configuration files (e.g., `/etc/walpad/apps.json`, `/etc/resolv.conf`) |
| `/dev` | Hardware device files (screen `/dev/fb0`, storage `/dev/mmcblk0`, etc.) |
| `/proc` & `/sys` | Virtual folders showing live kernel, CPU, memory, and driver info |
| `/run` | Temporary runtime files (e.g., `/run/walpad/control.sock`) |
| `/mnt` | Mounted external storage (SD card, USB drives) |
| `/opt` | Optional large software — your Kali chroot lives at `/opt/kali/rootfs` |
| `/tmp` | Temporary files (also holds X11 display socket `/tmp/.X11-unix`) |

---

## 2. Essential Terminal Commands

Practice these in your normal **Terminal**:

### Moving Around & Reading Files
```sh
pwd                 # Print current working directory
ls -la              # List all files (including hidden ones) with details
cd /etc/walpad      # Change directory
cat apps.json       # Print a file's contents
head -n 20 file     # Show first 20 lines
tail -n 20 file     # Show last 20 lines
```

### Checking System Health
```sh
uname -a            # Kernel version and CPU architecture (aarch64)
df -h               # Free disk space on mounted partitions
free -h             # RAM and swap usage
ps aux              # List all running processes
top                 # Live CPU/memory monitor (press q to quit)
```

### Finding Things
```sh
which python3       # Show full path of a command (if installed)
grep -i "kali" /etc/walpad/apps.json   # Search text inside a file
```

---

## 3. Quick `tmux` Tips (Normal Terminal)

Your normal **Terminal** launches inside `tmux` (a terminal multiplexer):
- You can scroll up/down or split panes.
- Typing `exit` closes the current shell pane.
- You can also open up to **6** separate Terminal windows using the top window bar (`max_instances: 6` in `apps.json`).

---

**Next:** [04 — Package Managers: `apk` (Alpine) vs `apt` (Kali)](04-package-managers-apk-and-apt.md)
