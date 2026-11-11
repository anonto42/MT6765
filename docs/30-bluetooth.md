# Bluetooth with BlueZ (2026-11-10)

Status: HCI level verified on the tablet (hci0 UP RUNNING, `bluetoothctl power on` / `scan on` start). Boot start, core module and UI built and host-tested (2026-11-10); no remote device was found in a scan yet.  2026-11-11: the vendor start-up sequence that Android sends (static analysis of `libbluetooth_mtk.so`, below) is now implemented in `walpad-btbridge` (`init.rs`) and sent before hci0 exists; **untested on the device**, see "Device experiments".

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

The vendor init sequence (BD address from NVRAM, radio, TX power offset, sleep timeout) is done by the
bridge since 2026-11-11: see "Recovered vendor init sequence".  Still open: whether the chip needs
anything beyond that to hear remote devices (experiments at the end).
- The 4.19.191 Halium tree for MT6765 patches `hci_core.c` to skip `HCI_OP_READ_SYNC_TRAIN_PARAMS`
  because "the BT chip returns error ... which causes the initialization to fail".  The running
  Walton kernel lacks that patch, so the bridge rewrites a failed reply to that one command into a
  success with neutral values (`patched_replies` in the stats).

## Recovered vendor init sequence (static analysis, 2026-11-11)

Source: the owner's own `vendor_a` (ext4 inside `build/android-restore/super-raw.img`, offset
1996488704): `/vendor/lib64/libbluetooth_mtk.so` (38 KB, stripped), `/vendor/lib64/libbt-vendor.so`
(the `BLUETOOTH_VENDOR_LIB_INTERFACE`, 11 KB), `/vendor/lib64/hw/android.hardware.bluetooth@1.0-impl-mediatek.so`
(an AOSP-style HIDL HAL), `/vendor/firmware/BT_FW.cfg`.  Disassembled with aarch64 binutils in a
throwaway non-privileged Debian container.  Addresses below are vaddrs in `libbluetooth_mtk.so`.

**Android flow.**  HAL `initialize()` -> `libbt-vendor` `mtk_bt_op`: `USERIAL_OPEN` = `init_uart`
(0x4060: `open("/dev/stpbt", O_RDWR|O_NOCTTY|O_NONBLOCK)`, 3 tries 1 s apart; no ioctl, no firmware
download), `FW_CFG` = `mtk_fw_cfg` (0x4560) -> `BT_InitDevice` (0x4d88) -> `GORM_FW_Init_Thread`
(0x6e80), which runs a script of HCI commands.  `POWER_CTRL`, `LPM_*`, `SCO_CFG`, `EPILOG` do nothing
relevant (`mtk_set_psm_control`, the `COMBO_IOCTL_BT_SET_PSM` ioctl 0x4001b001, exists but nothing
calls it).  The BT RAM patch is loaded by the kernel (wmt_drv, `soc1_0_ram_bt_1_1_hdr.bin` in
`/vendor/firmware`), not by userspace.

**Script selection.**  `mtk_fw_cfg` waits for `vendor.connsys.driver.ready` (10 x 0.5 s), reads
`persist.vendor.connsys.chipid` / `vendor.connsys.adie.chipid` and the 438-byte NVRAM record (NVM file id 1 =
`/mnt/vendor/nvdata/APCFG/APRDEB/BT_Addr`).  The thread's switch (jump table at rodata 0x35f0, index
chip id - 0x6735) sends chip 0x6765 (MT6765) to 0x701c: script type 3, default record at rodata 0x2ec1,
script pointer from GOT slot 0x8238.  The APS2-packed relocations decode to
`0x8238 = bt_init_preload_script_connac` (0x8228 consys, 0x8230 6632, 0x8240 6628, 0x8248 6630, 0x8250
after_6893).  The `connac` table (.data 0x95a8) is `[0x4ff8, 0x56a0, 0x5818, 0x5ac0, 0x5f88, 0x6420, 0x6368]`.

