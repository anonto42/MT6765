# Bluetooth with BlueZ (2026-11-10)

Status: HCI level verified on the tablet (hci0 UP RUNNING, `bluetoothctl power on` / `scan on` start). Boot start, core module and UI built and host-tested (2026-11-10); no remote device was found in a scan yet (phone discoverable? RF init?).

## How it fits together

```
BlueZ (bluetoothd, bluetoothctl)  <-mgmt/HCI socket->  hci0
hci0  <- hci_vhci.ko (/dev/vhci, one packet per read/write; our build for 4.19.127)
      <- walpad-btbridge (re-frames the H:4 stream, logs counters only)
      <- /dev/stpbt (vendor bt_drv.ko, char major 192; open = BT power on, close = off)
      <- wmt_drv.ko / STP / CONNSYS firmware (already up for Wi-Fi via walpad-wifi)
```

`walpad-bluetooth start|stop|restart|status` (config/rootfs/usr/local/sbin) runs the chain.  `walpad-init`
starts it at boot after `walpad-wifi` (background subshell, log `/var/log/walpad-bluetooth.log`) unless
`/var/lib/walpad/bluetooth-enabled` is `0`.  `start` is idempotent.  walpad-core drives BlueZ through
`bluetoothctl` and exposes it in the UI: see `docs/rust/CONTRACTS.md` (Bluetooth) and `docs/rust/UI.md`.

## /dev/stpbt protocol (from `drivers/misc/mediatek/connectivity/bt/mt66xx/wmt/stp_chrdev_bt.c`, matching the strings of the shipped `/vendor/lib/modules/bt_drv.ko`)

- Plain H:4 both ways, with the packet-type byte.  Evidence: the driver's own synthetic
  Hardware Error is `{0x04, 0x10, 0x01, 0x00}`, and its debug path sends commands as
  `hci_cmd[0]=0x01, opcode = hci_cmd[1] | hci_cmd[2] << 8`.
- `open()` calls `mtk_wcn_wmt_func_on(WMTDRV_TYPE_BT)` (no ioctl or write needed to power on) and
  then `mtk_wcn_stp_set_bluez(0)` ("MTK Bluetooth Mode").  A second opener gets `-EIO`.
  `close()` powers BT off.
- `read()` returns whatever is in the STP rx ring (up to 2048 bytes): packets can be split or
  merged, so the bridge re-frames.  Blocking reads use an *uninterruptible* wait, so the bridge
  uses `O_NONBLOCK` + `poll()`.  `poll()` always reports writable; `write()` returns `-EAGAIN` when
  the STP queue is full (bridge backs off 5 ms, 1 s limit).  Max write 2048 bytes.
- The node is created by the driver (`CREATE_NODE_DYNAMIC=1`: class `stpbt`, device `stpbt`);
  without udev the script mknods it from `/sys/class/stpbt/stpbt/dev` (fallback 192:0).
- After a whole-chip reset the driver returns one Hardware Error event and then blocks; BlueZ
  resets the controller.  The bridge forwards it and counts it.

