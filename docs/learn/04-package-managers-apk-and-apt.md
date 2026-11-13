# 04 — Package Managers: `apk` (Alpine) vs `apt` (Kali)

A **package manager** downloads, verifies, and installs software (and all its required libraries) from an online repository.

Because your tablet has **two** Linux userlands, you have **two** separate package managers:

| | Normal Terminal (**Alpine**) | **Kali** Terminal |
| :--- | :--- | :--- |
| **Tool** | `apk` | `apt` / `apt-get` |
| **Package format** | `.apk` | `.deb` |
| **Library style** | `musl` libc (tiny, fast) | `glibc` (standard Debian/Kali) |
| **Where files go** | Main OS (`/`) | Kali chroot (`/opt/kali/rootfs`) |

> **Important rule:** Never mix them up. Alpine cannot run `apt`, and Kali cannot run `apk`.

---

## 1. Using `apk` in Normal Terminal (Alpine)

```sh
apk update                  # Refresh list of available packages
apk search htop             # Search for a package
apk add htop                # Install a package
apk del htop                # Remove a package
apk info                    # List all installed packages
```

Keep Alpine clean: only install lightweight system tools you need on the host tablet.

---

## 2. Using `apt` in Kali Terminal

Inside the **Kali** app, you are in a Debian-based Kali NetHunter environment:

```sh
apt update                  # Refresh package index
apt search nmap             # Search for a tool
apt install -y nmap         # Install a tool
apt remove nmap             # Remove a tool
apt list --installed        # List installed packages
```

If you ever need to install Kali's baseline packages for the first time, you can also run from Alpine:
```sh
walpad-kali setup
```
(This runs `apt-get update` and installs `kali-defaults`, `ca-certificates`, `curl`, `iproute2`, `iputils-ping`, `nmap`, and `fish`.)

---

## 3. Why Having Two Separate Systems Is Useful

- **Safety:** If you experiment in Kali and install heavy tools or break a config inside `/opt/kali/rootfs`, your main Alpine tablet OS still boots and works normally.
- **Compatibility:** Many security tools expect standard Debian/Kali (`glibc`) libraries, which Kali provides out of the box, while Alpine stays small and fast for the tablet itself.

---

**Next:** [05 — Storage, Partitions, SD Cards & USB Drives](05-storage-partitions-sd-and-usb.md)
