# Status log

Running field notes for the WalPad 8G recovery/server project.

## 2026-08-27 — internal userdata converted to 52.5 GiB Alpine VPS storage

The unmounted Android `userdata` partition was positively identified as
`/dev/mmcblk0p44` (GPT `PARTNAME=userdata`, 57,299,402,752 bytes). With explicit
approval to erase the old Android data, it was formatted ext4 and the working
Alpine 3.24.1 installation was migrated from its 3 GiB SD loop image.

Before formatting, the primary/backup GPT regions and the first/last 64 MiB of
userdata were saved under `firmware/MT6765/backups/storage/` on the host and
`/mnt/sd/walpad-backup/storage/` on the SD card, with matching SHA-256 hashes.
These are structural recovery artefacts, not a backup of the erased Android
files.

After a real reboot, `/dev/mmcblk0p44` mounted automatically at `/mnt/vps` with
52.5 GiB total and 51.8 GiB free. Alpine, Python 3.14.7, Node.js 24.18.1,
Tailscale, WiFi and SSH all restarted successfully. Passwordless telnet remains
disabled. The verified old 3 GiB SD loop image was then removed, leaving 12.8
GiB free on the SD card.

## 2026-08-27 — boot persistence verified end-to-end

Persistence is fixed. The exact active `boot_a` partition (`mmcblk0p23`, 32
MiB) was backed up to both the host and SD card before modification. The live
partition and backup matched at:

    cadb95e22847966891f699aef0fe8b73aa809d2e11187b836ccf0b8bcd649be1

The Android boot-image v2 ramdisk now contains `/wifi_boot.sh`, launched by
`/init` asynchronously after the initial `mdev -s`. It waits for the SD block
device, refuses to write into an unmounted `/mnt/sd`, mounts the card, restores
the authorized key and Dropbear host key, runs `mtk_stack_up.sh` followed by
`wifi_assoc.sh`, and appends diagnostics to `/mnt/sd/wifi/boot.log`. Running it
asynchronously preserves the USB recovery console if the SD card or WiFi fails.

The full-partition candidate and the flashed device both matched at:

    d4a2e86f95c85d4c7c78f9917ccb37c396c1b040f0606a354d0881dd5141b3cf

Reboot verification was performed without issuing any serial command or
running `make wifi-up`. The tablet mounted `/vendor` and `/system`, initialized
CONNSYS in the required order, associated with the configured WPA network,
obtained `192.168.0.107`, reached `1.1.1.1`, and exposed key-authenticated SSH.
The live and SD copies of the Ed25519 host key also hash identically.

Recovery copies on the SD card:

    /mnt/sd/walpad-backup/boot_a-original.img
    /mnt/sd/walpad-backup/boot_a-persistent.img

The DHCP address changed from `.105` to `.107` during testing. A router-side
reservation remains the correct fix for a stable address.

## 2026-08-18 — host-side recon; console access is the one blocker

### The device is up and stable
Unlike the earlier low-power cycling, `0e8d:2008` now stays enumerated. Full
host-side inventory captured in `data/host/` (`make host-recon`).

### What the USB descriptors reveal
- `iManufacturer` **Walton**, `iProduct` **WalPad-Server**, `iSerial` `0001`
- **`bcdDevice 4.19`** — the gadget reports the kernel version, so the server
  firmware runs a **4.19** kernel. That is the same series as the stock MT6765
  Android kernel, which is a good sign for loading stock `.ko` modules (the
  vermagic still has to match exactly — `harvest_wifi_fw.sh` checks).
- 8 interfaces in one config: 2× CDC-ACM (`ttyACM0`, `ttyACM1`),
  1× CDC-ECM, 1× RNDIS. Bus powered, `MaxPower 2mA`.

### Why the USB network never worked
Host kernel log shows, repeatedly, on **both** links:

    cdc_ether  3-1:1.4 enp10s0f3u1i4: NETDEV WATCHDOG: transmit queue 0 timed out
    rndis_host 3-1:1.6 enp10s0f3u1i6: NETDEV WATCHDOG: transmit queue 0 timed out

The host cannot transmit — the gadget side is not draining the bulk OUT
endpoint. Meanwhile `rx_packets=10` on both interfaces, so the tablet *is*
sending to us. Conclusion: the gadget's ethernet functions are bound and the
endpoints exist, but nothing on the tablet has brought the corresponding
`usb0`/`rndis0` interface up and given it an address. That is fixable from the
serial console — it is not a driver or cabling fault.

