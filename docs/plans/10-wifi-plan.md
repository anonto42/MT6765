# WiFi bring-up plan — WalPad 8G (MT6765)

Goal: get the tablet onto the internet over its **built-in WiFi**, from the
custom WalPad-Server Linux, with no USB-host dongle involved.

---

## Two assumptions worth correcting first

These came up in earlier planning and both change the amount of work:

**1. The WiFi part is probably *not* an MT7668.**
The MT6765 (Helio A22/P22) has the connectivity subsystem **on-die** —
MediaTek calls it CONNSYS/CONNAC. Its driver is the `gen4m` family
(`wlan_drv_gen4m`), which comes up alongside a connectivity core
(`conninfra` / `wmt_drv`) and is switched on by writing to `/dev/wmtWifi`
rather than by a plain `modprobe`. MT7668 is a *separate* combo chip used on
other boards. Which one this device has decides which firmware blobs and
which bring-up sequence are correct, so **phase 1 measures it instead of
guessing**.

**2. The firmware is not locked behind encryption.**
On this device only `/data` is FBE-encrypted. `/vendor` and `/system` are
plain ext4 and mount read-only from the server ramdisk without any keystore,
vold, or Android userspace. So the firmware blobs and the prebuilt `.ko` are
a `mount -o ro` and a `cp` away — not a reverse-engineering job, and no need
to source blobs from third-party OpenWrt trees.

The one real caveat: `/vendor` does live inside a **`super`** dynamic-partition
image (Android 10+). That turns out not to matter much — `super`'s own metadata
says where `vendor` starts, and busybox `losetup` can map that offset. Phase 2
does exactly that; no `dmsetup`, no `lpdump`, no dumping 4 GB.

---

## The one gate that decides everything

Everything downstream hinges on a single question:

> **Does the kernel currently running in WalPad-Server match the kernel the
> stock `wlan_drv_gen4m.ko` was built against?**

Kernel modules only load into the exact kernel they were compiled for
(`vermagic` must match). So:

| Outcome | Cost | Path |
|---|---|---|
| Server firmware runs the **stock vendor kernel** | hours | Route A — copy module + firmware, load, connect |
| Server kernel differs but **has the driver built in** (`=y`) | hours | Route B — firmware + userspace only |
| Different kernel, driver **not** built in | days | Route C — rebuild the kernel with the MTK driver, repack boot.img |

`scripts/harvest_wifi_fw.sh` prints `uname -r` next to every module's
`vermagic` precisely so you can read this off directly.

---

## Measured (2026-08-19) — what the on-device survey found

Phase 1 is done. The full recon (`data/recon/`) collected all 49 probes across
two boot windows. The plan's gate is answered:

| Question | Measured answer |
|---|---|
| Board | `MT6762V/CB` — a **Helio P22**, not MT6765. Same family, same CONNSYS block, so the gen4m plan stands. |
| Kernel | `4.19.127 #1 SMP PREEMPT ... May 6 2022 CST`, built `nobody@android-build` with an Android config → **the stock vendor kernel**. |
| Driver present? | ~~**Yes, built in.**~~ **Wrong — see the correction below.** What is built in is `cfg80211` + `mac80211` and the `18000000.wifi` platform device. The MediaTek driver itself ships as loadable modules in `/vendor/lib/modules`, and `phy0` belongs to something else entirely, not to the MTK radio. |
| WiFi netdev? | **No `wlan0`.** Only `ccmni*` (modem), `usb0`, `rndis0`, tunnels. The radio registered but no interface exists. |
| `/dev/wmtWifi` | Absent — WMT userspace path not running. |
| Firmware | `firmware_class` path is `/vendor/firmware`, but `/vendor` is **not mounted**, so both the regulatory db (`cfg80211: failed to load regulatory.db`) and the WIFI RAM code can't load. Strong suspect for the missing `wlan0`. |
| Storage | No `/dev/block/by-name/`. `mmcblk0p40` (2 GB) is the likely `super`; `mmcblk0p44` (~28 GB) is `userdata`. 16 GB SD present as `mmcblk1p1`, **not mounted** — but ext4/vfat/msdos all supported. |
| Userspace | `udhcpc`, `ip`, `wget`, `dropbear`, `tar`, `base64` present. **Missing** `wpa_supplicant`, `wpa_passphrase`, `iw`. |
| Kernel config.gz | Not available (`/proc/config.gz` empty). |
| Taint | `512` (`TAINT_UNSIGNED_MODULE`) — an unsigned module was loaded at some point. |

**Conclusion (superseded — kept so the reasoning error stays visible):** Route B
confirmed, the stock kernel already carries the WiFi driver, so no module work.