**Commands.**  Each is built by a small function, sent through the HAL's `xmit_cb`, then the thread
waits up to 2 s on a condvar for the reply (`process_cmd_rsp`, 0x6bc0); on timeout it sends `0xFD5B`
(firmware assert) and aborts; an error event aborts the init.

| # | function | opcode | params | source | confidence |
|---|----------|--------|--------|--------|------------|
| 1 | 0x4ff8 `Set_Local_BD_Addr` | 0xFC1A | 6: BD address reversed | record[0..6] (printed order).  If it equals a placeholder (00:00:46:00:00:01, 00:00:00:00:00:00) the library invents a random one (`GetRandomByte`, `rand()` seeded by time) and writes it back to the NVRAM file | high |
| 2 | 0x56a0 `Set_Radio` | 0xFC79 | 8: record[12..18] + record[36..38] | type 3/4 branch (HCI length 0x0b); older chips use a 6-byte branch | high |
| 3 | 0x5818 `Set_TX_Power_Offset` | 0xFC93 | 16: record[27..30] + record[38..51] | type 3 branch (length 0x13); type 4 sends 96 bytes, 6632 sends 6 | high |
| 4 | 0x5ac0 `Set_Sleep_Timeout` | 0xFC7A | 7: record[18..25] | | high |
| 5 | 0x5f88 `RESET` | 0x0C03 | none | | high |
| - | 0x6420 `Set_PIP` | 0xFCC5 | 2 | only if `persist.vendor.bt.pip.enable` = "1" (compare with 0x31 at 0x7430) | off by default |
| - | 0x6368 `Set_SSP_Debug_Mode` | 0x1804 | 1 | only if `persist.vendor.bt.sspdebug.enable` = "1" | off by default |

After the table the thread reads `/vendor/firmware/BT_FW.cfg` and sends every `VENDOR_CMD:` line
(`parse_fw_cfg_para` 0x69d8, `_parse_cmd` 0x66a8).  The shipped file has only a commented example
(`#VENDOR_CMD: ...`) and coex / LE-scan ratio keys that are all `0x00`; those keys feed only the
`Coex_Performance_Adjust` (0xFC22) entry of the 6630 script, which `connac` does not contain.

**The tablet's record** (`firmware/MT6765/backups/modem-20261110/nvdata.img`, `/APCFG/APRDEB/BT_Addr`,
440 bytes = 438-byte record + 2 trailer bytes): the address is provisioned (not a placeholder; ends
`62:30`), Voice `60 00`, Codec `23 10 00 00`, Radio `07 00 00 00 05 07`, Sleep `03 04 00 00 00 00 00`,
BtFTR `80 00`, TxPWOffset `ff ff ff`, the rest `00`/`ff`.  Everything except the address is byte-identical to
the library's built-in default record for this chip (rodata 0x2ec1).  So the wire sequence is:

```
FC1A 06  <addr reversed>
FC79 08  07 00 00 00 05 07 00 00
FC93 10  ff * 16
FC7A 07  03 04 00 00 00 00 00
0C03 00
```

Uncertain: parameter meanings are inferred from function names only (no MediaTek documentation).
`FC93` is all `ff` (probably "no offset programmed"), but the firmware might take it literally.  The
mapping chip 0x6765 -> `connac` rests on the jump-table and GOT/APS2 decodes (both scripted and
cross-checked with the 6632/6628/6630 slots): about 90 %.  The kernel side (wmt_drv) may add its own
patch download and calibration at `open()`, which is invisible statically.

**Implementation.**  `walpad-btbridge` (`crates/walpad-btbridge/src/init.rs`) sends exactly this after
`/dev/stpbt` is open and before `hciN` exists: 2 s per reply, stop at the first timeout, skip status
0x01/0x11 (unknown command / unsupported), count other failures and carry on.  Address: `--bdaddr`
(`BDADDR=` in `/etc/walpad/bluetooth.conf`) > nvdata copy (`/var/lib/walpad/bt-nvram.bin`, copied once by
`walpad-bluetooth` from a read-only, no-journal mount of the nvdata partition) > random address kept in
`/var/lib/walpad/bt-addr` (locally administered, mode 0600, generated once).  Logs carry only step
names, opcodes, counters and an address tail (`xx:xx:xx:xx:62:30`); stats keys `init_ok`, `init_skipped`,
`init_failed`.  `INIT=0` in the conf (or `--no-init`) turns the sequence off.

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