### The blocker: console permissions
`/dev/ttyACM0` is `root:uucp 0660`; the user is not in `uucp` and no udev rule
is installed, so every console attempt needs an interactive sudo password.
This is what stalled the previous session repeatedly.

Fix, one time:

    make udev        # installs scripts/99-walpad.rules
    # unplug + replug

After that the console, recon, and mtkclient all run unprivileged.

### Tooling added
- `scripts/serial_probe.py` — shared console engine: waits for the window,
  frames each command with a sentinel (no more guessed `--read-time`), and
  raises `WindowClosed` cleanly when the tablet drops off the bus.
- `scripts/device_recon.py` — 49-probe catalogue across identity, power,
  storage, kernel, wifi, net, userspace, hw. Writes each result to
  `data/recon/<group>/<name>.txt` the moment it lands, and **resumes**: a
  re-run collects only what is still missing, so a short boot window is never
  wasted. Generates `INDEX.md` + `KEY-FACTS.md`.
- `scripts/host_recon.sh` — the no-root host-side collection above.
- `scripts/push_file.py` — base64 a host file down the console, size-verified.
- `scripts/harvest_wifi_fw.sh`, `scripts/wifi_up.sh` — on-tablet WiFi work.
- `wifi_survey.py` is now a thin front-end over the recon catalogue, so there
  is one engine and one probe list rather than two.

### Next steps
1. `make udev`, replug.
2. `make recon-watch` — fills `data/recon/` across however many windows it takes.
3. Read `data/recon/KEY-FACTS.md`: kernel version vs. stock, `/dev/wmtWifi`
   presence, battery state, partition layout.
4. Bring up `usb0` on the tablet to get IP-over-USB working (fixes the TX
   watchdog, and gives a path to copy binaries without the SD card).
5. Then the WiFi plan in `docs/10-wifi-plan.md`.

### 2026-08-19 — device-side WiFi survey complete

Full 49-probe recon completed in two boot windows (`make recon-watch`). See
`docs/10-wifi-plan.md` for the measured results. Key findings:
- The stock kernel (4.19.127) already carries the WiFi driver built in (`phy0`
  registered, `cfg80211` + `mac80211` loaded), but `wlan0` does not exist —
  the firmware from the unmounted `/vendor` partition (likely inside `super`,
  `mmcblk0p40`) is still needed, plus static `wpa_supplicant`/`iw`.
- The USB ethernet gadget still shows the watchdog timeout on the host;
  `usb0`/`rndis0` are up but the tablet side isn't draining the bulk OUT.
- The `ttyACM0` ACL gap from the udev rules is still present; `make udev`
  + replug resolves it, but from this session sudo is needed.
- A 16 GB SD card (`mmcblk1p1`) is present on the device and can be mounted
  from the server shell; a `super` dump host-side or incremental on-device `dd`
  are the two options to get the firmware blobs.
- Userspace confirmed: `udhcpc`, `ip`, `wget`, `dropbear`, `tar`, `base64`
  present; `wpa_supplicant`, `iw`, `openssl`, `python` missing.
- The GPT partition table has no `/dev/block/by-name/`; full list in
  `data/recon/storage/partitions.txt`.

### 2026-08-19 (later) — dropped the 4 GB dump; `/vendor` is a loop mount away

The plan on the table was to copy all 4 GB of `super` off the tablet, either in
32 MB chunks across ~120 boot windows or in one long BROM window. That was the
wrong shape of solution. `super` starts with a small LP metadata region that
records where each logical partition lives inside it; reading a few KB of that
gives `vendor`'s byte offset, and busybox `losetup -o` maps it directly. The
other 4 GB never has to move.

Mounting it at `/vendor` also does more than give access to the blobs: the
kernel booted with `firmware_class.path=/vendor/firmware`, so the firmware path
starts resolving on its own — no copying anything to the SD card.

Added:
- `scripts/lpmeta.py` — standalone LP-metadata reader (no AOSP `lpdump`
  dependency). Its extent parser picks the struct layout from the table
  descriptor's `entry_size` instead of assuming one; unit-tested against the
  packed (24 B), and both padded (28 B / 32 B) layouts.
