# ZT9101 USB Wi-Fi dongle driver for WalPad (station mode, monitor mode, injection)

## Status: done and verified on-device (2026-11-12)

Station mode, monitor mode and packet injection all confirmed working on WalPad itself (not just
host/VM). See **Verification** below for the actual device-side results.

## Why

Owner request 2026-11-12: use the ZT9101-based USB Wi-Fi dongle (Speedlink 300 Mbps,
`2310:9086`/`2310:1a2b` pre-switch, identified on the host PC: `memory/usb-dongle-identify.md`)
on WalPad itself over the OTG cable (USB host via musb, confirmed working: `memory/hardware.md`).

Scope changed mid-change: this started as station-mode-only (monitor/injection staying
host/VM-only, mirroring the onboard Wi-Fi monitor-mode decision in `memory/known-issues.md`).
The owner explicitly asked to extend it to monitor mode + injection on WalPad too once station
mode was proven; that reversal is recorded here, not silently applied.

The ZT9101 chip has no in-kernel Linux driver; it needs the out-of-tree `dobbsbp/ztop-wifi-driver`
source built as a `.ko` for WalPad's exact running kernel (4.19.127, `CONFIG_MODVERSIONS=y`), the
same way `scripts/build_bt_module.sh` and `scripts/build_wifi_module.sh` already build
out-of-tree/reconfigured modules for this device.

## What changed

- `scripts/build_zt9101_module.sh` (new): builds `zt9101_ztopmac_usb.ko` for WalPad's kernel,
  reusing `build_bt_module.sh`'s header/autoconf.h/asm-offsets overlay and CRC-cross-check recipe
  (`scripts/bt/versions.py`, `scripts/bt/merge_symvers.py`) against an out-of-tree module source
  tree instead of an in-tree one. Real build bugs found and fixed along the way:
  1. missing `LLVM=1` passthrough to the module's own inner kbuild invocation (tried a nonexistent
     GCC cross-compiler otherwise).
  2. bypassing the driver's `platform.mak` `HOST_PLAT` branches (needed since none of its listed
     platforms match this device) also dropped `-DCONFIG_LITTLE_ENDIAN`, which `nic/tx.c`'s
     TKIP/AES IV macros need (`union pn48._byte_` only exists under that define); passed back in
     via `USER_EXTRA_CFLAGS`.
  3. `modpost` reads `Module.symvers` from the kernel build tree root, not from the external
     module's own directory -- writing it to the module dir (first attempt) silently produced a
     CRC-less module.