> **Correction, 2026-08-20.** That conclusion was wrong, and it was wrong because
> `phy0` was read as proof of a built-in MediaTek radio. It is not: `phy0` is a
> different device, and `/proc/modules` was empty only because nothing had been
> insmodded *yet*. The real driver is three loadable modules in
> `/vendor/lib/modules` — `wmt_drv.ko`, `wmt_chrdev_wifi.ko`, `wlan_drv_gen4m.ko` —
> and bringing them up needs considerably more than firmware blobs: an Android
> property service (supplied here by an `LD_PRELOAD` shim), `wmt_loader` and a
> resident `wmt_launcher` to answer the kernel's ROM-patch request, a strict load
> order so the CONNSYS EMI base is set before `wlan_drv_gen4m` arrives, and a
> hand-`mknod`'d `/dev/wmtWifi` because there is no `ueventd`. That whole sequence
> is `scripts/mtk_stack_up.sh`, and it does now produce `wlan0`. The blow-by-blow
> is in `docs/99-status-log.md`.

---

## Phase 0 — stop fighting sudo (5 min)

The transcript so far is mostly failed `sudo` prompts. Fix it once:

```
make udev          # installs scripts/99-walpad.rules, reloads udev
# unplug + replug the tablet
```

The rules tag every `0e8d:*` USB device and its `ttyACM*` consoles with
`uaccess` **and** set `MODE="0666"`. The `uaccess` tag alone was not enough and
is what stalled this twice: it only grants an ACL for a real local seat session
via logind, so from a non-seat session `/dev/ttyACM0` stayed `root:uucp 0660`
and every console attempt fell back to an interactive sudo prompt — inside a
boot window that lasts seconds. The explicit mode removes that failure mode.

After this, `make console`, `make vendor-up`, `make wifi-survey` and mtkclient
all run **without root**.

`udevadm trigger` re-applies the mode to an already-attached device, so no
replug is needed for the permission change itself.

## Phase 1 — survey (one boot window)

```
make wifi-survey
```

`scripts/wifi_survey.py` sends the whole probe batch in one serial session,
frames each command with a sentinel instead of guessing read timeouts, and
saves a transcript to `docs/wifi-survey-<timestamp>.txt`.

What to read out of it:

| Probe | Question it answers |
|---|---|
| `kernel` | exact `uname -r` — the number `vermagic` must match |
| `wifi-chardev` | `/dev/wmtWifi` present ⇒ MTK CONNSYS driver already in the kernel |
| `kconfig` | `CFG80211`/`MTK_COMBO`/`WLAN_DRV` `=y` vs `=m` vs absent |
| `modules-dir` | is there a `/lib/modules` tree at all |
| `dmesg-wifi` | did connsys try to probe and fail on missing firmware |
| `tools` | which of `iw`/`wpa_supplicant`/`udhcpc`/`ip` exist |
| `blockdevs` | is there a real `vendor` partition, or only `super` |
| `fs-support` | does the ramdisk kernel support ext4 (needed to mount vendor) |

**Best case:** `/dev/wmtWifi` exists and `cfg80211` is loaded. Then only firmware
+ `wpa_supplicant` are missing — skip straight to phase 3. (Measured 2026-08-19
and misread: `phy0` is not the MediaTek radio and `/dev/wmtWifi` does not exist
until `wmt_chrdev_wifi.ko` is loaded *and* the node is created by hand. See the
correction above.)

## Phase 2 — make /vendor reachable (no 4 GB dump needed)

> **Correction to the earlier plan.** This section used to say: dd all 4 GB of
> `super` onto the SD card across ~120 boot windows, or catch a BROM window long
> enough to pull 4 GB host-side. Neither is necessary, and both are hours of
> work for something that takes seconds.

`super` is not opaque. It begins with an **LP metadata region** — a geometry
block plus a partition/extent table — that says exactly where each logical
partition (`vendor`, `system`, `product`) sits inside it. That region is a few
KB. Read *that*, and you get a byte offset you can hand straight to
`losetup -o`; the other 4 GB never has to move.

```
make super-map     # pull + parse the metadata, print the layout
make vendor-up     # same, then mount vendor + system read-only on the tablet
```

`scripts/vendor_up.py` does it in stages, each cached under `data/super/`, so a
window that dies mid-way costs nothing:

1. `dd` the first 128 KB of `mmcblk0p40`, `gzip` it **on the device** (it is
   almost all zeros, so it compresses to a few KB), `base64` it over the
   console.
2. Parse it with `scripts/lpmeta.py` — a standalone reader for the LP format,
   so there is no dependency on AOSP's `lpdump`/`lpunpack`. Its extent parser
   keys off the table descriptor's `entry_size` rather than assuming a struct
   packing, and is unit-tested against all three plausible layouts.
3. `losetup -r -o <offset> /dev/loopN /dev/mmcblk0p40` and
   `mount -t ext4 -o ro /dev/loopN /vendor` on the tablet.

Step 3 is the one that matters: the kernel was booted with
`firmware_class.path=/vendor/firmware`, so **mounting vendor at `/vendor` makes
the firmware path resolve for real** — no copying blobs to the SD card, no
`harvest_wifi_fw.sh` shuffle. `regulatory.db` and the WiFi RAM code become
loadable in place.