- `scripts/vendor_up.py` — pulls the metadata over the console
  (`dd | gzip | base64`, a few KB because it is mostly zeros), parses it,
  then `losetup` + `mount -o ro` for `vendor` and `system`. Stages cache under
  `data/super/`, so a window dying mid-way costs nothing. Detects the
  multi-extent case and refuses rather than mounting something wrong.
- `tools/build_wifi_userspace.sh` — builds a musl `wpa_supplicant` + `iw`
  bundle from Alpine aarch64 packages, and **fails if the `DT_NEEDED` closure
  is incomplete** rather than shipping something that dies mid-window.
  Verified: 4.4 MB, closure complete.
- `scripts/push_wifi_userspace.py` — pushes it, and installs loader wrappers
  (the binaries name `/lib/ld-musl-aarch64.so.1`, which the tablet lacks).
- `wifi_up.sh` now checks the firmware path, unbinds/rebinds `18000000.wifi` so
  a driver that already failed its boot-time firmware load retries, falls back
  to `iw phy phy0 interface add wlan0` when a wiphy exists with no netdev
  (which is this board's exact state), and starts dropbear on the WiFi address
  at the end.

**The udev rule was the real recurring blocker, and it was wrong.** It used
`TAG+="uaccess"` only, which grants an ACL through logind for a *local seat*
session. From a non-seat session it does nothing, so `/dev/ttyACM0` stayed
`root:uucp 0660` and every console attempt hit an interactive sudo prompt.
Rule now also sets `MODE="0666"`. Needs one `sudo` to install; after that,
none.

Still unverified against hardware: everything above is written and
self-tested host-side, but nothing has touched the tablet yet, because the
console is still permission-blocked pending that one rule install.

## 2026-08-16 — WalPad-Server USB gadget discovered; BROM mode reached

### Device is NOT in a plain MTK preloader
When connected, the tablet enumerates as **`0e8d:2008` "Walton / WalPad-Server"**
with an unusual USB gadget configuration:

- 2× CDC-ACM serial pairs → `/dev/ttyACM0`, `/dev/ttyACM1`
- 1× CDC-ECM ethernet → `enp10s0f3u1i4`
- 1× RNDIS ethernet → `enp10s0f3u1i6`

That interface set (serial + RNDIS/ECM) is the signature of a **Linux USB
gadget** (configfs `g_serial` + `g_ether`), i.e. someone already built a
"server mode" firmware for this tablet that exposes consoles + a USB network
link so the host can reach it like a little server.

### The tablet is power-limited and boot-cycles
With the dead battery, the device cycles:

    BROM (0e8d:0003) -> preloader (0e8d:2000) -> WalPad-Server (0e8d:2008) -> off -> repeat

Observed live with `scripts/detect_mode.sh`. Each window is short. This is the
classic low-power boot loop; the documented dead-battery lockout is still the
root blocker.

### What responds / what doesn't
- **mtkclient handshake fails** on the 2008 gadget (it is not a flashable MTK
  preloader; also PID `0x2008` was missing from mtkclient's `usb_ids.py` — fixed
  in `tools/mtk/mtkclient/config/usb_ids.py`).
- **Serial consoles silent** (no reply, bulk writes time out).
- **USB ethernet links UP but no DHCP/ARP reply** while in 2008.
- **BROM mode (0003) is reachable** (hold Vol Up + Power + plug USB), so
  mtkclient *can* talk to the SoC — needs a catch of the short BROM window.

### Actions taken
- Cloned mtkclient into `tools/mtk` (venv at `tools/mtk/.venv`).
- Added PID `0x2008` to mtkclient device table.
- Built `scripts/detect_mode.sh`, `scripts/probe_server.py`,
  `scripts/instant_catch.py`, `scripts/flash_b_slot.py`,
  `scripts/boot_kernel.sh`, `scripts/charge_keeper.sh`, `scripts/usb_utils.sh`.
- Added `Makefile` automation layer.

### Next steps
1. Confirm whether the 2008 state is a live Linux server (probe ARP/serial in
   the window) — `scripts/probe_server.py` (needs root).
2. If alive: bring up RNDIS, find its IP, SSH/NAS over USB.
3. If inert: charge the battery (KPOC / strong supply), then boot to server.
4. Verify the patched LK / slot-A images are intact via `mtkclient printgpt`
   during a BROM window.
## 2026-08-20 — CONNSYS stack fully initialises; wlan firmware start still fails

### Corrections to earlier entries
- The "Route B — the driver is built in" conclusion in `10-wifi-plan.md` is
  **wrong**.  The driver ships as loadable modules in `/vendor/lib/modules`
  (`wmt_drv`, `wmt_chrdev_wifi`, `wlan_drv_gen4m`), all with vermagic matching
  the running 4.19.127 kernel.  `phy0` is *not* the MediaTek radio.

### Solved
- **No Android property service.**  `wmt_loader` / `wmt_launcher` reach it via
  libcutils' unversioned `property_get` / `property_set`, so a 2944-byte
  freestanding aarch64 `LD_PRELOAD` shim (`tools/propshim/`, built to
  `data/wifi/libwpdprops.so`) satisfies them.  Store is a flat
  `key=value` file at `/tmp/wpd-props`; `property_get` takes the *last* match
  so a rewrite is just an append.  Confirmed working — the loader now
  publishes `persist.vendor.connsys.chipid=0x6765`.
- **Wrong wmt_launcher arguments.**  The guessed `-m 1 -p /vendor/firmware`
  made the launcher open `/dev/stpwmt` and immediately exit.  The real service
  line, from `/vendor/etc/init/init_connectivity.rc`:

      service wmt_launcher /vendor/bin/wmt_launcher -p /vendor/firmware/ -o 1

  There is no `-m`; the mode flag is `-o 1`; and the **trailing slash on the
  path matters** because the launcher concatenates path + filename.  With
  these the launcher stays up and serves patches.
- **SD pushed into the ramdisk.**  `wifi_now.py` mounted `/mnt/sd` the instant
  the console answered, but `/dev/mmcblk1p1` enumerates seconds later, so the
  mount failed silently and every pushed file landed in the ramdisk, only to
  be shadowed when the stack script later mounted the real card.  It now waits
  for the block device and aborts rather than pushing into nowhere.
- **Console node index is not stable.**  Each re-enumeration shifts the pair
  (`ttyACM0` -> `ttyACM1` -> `ttyACM2`), and only one of the two is a shell.
  `connect()` now probes every `/dev/ttyACM*` instead of hardcoding one.

### Reached this boot
CONNSYS initialises completely.  From dmesg:

    mtk_wcn_soc_ver_check: 0x6765: SOC_CONSYS.E1 (0x8a00/0x8a00, patch_ext:_e1)
    mtk_wcn_soc_patch_dwn: [Patch]BuiltTime=220121120501000, HVer=0x8a00 ...
    mtk_wcn_soc_pda_patch_dwn: patch dwn:0 frag(27, 416) ok
    mtk_wcn_soc_calibration_restore: ... OK.
    opfunc_sw_rst: WMT-CORE: SW Rst succeed

### Remaining blocker
The WiFi *firmware* does not start, so `echo 1 > /dev/wmtWifi` returns EIO:

    halRxWaitResponse: (HAL ERROR) CONN_HIF_ON_DBGCR01[0xf000b120]
    nicRxWaitResponse: (RX ERROR) halRxWaitResponse fail! status C0000001
    wlanConfigWifiFunc: (INIT INFO) FW_START EVT failed
    glResetTrigger: Chip[6632 E1] FW Ver DEC[0.0] HEX[0.0], Driver Ver[0.0]
    wlanAdapterStart: Fail reason: 5
    wlanProbe: probe failed, reason:3
    wmt_func_wifi_on(799): wmt call wlan probe fail(-1)

Note the version mismatch to chase next: WMT identifies the chip as **0x6765**
while `wlan_drv_gen4m` reports **Chip[6632 E1]** with FW/Driver version 0.0,
and `/vendor/firmware` carries both `soc1_0_*` and `soc3_0_*` image sets plus
`WIFI_RAM_CODE_soc1_0_1_1.bin`.  Suspect the driver is selecting the wrong
firmware image set, and/or `wlan_drv_gen4m` needs module parameters that
`init.wlan_drv.rc` supplies.  Read `/vendor/etc/init/init.wlan_drv.rc` next.

### Do not do
- `rmmod wmt_drv` deadlocks the kernel and takes the serial console with it.
- Powering on WiFi before `wmt_launcher` is serving wedges the driver's single
  command buffer for the rest of the boot (`cmd buf is occupied by
  (srh_rom_patch)`); only a power-cycle recovers.

### State at end of session
Both ACM endpoints wedged after the failed firmware start — the gadget still
enumerates as 0e8d:2008 but neither port responds.  Needs a power-cycle.

## 2026-08-20 (later) — root cause of the firmware start failure: module load order

Instrumented `mtk_stack_up.sh` to clear the kernel log immediately before the
*first* `echo 1 > /dev/wmtWifi` and dump the trace on failure.  (Only the first
attempt is informative: once one fails, the chip latches into "chip reset and
chip no response" and every later trace is post-mortem noise.)

The captured first-attempt trace shows the firmware download itself is fine:

    wlanSetChipEcoInfo: Chip ID[0001] Version[E1] HW[0x00008a00] SW[0x00008a00]
    kalFirmwareOpen: Request FW image: WIFI_RAM_CODE_soc1_0_1_1.bin done
    wlanGetConnacTailerInfo: N9: chip_info[7:E1] region_num[5] version[tk__000200]
    fwDlGetReleaseManifest: t-neptune-mp-soc1_0e1-2015-CERVINO_SOC1_0_E1_ASIC-...
    ...
    wlanDownloadEMISection: (INIT ERROR) Consys emi memory address
                            gConEmiPhyBaseFinal invalid
    wlanConfigWifiFunc: FW_START CMD send, waiting for RSP     <- then times out

So the right image is found and loaded; what fails is the EMI section write,
because `wlan_drv_gen4m` holds an unset CONNSYS EMI physical base.  The firmware
therefore starts with a hole in it and never answers FW_START.

**Cause: our module load order was wrong.**  `/vendor/etc/init/init.wlan_drv.rc`:

    on property:vendor.connsys.driver.ready=yes
      insmod /vendor/lib/modules/wmt_chrdev_wifi.ko
      insmod /vendor/lib/modules/wlan_drv_${ro.vendor.wlan.gen}.ko

Android deliberately withholds the two wifi modules until `wmt_loader` has
published `driver.ready=yes` -- that is the point at which `wmt_drv` has stored
the EMI base.  `mtk_stack_up.sh` was insmod'ing all three back to back *before*
running `wmt_loader`, so `wlan_drv_gen4m` latched the base before it existed.

Corrected staging in `mtk_stack_up.sh`:

    2.  insmod wmt_drv
    3.  wmt_loader           (chip detect; publishes chipid + driver.ready)
    3a. insmod wmt_chrdev_wifi, wlan_drv_gen4m
    4.  wmt_launcher         (must serve before any /dev/wmtWifi write)
    5.  echo 1 > /dev/wmtWifi

Ruled out along the way:
- Firmware search path is fine: `/sys/module/firmware_class/parameters/path` is
  already `/vendor/firmware`, and the stock firmware already symlinks
  `/lib/firmware -> /vendor/firmware`.
- The firmware image is the right one for this chip (soc1_0, E1, HW 0x8a00).

Still unverified (needs a fresh boot): whether the corrected order makes
`gConEmiPhyBaseFinal` valid and lets FW_START succeed.

Open secondary concerns, only if reordering is not enough:
- `glLoadNvram: Nvram not available` -- MTK wifi NVRAM (MAC + per-device
  calibration) lives on a partition we do not mount.
- `wlanCfgInit: Init wifi config len 0` and the earlier
  `wmt_conf_parse: failed to parse 'wifi_config'`.
- `opfunc_wlan_probe: not implemented yet hifType: 0x2, unspecified wifi_hif`
  (appears benign -- probe continues past it).

## 2026-08-20 (evening) — wlan0 EXISTS.  CONNSYS + firmware fully up.

Two separate bugs stood between us and the interface; both are fixed in
`scripts/mtk_stack_up.sh`.

### Bug 1 — module load order (the EMI base)
Fixed as described in the previous entry.  Confirmation that it worked, from a
clean run:

    wlanDownloadEMISection: EmiPhyBase:0xbe000000 offset:0x177000,
                            ioremap region 0x400000 @ 0xFFFFFF8019C00000

Compare with the old failure, `gConEmiPhyBaseFinal invalid`.  The EMI section
now downloads and FW_START gets its response.

### Bug 2 — /dev/wmtWifi was a regular file
There is no `ueventd` on this firmware, so nothing turns the driver's class
uevent into a device node.  `mtk_stack_up.sh` then did `echo 1 > /dev/wmtWifi`,
which **created it as an ordinary file**.  Writes succeeded, `cat` read back
whatever had been written, and the driver never saw anything -- no dmesg output
at all, which is what made this so confusing to chase.

The real device is dynamic-major, published in sysfs:

    /sys/class/wmtWifi/wmtWifi/dev  ->  498:0
    /proc/devices                   ->  498 mtk_wmt_wifi_chrdev

The script now builds the node itself before any power-on:

    DEVNUM=$(cat /sys/class/wmtWifi/wmtWifi/dev)
    mknod /dev/wmtWifi c "${DEVNUM%%:*}" "${DEVNUM##*:}"

### Result
    [MTK-WIFI] WIFI_write[I]: WMT turn on WIFI success!
    wlanProbe: probe success, feature set: 0x84881418, persistNetdev: 1
    32: wlan0: <NO-CARRIER,BROADCAST,MULTICAST,UP> mtu 1500
        link/ether 00:08:22:18:2a:06

### Debugging note for future work
The kernel log is flooded with per-byte `USB_ACM pr_vmpt_debug` lines, so the
ring buffer evicts WMT/wlan messages within seconds and `dmesg` after the fact
shows nothing.  Either do the whole cycle in ONE shell command ending in a
`dmesg > file`, or stream it:  `(cat /proc/kmsg > /tmp/w.log &)`.
Also: this busybox has no `pkill` and no `killall`; kill by pid from `ps`.

### Next blocker — no libnl for the supplicant
`/vendor/bin/hw/wpa_supplicant` exists but will not link:

    CANNOT LINK EXECUTABLE "/vendor/bin/hw/wpa_supplicant":
    library "libnl.so" not found: needed by main executable

`libnl*.so*` is present nowhere on the mounted images (only `system.img` and
`vendor.img` are staged; a stock build would also have product/system_ext).
The prebuilt musl bundle `data/wifi-userspace/wifi-userspace-aarch64.tar.gz`
(4.5 MB) carries its own `libnl-3.so.200` plus wpa_supplicant/wpa_cli/iw, so it
sidesteps the Android libraries entirely.

Pushing 4.5 MB over the serial console at ~10 KB/s is painful, and the tablet
already runs USB networking (`usb0 192.168.42.1`, `rndis0 192.168.43.1`, both
links UP on the host as cdc_ether/rndis_host).  Bringing up the host side of
that link is the fast route for the transfer.

---

## 2026-08-20 — WiFi works end to end: `ssh root@192.168.0.100` over the built-in radio

The goal is met. The tablet associates with the configured SSID, takes a DHCP lease, reaches
the internet, and serves SSH — no USB tether anywhere in the path.

```
[wifi] using /mnt/sd/wifi/sbin/wpa_supplicant
[wifi] associated with <ssid>
udhcpc: lease of 192.168.0.100 obtained from 192.168.0.1, lease time 7200
[wifi] === wlan0 has 192.168.0.100 ===
default via 192.168.0.1 dev wlan0
2 packets transmitted, 2 packets received, 0% packet loss
round-trip min/avg/max = 11.578/43.960/76.343 ms
```

and from the host, with the serial console dead at the time:

```
$ ssh root@192.168.0.100 'uname -a'
Linux (none) 4.19.127 #1 SMP PREEMPT Fri May 6 19:07:29 CST 2022 aarch64 GNU/Linux
```

### The libnl blocker, resolved

`/vendor/bin/hw/wpa_supplicant` is unusable: it needs `libnl.so`, which lives in
the product / system_ext images this firmware does not carry. Rather than hunt
for it, the Alpine aarch64 bundle in `data/wifi-userspace/` brings its own
`libnl-3` and runs under its own musl loader. `wpa_supplicant v2.11` starts and
associates on the first try. The bundle was trimmed to
`wifi-userspace-min.tar.gz` (4.26 MB): stripped, with `iw`, `wpa_passphrase`,
`libnl-route-3` and `libpcsclite_real` dropped after `readelf -d` confirmed the
supplicant lists none of them as NEEDED. That matters because every byte crosses
the console as base64.

### Three bugs worth remembering

**1. Sentinels were satisfied by the console's own echo.** `stty -echo` does not
reliably stick on this gadget. When it does not, `echo SDOK` is matched by the
echoed *command*, not by any output — so a run reported "staged scripts,
wifi.conf and the property shim" against a tablet where `/mnt/sd` was not even
mounted. Every framed command had this hole. Markers are now emitted as
`printf '%s%s\n' '__WPD' '_EOC_1__'`: the echo shows two separate arguments and
only real output ever contains the joined token. This had already been fixed
once in `wifi_loop.py` and not generalised; it is now in `serial_probe.py`,
where every caller inherits it.

**2. A `> ` in the transcript is a quote-continuation prompt.** `put()` fired
2 KB chunks back to back with no flow control, overran the tty input buffer, and
truncated one mid-string. The unbalanced quote parked the shell at `> `, where
everything sent afterwards was swallowed as the tail of a quoted string.
`put()` now round-trips per chunk, and `Console.open()` sends Ctrl-C first to
clear any half-typed line a previous run left behind.

**3. `python … | tee log` reports tee's exit status.** A run that raised an
exception was recorded as exit 0. Do not pipe a script whose exit code matters.

### Two device-side gotchas

**The SD card is vfat, so it cannot hold a symlink.** `tar` extracts the whole
bundle and then fails on `lib/libc.musl-aarch64.so.1 -> ld-musl-aarch64.so.1`
with "Operation not permitted". The unpack step now ignores that one error and
makes the link a real copy. `bundle.tar.gz` is deliberately left on the card:
it is the expensive artefact, and the card survives reboots, so a re-run never
re-transfers it.

**`/root` ships owned by uid 1000.** The ramdisk was built by a non-root user,
so `/root`, `/root/.ssh` and `authorized_keys` all belong to 1000:1000 and
`/root` is group-writable. Dropbear silently refuses a key file the logging-in
user does not own, and the client sees only
`Permission denied (publickey,password)` — nothing in between says why.
`chown -R 0:0 /root && chmod 700 /root /root/.ssh` fixes it; `wifi_assoc.sh`
now does this before starting the daemon.

### Once wlan0 is up, stop using the serial console

The CDC-ACM console wedged again immediately after this (writes time out, the
second endpoint is silent) — and it did not matter, because the tablet stayed
reachable over WiFi the whole time. `scripts/tnet.py` drives the firmware's
unauthenticated root telnetd over the network: it does not wedge, does not
re-enumerate, and is orders of magnitude faster. Prefer it over serial for
everything after association.

### Open

- **Persistence.** Everything above lives in a volatile ramdisk. A reboot loses
  the whole stack and `mtk_stack_up.sh` has to run again. This is plan phase 5
  and is the only substantial thing left.
- **Exposure.** The firmware runs telnetd on port 23 with a root shell and no
  password. On the USB tether that was a private link; on WiFi it is offered to
  the whole LAN. Worth closing once persistence lands.
- **NVRAM.** `glLoadNvram: Nvram not available` — the MAC/calibration partition
  is not mounted, so the MAC is the driver's fallback `00:08:22:18:2a:06`. It
  associates fine, but TX calibration is whatever the firmware defaults to.


## 2026-11-06 - direction change and boot check

- IchiBazar is no longer hosted on the tablet. Caddy, cloudflared, Redis and the
  ecommerce services are set to `AUTOSTART=0` in `config/services/`. Not yet
  redeployed to the tablet.
- New goal: use the tablet as a standalone device (touch, buttons, display).
- Boot check: the tablet came up on WalPad-Server with `usb0`=192.168.42.1 and
  `rndis0`=192.168.43.1, but no `/dev/mmcblk1` (no SD card detected), so
  `wifi_boot.sh` could not start Wi-Fi. Host pings over USB got no ARP reply;
  the serial console on `/dev/ttyACM0` did work. Action: reseat the SD card.

## 2026-11-07 / 08 — attempt to restore Android, root causes found, switch to a Linux touch UI

Goal changed on 2026-11-06: stop hosting IchiBazar and use the tablet as a normal device
(touch, buttons, display, Wi-Fi).  Android restore was tried first; it was dropped on
2026-11-08 in favour of an on-device Linux touch UI (see `docs/20-real-device-plan.md`).

### How the tablet is reached now (all verified)
- **mtkclient in preloader mode needs no preloader file** (BROM mode failed at DRAM setup).
  Start ONE `mtk.py` command, then unplug/replug the USB cable with no buttons held (a 15 s
  power hold also works while the tablet is running).  The window stays open for several
  consecutive commands.  Never run two mtkclient processes at once; a killed session needs a
  new unplug/replug.  Use `mtk.py reset` to leave the window.
- **Fastboot works** (`0e8d:201c`, bootloader **unlocked**).  There is no `fastboot` binary here;
  `scripts/fastboot_py.py` is a minimal pyusb client (getvar, reboot).  `fetch` is not supported.
- **Serial console wedges** after any large transfer; the host side of the USB ethernet never
  transmits (ARP stays INCOMPLETE).  Prefer mtkclient reads for anything big.

### Root causes found
- `super` held an **Android sparse image written raw** (magic `3aff26ed` at offset 0), so Android
  could never read it.  It contains genuine Walton Android 11 system/vendor/product images
  (`WALTON/WALPAD8G/WALPAD8G_V2:11/RP1A.200720.011/mp1V915`, no system_ext).  Expanded and
  rewritten 2026-11-07 and verified byte-for-byte (`build/android-restore/super-raw.img`; the
  as-found sparse dump is `super-sparse-as-found.img`).
- `firmware/MT6765/backups/boot_a-original.img` is **not stock Android**: its ramdisk is the
  BusyBox + dropbear WalPad-Server ramdisk (and `boot_b` holds the same image).  No stock
  `boot.img` exists anywhere; `vendor_boot_a` is all zeros; `vbmeta_a/system/vendor` were edited
  in place (flags 0x3, hashes no longer match).
- `userdata` (p44) and `metadata` (p8) were erased on 2026-11-07 with the user's approval.  The
  Alpine VPS that used to live on `userdata` is gone; the SD card is currently not detected
  (`mmc1` shows no card), so the old Wi-Fi bundle and keys on it are unreachable.

### Android experiment (kept in `config/boot/init-android-experiment`)
A replacement first-stage `/init` (BusyBox script + `tools/dmlinear`) mounted system/vendor/product
from `super` through device-mapper and `switch_root`ed into Android's `init selinux_setup`.
Progress, each step proven from kernel logs read out of `expdb`/`metadata`:
1. selinuxfs not mounted -> fixed.  2. loop devices are blocked by SELinux (kernel domain cannot
read the `tmpfs`-labelled node) -> replaced by dm-linear.  3. `/dev/null` was created as a regular
file by a `2>/dev/null` redirect -> fixed.  4. `mdev -s` pre-created `/dev` nodes that ueventd
could not relabel -> minimal `/dev`.  After that Android reached second-stage init, ueventd and
apexd, and the tablet enumerated as `0e8d:20ff WALTON WALPAD8G` (Android USB), but the screen
stayed black and nothing more could be diagnosed without more log cycles.  Stopped there.
A real fix is the official Walton firmware (stock boot.img + vendor_boot.img + vbmeta + dtbo).

### Current state of the tablet
- `boot_a` = server ramdisk + **padui** (tools/padui): framebuffer UI with a home screen, touch
  pad, button test, status and brightness.  Power key turns the screen off/on, volume keys change
  brightness.  Built with `scripts/build_ui_boot.sh`.
- Verified working: display, hardware buttons.  **Not working: touch** (cause unknown; the build
  flashed last adds an on-screen event counter per input device to find out).  **Battery shows -1%**:
  the kernel gauge reports `uisoc:-1`; padui now estimates from voltage and labels it "est.".
- `boot_b` still holds the unmodified server image (fallback); `super` holds repaired Android.
- Input map: event0 HALL_DEV, event1 mtk-kpd (volume), event2 mtk-pmic-keys (power),
  event3 kxd-kxd, event4 mtk-tpd (touch).

### Tools added
`scripts/repack_boot.py` (replace a v2 boot image's ramdisk), `scripts/build_ui_boot.sh`,
`scripts/fastboot_py.py`, `tools/padui/` and `tools/dmlinear/` (static aarch64 C built in a
throwaway Alpine container with clang+lld; no cross toolchain needed on the host).

### Gotchas that cost time
- `pkill -f`/`pgrep -f` patterns can match your own shell command line and kill it; use
  `ps | grep | grep -v grep`.
- `cp` is aliased interactive here: use `command cp -f`.
- `/tmp` is a 6.8 GB tmpfs; a 4 GB partition read plus a copy fills it.
- `cmd 2>/dev/null` evaluates the redirect first: it creates `/dev/null` as a regular file.
- A write that "succeeds" can still not land if the previous mtk session was stale: always read
  back and compare hashes.