- `scripts/bt/merge_symvers.py`: added an optional 6th arg (a short, explicit list of symbols
  where a root-caused vendor/computed CRC mismatch is expected and should not hard-fail the
  build; vendor's CRC -- already what gets written -- is simply trusted without stopping).
  Used for exactly `wake_up_process`, `remap_pfn_range`, `set_user_nice`: all three exported from
  `kernel/sched/core.c` / `mm/memory.c`, MTK-vendor-patched files that `tools/kernel-mt6765-4.19`
  (a 4.19.191 tree, not Walton's real 4.19.127 source) does not match -- a header overlay cannot
  fix a `.c`-file difference. `scripts/build_bt_module.sh` is unaffected (no 6th arg passed there).
- Monitor mode + injection ported from the owner's already-verified host-PC patch
  (`/home/md/Code/zt9101-wifi/patches/radianceadm-zt9101-monitor-and-injection.patch`, written
  against `RadianceADM/zt9101-linux`) onto `dobbsbp`'s own source at
  `/home/md/Code/zt9101-wifi/dobbsbp/driver/` (outside this repo; same chip/vendor lineage, same
  bugs, same fix shape, re-applied by hand against dobbsbp's actual symbols rather than
  `git apply`d blind -- it did not apply cleanly, each hunk's target function/macro/struct field
  was independently confirmed present under its real name before editing):
  - `mak/linux/Makefile`: `-DCONFIG_WIFI_MONITOR` alongside the existing
    `-DCFG_ENABLE_MONITOR_MODE` (two different macros gate this: one for `interface_modes`, one
    for the rest of the monitor code paths).
  - `nic/nic.h`: `mon_channel` field on `nic_info_st`.
  - `nic/rx.c`: radiotap header no longer claims an FCS the chip doesn't append; channel reports
    the tuned monitor channel instead of the connected network's; signal is the real
    `phy_status.signal_strength - 100` dBm instead of always 0.
  - `os/linux/cfg80211/zt_cfg80211.c`: real RX filter open (`UMSG_OPS_MP_SET_PRX`, same MCU
    command and RCR bit values as `os/linux/proc_trx.c`'s existing `mp_proc_set_rx_filter` --
    cross-checked identical, not guessed) and TX queue re-enable (the vendor driver ties kernel
    netif TX queue state to "associated to an AP" and never re-enables it for monitor mode,
    which never associates) on entering monitor mode; `_set_monitor_channel` was an empty stub,
    now retunes the radio via `zt_hw_info_set_channel_bw` (already used elsewhere in this file for
    the equivalent station-mode path) and records `mon_channel`.
  - `os/linux/netdev/ndev_linux.c`: `work_monitor_tx_entry` was a commented-out stub; implemented
    to strip the radiotap header userspace injection tools prepend and hand the raw 802.11 frame
    to `zt_nic_mgmt_frame_xmit` (the driver's real management-frame TX path; `cfg80211`'s
    `Mgmt_Tx` is a no-op outside P2P).
  - The kernel `>=6.13`-gated `.get_channel` op from the host patch was **not** ported: it's
    irrelevant to this 4.19 build (the `#if` around it never compiles on this kernel either way).
- `build/zt9101/wifi.cfg` (new, local-only, not committed -- gitignored under `build/`):
  `ifname=wlan1`/`if2name=p2p1` (never `wlan0`/`p2p0`, which the onboard CONNSYS chip already owns
  and which was live and associated throughout this whole change), firmware paths pointing at
  `/root/zt9101/fw/*.bin` on the device.

## Out of scope (unchanged)

- Loading `zt9101_ztopmac_usb.ko` persistently at boot, udev rules for the mode-switch, any
  `walpad-wifi`/CONNSYS integration (frozen per `CLAUDE.md`) -- every test this change describes
  was a manual, SSH-driven, non-persistent `insmod`/`wpa_supplicant`/`iw` session.
- A from-scratch self-test harness: the injection verification used a real client already on the
  owner's own 2.4GHz "Loff" AP (WalPad's own `wlan0` is on 5GHz, unreachable by this 2.4GHz-only
  chip, so it could not be the deauth target itself) -- see **Verification**.

## Device risk

No partition writes, no flashing anywhere in this change. `insmod`/`rmmod` and `ip`/`iw` state
changes only; a CRC/vermagic mismatch simply refuses to load (never hit in practice here -- every
build passed verification before being tried on-device). One real operational incident during
this change, **not caused by the driver build itself**: running the dongle in station mode
(its own DHCP lease, `192.168.0.107`) at the same time as the onboard Wi-Fi's own station-mode
connection broke the tablet's SSH reachability for several minutes (`Destination Host
Unreachable` from the LAN gateway) -- the tablet itself stayed fully up throughout (uptime never
reset, screen stayed responsive, confirmed after recovery) and it was a routing-state issue, not
a crash; it resolved on its own (a later DHCP lease on `wlan0` landed on `192.168.0.108`). Monitor
mode does not carry this specific risk (no DHCP), but running two active Wi-Fi radios on this
tablet at once is now a known soft spot worth avoiding when both are in station mode.

Separately: the dongle itself showed real USB enumeration instability directly on WalPad's bare
OTG port (repeated connect/reset/disconnect inside ~200ms, several times, before ever reaching a
driver) -- looked like a power-delivery limit, not a software bug; it did eventually enumerate
and stay stable on retries without needing a powered hub in the end, but that remains the
likely explanation if it recurs.

## Verification (actual results, 2026-11-12)

- Build: `build/zt9101/zt9101_ztopmac_usb.ko` vermagic `4.19.127 SMP preempt mod_unload
  modversions aarch64`, matching `wmt_drv.ko` exactly; every CRC in `__versions` traced to the
  `Module.symvers` used (vendor-authoritative for the 3 expected-mismatch symbols, cross-checked
  agreement for the rest); zero unresolved symbols, zero modpost warnings. Built twice (station-
  mode-only, then with monitor/injection added) -- both clean.
- Mode-switch: `usb_modeswitch` + the host PC's own known-good `/etc/usb_modeswitch.d/2310:1a2b`
  config (`StandardEject=1`) installed on WalPad via `apk`; reliably switches `2310:1a2b` ->
  `2310:9086` once `usb-storage` is unbound first (binds automatically on enumeration; unbinding
  before the switch avoids the known "SCSI/eject hangs once usb-storage holds the fake CD" trap).
- Station mode: `wlan1` associates WPA2-PSK to a real AP (`wpa_state=COMPLETED`), gets a real
  DHCP lease, pings out. Firmware loads from `/root/zt9101/fw/*.bin` via the driver's own
  `cfg=` config file mechanism (not `request_firmware`/`/proc/2/root` -- that open question from
  the original proposal did not apply; this driver reads firmware itself, off a plain file path
  set by a module parameter).
- Monitor mode: `iw dev wlan1 set type monitor` + `set channel 9` succeeds cleanly; `tcpdump -i
  wlan1` shows real beacons, RTS/CTS, QoS data and BA frames from the owner's own AP, with
  correct channel/frequency, non-zero/non-garbage dBm signal values, and no spurious FCS bytes --
  i.e. all three `rx.c` radiotap fixes are independently confirmed correct by a real decoder, not
  just "compiles".
- Injection: `aircrack-ng` installed inside the already-provisioned `walpad-kali` chroot
  (`openspec/changes/kali-chroot-mode/`, shares the host kernel so it sees `wlan1` directly --
  deliberately not `apk add`ed into the base Alpine rootfs). `aireplay-ng --ignore-negative-one
  --deauth 2 -a <AP> -c <a real client already on the owner's own "Loff" AP> wlan1` reported
  **64/64 ACKs received** (over-the-air confirmation); a concurrent `tcpdump -i wlan1` on the
  same interface independently captured **258 real DeAuthentication frames**, including the AP's
  own "4-Way Handshake timeout" response -- i.e. the injected frames had a real, observable effect
  on real hardware, not just a clean return code.

## Open questions

None outstanding. The two open questions in the original proposal (which upstream source tree,
and how firmware loading works on this device) are both answered above.