Not known / not done (Android's `libbluetooth_mtk` may do these after open):
- BD address from NVRAM (`/mnt/vendor/nvdata/APCFG/APRDEB/BT_Addr`).  The bridge can send the
  MediaTek vendor command `0xFC1A` with `BDADDR=` from `/etc/walpad/bluetooth.conf` (untested).
- TX power / radio settings vendor commands.  BlueZ only sends standard HCI; defaults may be fine.
- The 4.19.191 Halium tree for MT6765 patches `hci_core.c` to skip `HCI_OP_READ_SYNC_TRAIN_PARAMS`
  because "the BT chip returns error ... which causes the initialization to fail".  The running
  Walton kernel lacks that patch, so the bridge rewrites a failed reply to that one command into a
  success with neutral values (`patched_replies` in the stats).

## The module build (`scripts/build_bt_module.sh`)

Our kernel tree is a different vendor tree at 4.19.191; the running kernel is Walton's 4.19.127.
The vendor partition's `kheaders.ko` carries the running kernel's exact headers (incl. generated
`autoconf.h`, `asm-offsets.h`, `vdso-offsets.h`), so the module is compiled against those, with
`SUBLEVEL=127` and the running config.  Symbol CRCs: taken from the vendor modules' `__versions`
where present, otherwise computed by genksyms against the 4.19.127 headers.  Evidence (build/bt/crc-report.txt):

- vermagic `4.19.127 SMP preempt mod_unload modversions aarch64` == `wmt_drv.ko`'s.
- hci_vhci needs 46 symbols: 33 have both a vendor and a computed CRC and all 33 agree; 2 are
  vendor-only (`module_layout`, `__ll_sc_atomic64_or`); 11 are computed-only.
- Wider sample: 49/51 other symbols agree.  The 2 that differ (`set_user_nice`,
  `wake_up_process`) come from `kernel/sched/core.c`, which includes the private
  `kernel/sched/sched.h` that kheaders does not carry (4.19.191's is used).
- Computed-only: `hci_{alloc,free,register,unregister}_dev`, `hci_recv_frame`, `bt_err` (net/bluetooth,
  whose private headers `hci_request.h hci_debugfs.h smp.h leds.h` are byte-identical to upstream
  v4.19.127) and `skb_pull`, `skb_queue_purge`, `no_llseek`, `param_ops_bool`,
  `_copy_from_iter_full` (from skbuff.c, read_write.c, params.c, iov_iter.c, whose other exports
  agree with the vendor CRCs).  Expected right, but only `insmod` on the device proves it.
- If any CRC is still wrong, `insmod` refuses with `disagrees about version of symbol X` (no crash).

## Device test plan

Run in this order over SSH; stop at the first unexpected result and send the output.

0. Host: build and copy (the current flashed image has none of this):
   ```
   scripts/build_bt_module.sh          # -> build/bt/hci_vhci.ko
   scripts/build_rust.sh               # -> build/rust/aarch64/walpad-btbridge
   T=root@<tablet-ip>
   ssh $T mkdir -p /usr/local/lib/modules
   scp build/bt/hci_vhci.ko $T:/usr/local/lib/modules/
   scp build/rust/aarch64/walpad-btbridge $T:/usr/local/bin/
   scp config/rootfs/usr/local/sbin/walpad-bluetooth $T:/usr/local/sbin/
   ```
1. Preconditions (Wi-Fi must be up: it owns the CONNSYS stack):
   ```
   uname -r                                   # 4.19.127
   cat /run/walpad/wifi-state; grep -E '^(wmt|bt_drv|hci_vhci)' /proc/modules; ls -l /dev/stpwmt
   ```
2. Packages (Wi-Fi needed):
   ```
   apk add bluez bluez-deprecated dbus
   grep -q ^messagebus: /etc/passwd || { echo 'messagebus:x:101:messagebus' >> /etc/group; echo 'messagebus:x:100:101:messagebus:/run/dbus:/sbin/nologin' >> /etc/passwd; }
   ```
3. Vendor BT driver alone:
   ```
   insmod /vendor/lib/modules/bt_drv.ko; dmesg | grep MTK-BT | tail
   ```
   Expect `mtk_stp_bt_chrdev driver(major 192) installed`; `cat /sys/class/stpbt/stpbt/dev` -> `192:0`.
4. Our module alone (the CRC/vermagic test; safe: a mismatch is refused):
   ```
   insmod /usr/local/lib/modules/hci_vhci.ko; echo rc=$?; dmesg | tail -5; ls -l /dev/vhci /sys/class/misc/vhci
   ```
   Expect rc=0 and `Bluetooth: Virtual HCI driver ver 1.5`.  On failure send the dmesg lines
   (`disagrees about version of symbol ...` / `version magic ...` / `Unknown symbol ...`).
5. Whole chain:
   ```
   walpad-bluetooth start; echo rc=$?
   tail -20 /var/log/walpad-btbridge.log
   dmesg | grep -E 'MTK-BT|Bluetooth|WALPAD-BT' | tail -20
   ```
   Expect `WMT turn on BT OK!`, `STP is ready!`, bridge `hci0 created; bridging`.
   If `open /dev/stpbt` hangs or fails, stop here (a failed WMT power-on may need a reboot).
6. Controller:
   ```
   hciconfig -a hci0          # UP RUNNING, BD Address not 00:00:00:00:00:00, Manufacturer: MediaTek
   walpad-bluetooth status    # stats: dropped_bytes=0, rejected_host_packets=0
   ```
   If `hciconfig hci0 up` fails: `cat /run/walpad/btbridge.stats` (patched_replies), and, if
   available, `btmon` output while running `hciconfig hci0 up` again.
7. BlueZ:
   ```
   bluetoothctl show
   bluetoothctl power on
   timeout 20 bluetoothctl --timeout 15 scan on    # expect "[NEW] Device xx:xx:..." lines
   ```
   Then try pairing a phone or a keyboard (`bluetoothctl pair/trust/connect <addr>`).
8. Teardown and Wi-Fi check:
   ```
   walpad-bluetooth stop; dmesg | grep MTK-BT | tail -3     # WMT turn off BT OK!
   ping -c 3 1.1.1.1                                         # Wi-Fi unaffected
   ```
   `stop` never unloads `bt_drv`/`wmt_drv` (rmmod wmt_drv deadlocks the kernel).

Rollback: `walpad-bluetooth stop`; delete the three copied files.  Nothing touches partitions or
the boot image.