`system` is mounted at the same time for a second reason: it carries bionic and
`linker64`, which is what the stock `/vendor/bin/hw/wpa_supplicant` needs in
order to run at all. That is a free fallback if the musl bundle in phase 3
misbehaves.

**Caveat this can hit:** a logical partition made of more than one extent can't
be mapped with a plain loop offset — it needs `dmsetup create` with a
dm-linear table. `vendor_up.py` detects that case and says so rather than
mounting something wrong. On a factory image `vendor` is almost always a single
contiguous extent.

**Then the driver has to be told to retry.** A built-in driver that already
failed its firmware load during boot will not try again on its own, so
`wifi_up.sh` now unbinds and rebinds `18000000.wifi` after checking that the
firmware path exists.

## Phase 3 — userspace

The ramdisk has no `wpa_supplicant` and no `iw`. The stock `/vendor` copies are
bionic binaries; they will run **only** if `/system` is mounted too (phase 2
does that), and even then Android's linker config can get in the way.

The reliable answer is a musl build that depends on nothing already on the
device. `tools/build_wifi_userspace.sh` produces one:

```
tools/build_wifi_userspace.sh    # -> data/wifi-userspace/wifi-userspace-aarch64.tar.gz
make wifi-userspace              # push it to /mnt/sd/wifi on the tablet
```

It pulls Alpine's aarch64 `wpa_supplicant` and `iw`, takes the exact set of
shared libraries their `DT_NEEDED` entries name (musl, libnl3, libssl/libcrypto,
dbus, pcsc-lite), and **fails the build if the dependency closure is
incomplete** rather than shipping something that dies with a missing-library
error inside a boot window. Result is ~4.4 MB.

Those binaries name `/lib/ld-musl-aarch64.so.1` as their ELF interpreter, which
the tablet does not have. `push_wifi_userspace.py` therefore installs each
binary as `<name>.real` and writes a wrapper that invokes the bundled loader
with an explicit `--library-path`. `wifi_up.sh` already puts `/mnt/sd/wifi/sbin`
first on `PATH`, so `wpa_supplicant` and `iw` resolve normally.

> **Transfer speed is the sore point here.** ~4.4 MB becomes ~6 MB of base64
> down a CDC-ACM console. `push_file.py`'s 256-byte chunks with a fixed 60 ms
> sleep would take about 25 minutes; `push_wifi_userspace.py` uses 2 KB chunks
> (the tty line discipline caps a canonical-mode line near 4095 bytes) with an
> ack every 32 chunks instead of per-chunk sleeps. If the USB ethernet link
> ever gets fixed, use that instead — it is far faster than either.

## Phase 4 — connect

```
# on the tablet, once, with your network's details:
mkdir -p /mnt/sd/wifi
printf 'SSID="YourNetwork"\nPSK="yourpassword"\n' > /mnt/sd/wifi/wifi.conf

sh /mnt/sd/wifi_up.sh
```

`wifi_up.sh` loads the connectivity modules in dependency order, writes `1` to
`/dev/wmtWifi` to make CONNSYS create the interface, discovers the interface
name if it isn't `wlan0`, runs `wpa_supplicant`, then `udhcpc`, and finishes
with a `ping 1.1.1.1`.

## Phase 5 — persistence

The ramdisk is volatile, so today every reboot loses this. Two options:

- **Cheap:** call `wifi_up.sh` from `/mnt/sd/server/init-extra.sh` (the hook
  `setup_server.sh` already creates) and have `/init` source it.
- **Proper:** unpack the server `boot.img`, add the driver + firmware +
  supplicant into the ramdisk itself, repack, and flash in a BROM window with
  `scripts/instant_catch.py`. Keep the working `boot.img` saved first — a bad
  boot image means BROM-only recovery.

---

## If route C turns out to be necessary

Rebuilding is real work but it is bounded: the MT6765 kernel is 4.19-based and
MediaTek's `wlan_drv_gen4m` source ships in the vendor kernel trees. You need
the matching defconfig, `CONFIG_MTK_COMBO_WIFI`-family options, and the same
toolchain the original was built with (mismatched compilers produce modules
that load but misbehave). Before committing to that, check whether the person
who built WalPad-Server published their kernel tree — inheriting their config
saves most of it.

## What is deliberately not in this plan

- **USB-host WiFi dongle** — excluded by request. It also costs the serial
  console, since MUSB can't be device and host at once on this board.
- **Cellular/ccmni** — needs a SIM, modem firmware, and Android's RIL.

---

## Fallback while WiFi is still down

You already have a working IP path that doesn't need WiFi at all: the RNDIS
and ECM gadgets the server firmware exposes. Getting those routed (host does
NAT, tablet gets a default route through the USB link) gives the tablet real
internet today, and is a *much* shorter path than WiFi if the goal is just
"the tablet can reach the network". `scripts/net_diag.sh` already tests the
host side of that link. Worth doing in parallel — it's also how you'd fetch
the phase 3 binaries onto the device without shuffling the SD card.