## Device experiments for "no device found" (2026-11-11, not run yet)

Safe order, over SSH, each step reversible with `walpad-bluetooth restart`; stop at the first surprise
and send the output.  Put a phone 30 cm away with Bluetooth on and its "visible" screen open (and
earbuds in pairing mode if available).

1. Deploy `walpad-btbridge` + `walpad-bluetooth`, `walpad-bluetooth restart`.  In
   `/var/log/walpad-btbridge.log` expect `BD address xx:xx:xx:xx:62:30 from nvdata`, five `init ... ok`
   lines and `init done: 5 ok, 0 skipped, 0 failed, 0 timed out`.  `dmesg | grep -i 'MTK-BT'` should show
   the writes without an ASSERT.  `skipped`/`failed` lines name the vendor opcode the firmware rejects.
2. Baseline scan: `hciconfig hci0 up; hcitool -i hci0 scan --flush-cache` (BR/EDR, 10 s) and
   `timeout 15 hcitool lescan --duplicates` (LE), with `btmon -w /tmp/bt.btsnoop` running; read it with
   `btmon -r`.  Look for `Inquiry Result with RSSI`, `Extended Inquiry Result`, `LE Advertising Report`
   and any `Hardware Error`.  Nothing from a phone 30 cm away means RF is dead (3-7); results with weak RSSI
   (below -80 dBm at 30 cm) mean TX/antenna/coex.
3. Controller sanity: `hciconfig -a hci0` (BD address = the nvdata one, Manufacturer MediaTek),
   `btmgmt info`, `hcitool cmd 0x04 0x0001` (Read Local Version), `hcitool cmd 0x04 0x0003` (Read Local
   Features).  A controller that answers but hears nothing points at RF, one that does not answer at the
   transport.
4. Wi-Fi coexistence (shared 2.4 GHz antenna; Wi-Fi is on 5 GHz "Loff 5g"): `iw dev wlan0 disconnect` or
   stop walpad-wifi traffic, repeat step 2.  If devices appear, Wi-Fi/BT arbitration (PTA, the `coex_*` keys
   in `BT_FW.cfg`, the 0xFC22 `Coex_Performance_Adjust` of the older scripts) is the cause.  Then the reverse:
   Wi-Fi connected on 2.4 GHz versus 5 GHz.
5. Sequence variants, one change per run (`hcitool -i hci0 cmd 0x3f <ocf> ...` works while the bridge runs):
   (a) `INIT=0` in `/etc/walpad/bluetooth.conf` for the old behaviour; (b) re-send `FC93` as `00` * 16 (an
   all-`ff` offset may be read as a large negative dB value) or never send it; (c) send the optional
   `0xFCC5` PIP entry; (d) reset first.
6. LE scan parameters: `hcitool cmd 0x08 0x000b 01 10 00 10 00 00 00` (active, 10 ms/10 ms, public own
   address) then `hcitool cmd 0x08 0x000c 01 00`; vary active/passive and own-address type.
7. Calibration: Android's `nvram_daemon` restores BT/RF calibration; check `dmesg` at BT-on for
   `EFUSE`/`cal`/`RF` lines and for the patch name (`soc1_0_ram_bt_1_1_hdr.bin`) being requested.
8. Mechanical: keep the phone close, no metal in between; check the internal antenna flex if the tablet
   was opened.

Fallbacks, most likely first: (1) shared-antenna coex arbitration with Wi-Fi, (2) the all-`ff` TX power
offset or the `Set_Radio` values read literally, (3) scan parameters / own-address type, (4) missing
calibration restore normally done by a vendor daemon, (5) the kernel BT patch not loaded from
`/vendor/firmware` at `open()`, (6) the antenna or controller hardware itself.
