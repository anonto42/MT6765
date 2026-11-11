# GPS on the WalPad 8G: the mnld protocol, walpad-gnss and the device experiments

Owner request 2026-11-11: "GPS was not working" and "I want the GPS control like the Wi-Fi and
hotspot". Status: **built and tested on the PC only; never run against the real mnld.** Everything
below was recovered statically from the stock Android image; section 4 lists what must be checked on
the tablet, in a safe order.

## 1. The problem

`walpad-gps start` already loads `gps_drv.ko`, creates `/dev/stpgps` `/dev/gps_emi` `/dev/fw_log_gps`,
mounts the VNDK APEX and runs MediaTek's `mnld` (`/vendor/bin/mnld`, 32-bit Thumb-2). mnld runs, but it
publishes positions only to Android's GNSS HAL over private datagram sockets. With no HAL listening
its log says `safe_sendto path=[mtk_mnl2hal] Connection refused` (that errno, not ENOENT, is what an
abstract AF_UNIX datagram address with no owner returns: the first hint that the sockets are in the
abstract namespace).

The Android side is: `android.hardware.gnss@2.1-service-mediatek` (64-bit) loads
`android.hardware.gnss@2.1-impl-mediatek.so`, which loads `/vendor/lib64/hw/gps.default.so`, the
library that owns the sockets. `walpad-gnss` replaces that chain.

## 2. How it was recovered (static analysis only)

Extraction: `dd` of `vendor_a` (offset 1996488704, 305889280 bytes) from `build/android-restore/super-raw.img`,
then `debugfs -R "dump ..."` (read-only). Files: `/bin/mnld` (BuildID f2bd1aba, ELF32 ARM, stripped),
`/lib64/hw/gps.default.so` (42 KB, ELF64 aarch64, **the protocol library**), `/lib64/hw/android.hardware.gnss@2.1-impl-mediatek.so`,
`/etc/MNL_Config.xml`, `/etc/init/*.rc`. Tools: `objdump` (binutils-multiarch, `-M force-thumb` for mnld) in a
throwaway, non-privileged `debian:stable-slim` container. Nothing was executed.

### 2.1 Transport (confidence: high)

`gps.default.so` `socket_bind_udp` (0x9420) and `safe_sendto` (0x9688): `socket(AF_UNIX=1, SOCK_DGRAM=2, 0)`;
a zeroed `sockaddr_un`; the name is `strncpy`-ed to `sun_path + 1` (so `sun_path[0] == 0`: **abstract
namespace**), at most 106 bytes; `bind` / `sendto` with length **110** (2 + 108). Abstract names compare
the whole length, so the address is the name padded with NULs to 108 bytes; `walpad-gnss` builds exactly
that (`io::abstract_addr`). `sendto` is retried on EINTR and, 10 times with 100 ms sleeps, on EAGAIN.
mnld uses the same helper (sockaddr length 110 and `strncpy(.., 106)` at 0x1aefc-0x1af54).

| Socket | Bound by | Name |
|---|---|---|
| `mtk_mnl2hal` | the HAL (now walpad-gnss) | strings at 0x2c6d of gps.default.so, 0x8527 of mnld |
| `mtk_hal2mnl` | mnld | 0x3174 of gps.default.so, 0xaf6e of mnld |

There is no registration: mnld just `sendto`s to `mtk_mnl2hal`; the HAL `sendto`s commands to `mtk_hal2mnl`.
mnld also talks to `mtk_agpsd` (`mtk_agps2mnl`, `/data/agps_supl/*`): not needed for positions.

### 2.2 Framing (confidence: high)

Little endian, no padding. Datagram = `i32 version (=1)`, `i32 command`, payload.
`put_int` is 4 LE bytes (0x86c8), `put_long` 8, `put_double`/`put_float` IEEE, `put_byte` 1,
`put_string` = `u8 1`, `i32 len+1`, bytes, NUL (or `u8 0` for NULL) (0x88c8),
`put_binary` = `i32 len`, bytes (0x89c0). Every sender memsets a 0x8000-byte buffer; the receiver reads up to 0x8000.
Example: `hal2mnl_gps_start` (0x6180) sends `01 00 00 00 66 00 00 00`.

### 2.3 HAL -> mnld commands (the `hal2mnl_*` functions, 0x6020-0x76e8)

| cmd | function | payload | used by walpad-gnss | confidence |
|---|---|---|---|---|
| 0x65 | gps_init | none | yes | high (HAL), medium (that mnld needs it) |
| 0x66 | gps_start | none | yes | high |
| 0x67 | gps_stop | none | yes | high |
| 0x68 | gps_cleanup | none | at exit | high |
| 0x69 | gps_inject_time | `i64 utc_ms, i64 reference_ms, i32 uncertainty_ms` | yes, with the system time | medium-high |
| 0x6a | gps_inject_location | `f64 lat, f64 lon, f32 accuracy` | no | medium |
| 0x6b | gps_delete_aiding_data | `i32 flags` | no (cold start) | medium |
| 0x6c | gps_set_position_mode | `i32 mode, i32 recurrence, i32 min_interval_ms, i32 preferred_accuracy, i32 preferred_time, u8 low_power` | yes: 0,0,1000,0,0,0 | medium-high |
| 0xc9-0x1f6 | data connection / network state | strings | no (AGPS) | low |
| 0x12d-0x25f | server, ref location, NI, measurement/navigation switches | various | no | low |

The HAL's own sequence is init, position mode, start. mnld's receiving dispatcher (`hal2mnl_hdlr`, strings
`hal2mnl_hdlr() ... is NULL`) was not decoded; its command numbers are taken from the sender side.

### 2.4 mnld -> HAL messages (confirmed from both sides)

The HAL's `mnl2hal_hdlr` (0x77d0) reads `i32, i32 cmd` and dispatches through a jump table at 0x2430
(`cmd - 0x65`, 0xd6 entries). The senders in mnld sit at the Thumb addresses where the command is loaded:
0x190a4 (0x65), 0x19140 (0x66), 0x191ec (0x67), 0x1927e (0x68), 0x19314 (0x6c), 0x1940e (0x6d).

| cmd | meaning | payload | confidence |
|---|---|---|---|
| 0x65 | location (`mnl2hal_location`) | `binary` of **88 bytes** (mnld, `movs r3,#88` at 0x190b2); the stock HAL copies only the first 64 | ids and size high |
| 0x66 | GPS status (`mnl2hal_gps_status`) | `binary(4)`: `i32` 0 none, 1 session begin, 2 session end, 3 engine on, 4 engine off | high |
| 0x67 | satellites (`mnl2hal_gps_sv`) | `binary` of **6248 bytes** (`movw r3,#6248` at 0x191fa) | id and size high; **layout not decoded** |
| 0x68 | NMEA (`mnl2hal_nmea`) | `i64 timestamp_ms`, `string` (the sentence, CR LF), `i32 length` | high |
| 0x69 | capabilities | `binary(4)` | medium |
| 0x6c | GNSS measurements | `binary` of 19304 bytes | medium (ignored) |
| 0x6d | navigation event | `binary` | medium (ignored) |
| 0x12d | request UTC time | none seen | medium: from the `request_utc_time is NULL` branch of the jump table; answered with `inject_time` |
| 0xc9, 0xca | request / release wakelock | | medium (ignored) |
| 0x12e-0x13a | request data connection, ref location, set id, NI notify, NFW access, update_gnss_name ... | | low (ignored) |

**Location blob (`GpsLocation_ext`, 88 bytes).** Offsets from the impl's `convertToGnssLocation1_0` (reads
`[x0+8]` as the flags byte, doubles at 16/24/32, floats at 40/44, timestamp at 56, four floats at 64..80 for the
HIDL accuracy fields) and from the legacy `GpsLocation` (accuracy at 48): `u64 size` @0, `u16 flags` @8
(1 lat/long, 2 altitude, 4 speed, 8 bearing, 0x10 horizontal accuracy), `f64 lat` @16, `f64 lon` @24,
`f64 alt` @32, `f32 speed` (m/s) @40, `f32 bearing` @44, `f32 accuracy` @48 (legacy), `i64 utc_ms` @56,
`f32 horizontal accuracy` @64. The parser takes the accuracy from @64, else @48. Confidence: medium-high
for lat/lon/time (they must be plausible: range-checked, the flag must be set), medium for accuracy.

**Satellites.** The binary 0x67 layout is unknown (6248 = 8 + 24 * 260 or 32 + 24 * 259, neither is
obvious, and the HAL's `gpsSvStatusCb` for the legacy struct is a no-op in the 2.1 impl). The HAL's
extended struct for 0x6a (7752 bytes = 24 + 48 * 161: svid i16 @0, constellation u8 @2, C/N0 f32 @4,
elevation f32 @8, azimuth f32 @12, flags u8 @16) is never sent by this mnld. **Satellites in view and used
therefore come from the NMEA the same mnld sends** (GSV for the list with elevation/azimuth/SNR, GSA for the
ids used in the fix, GGA's satellite count as a fallback). The first raw 0x67 blob is saved to
`/run/walpad/gnss-sv.bin` so the layout can be worked out later from a real capture.

### 2.5 What mnld needs (confidence: medium, from strings)

- Devices: `/dev/stpgps`, `/dev/gps_emi` (rw), `/dev/ttygserial` (debug), created by `walpad-gps`.
- Files it creates under `/data/vendor/gps/` (`walpad-gps` makes the directory): `mtkgps.dat`, `mtkgps_ofl.dat`,
  `MTKNAV.DAT`, `EPO.DAT`, `QEPO*.DAT`, `gps.conf`, `mnl.prop`, `gpsdebug.log`; also reads
  `/mnt/vendor/nvcfg/almanac.dat` and the modem calibration files under `/mnt/vendor/nvdata/md/NVRAM/CALIBRAT/`.
- Config: `/vendor/etc/MNL_Config.xml` (GnssMode 6 = GPS+GLONASS+Galileo+BeiDou; Bluesky off); an optional
  `/data/vendor/gps/mnl.prop` (property `persist.vendor.radio.mnl.prop`, fallback `/sbin/mnl.prop`) whose keys
  include `DEBUG_NMEA`-style switches, `EPO_ENABLE`, `GNSS_MODE`; `/vendor/etc/gnss/agps_profiles_conf2*.xml`.
- Properties it reads (all optional; the 32-bit shim answers from `/tmp/wpd-props32`, never `/tmp/wpd-props`):
  `persist.vendor.connsys.chipid`, `vendor.gps.*` (`vendor.gps.clock.type`, `vendor.gps.gps.version`,
  `vendor.gps.almanac`), `debug.debug_nmea`, `debug.mnl`, `vendor.gpsdbglog.enable|path|file`,
  `ro.vendor.mtk_log_hide_gps`.
- Stock `init.mt6765.rc` starts it as `user gps` with a `socket mnld stream`; our script runs it as root and does
  not create that socket.
- Other NMEA outputs inside mnld (`mtk_gps_mnl_create_dbg_port`, "dsp port / nmea port / nmea dbg port" in its
  config dump) were not followed: the socket message 0x68 is the supported path.

## 3. Implementation

- `crates/walpad-gnss`: `wire` (datagram codec, tests from hand-built messages), `nmea` (GGA/RMC/GSV/GSA,
  checksum, total parsers), `state` (the receiver state machine with an injected clock), `io` (abstract
  sockets with the 110-byte address, the NMEA pty), `daemon` (loop, time injection, resend if mnld is silent for
  15 s up to 6 times, atomic `gnss.json`, SIGTERM sends stop + cleanup). Tests include a fake mnld on private
  socket names (`tests/daemon.rs`).
- `config/rootfs/usr/local/sbin/walpad-gps`: `start` also starts `walpad-gnss`; `stop` stops it first; key=value
  output for the core; tests in `tests/shell/gps.sh` with a fake root.
- `walpad-core` `gnss.rs`: the switch (`/var/lib/walpad/gps-enabled`), the script with timeouts, `status.gps`,
  restart after boot. IPC in `docs/rust/CONTRACTS.md` ("GPS").
- `walpad-shell` `gps_screen.rs`: the screen, the Quick Settings row, the header mark; `docs/rust/UI.md`.
- Position privacy: coordinates are only in `gnss.json` (0644, tmpfs), the control socket status for the owner's
  shell and the NMEA pty; never in a log, in argv, or in `/run/walpad/status.json`.

## 4. Device experiments (over SSH; none writes a partition; run in this order)

GPS needs **open sky** (a window is not enough; a roof or open ground is), the first fix after a long time can
take 5-15 minutes (cold start, no almanac, no AGPS server: `mtk_agpsd` is not run), and the receiver wants a
sane time: `ntpd` (running since the Wi-Fi work) sets the clock, and walpad-gnss injects it
(`time injected into mnld` in `/var/log/walpad-gnss.log`).

0. **Preconditions.** Wi-Fi on and `wlan0` up (the GPS shares CONNSYS: `grep wmt_drv /proc/modules`); `date`
   is current; battery charged. Never `rmmod` any CONNSYS module; do not start Bluetooth and GPS at the same
   moment on a fresh boot (known issue: opening `/dev/stpbt` before wmt_launcher serves wedges CONNSYS).
1. **Deploy.** `scripts/build_rust.sh`, `scripts/deploy.sh` (core, shell, ctl, gnss), copy
   `config/rootfs/usr/local/sbin/walpad-gps` to `/usr/local/sbin/` and
   `ln -sfn ../lib/walpad/current/walpad-gnss /usr/local/bin/walpad-gnss` (a new image does both). Restart core.
2. **Sockets and protocol round trip.** `walpad-gps start`; expect `state=on`. Then
   `ss -x -a | grep mtk_` (abstract sockets `@mtk_hal2mnl` owned by mnld and `@mtk_mnl2hal` by walpad-gnss);
   `tail /var/log/walpad-gnss.log` (`session commands sent to mnld`); `tail /var/log/walpad-mnld.log` (the
   `Connection refused ... mtk_mnl2hal` lines must stop). `cat /run/walpad/gnss.json` twice, 10 s apart: the
   `counters` (`status`, `nmea`, `location`, `sv`) must grow. **If they stay 0** while mnld runs: the address
   convention is wrong. Then `strace -f -e trace=sendto,bind -p $(pidof mnld)` (strace is installed for the modem
   work) shows the exact name and length mnld uses; adjust `ADDR_LEN` in `crates/walpad-gnss/src/wire.rs`.
3. **NMEA pty.** `timeout 60 cat /run/walpad/nmea`: expect `$GPGSV`/`$GNGSA`/`$GPRMC` lines. With no fix the RMC
   is `V` and GGA quality 0, but GSV should list satellites after a minute or two. If lines are empty while
   `counters.status` grows, NMEA output is off in mnld: try `debug.debug_nmea=1` in `/tmp/wpd-props32` (ONLY that
   file, never `/tmp/wpd-props`) and restart mnld, or an `mnl.prop` in `/data/vendor/gps/`.
4. **Satellites.** Take the tablet outside, open sky, screen on: GPS screen "N in view" should rise above 0, SNR
   bars above 20 dB-Hz appear within a few minutes. 0 satellites after 10 minutes outdoors with NMEA flowing =
   an RF/antenna or firmware problem (`dmesg | grep -i gps`, `/var/log/walpad-mnld.log` for
   `mtk_gps_sys_init err`, `open gps_emi failed`), not the protocol.
5. **First fix.** Keep it still, wait up to 15 minutes. `gnss.json` `fix` must appear with plausible latitude and
   longitude (compare with a phone), `accuracy` tens of metres at first. A hot restart afterwards (`gps off`,
   `gps on`) should fix in well under a minute (mnld keeps `/data/vendor/gps/mtkgps.dat`; **`/data` is the
   rootfs here, so it persists**).
6. **Location blob layout.** Compare `fix.lat/lon/alt/speed/bearing/accuracy/time` with NMEA GGA/RMC. A mismatch
   in speed/bearing/accuracy means the offsets of section 2.4 are off: dump one 88-byte location datagram
   (temporary `eprintln!` of the blob, or `strace -e trace=sendto -s 200 -p $(pidof mnld)`) and fix
   `Location::parse`. Position from NMEA fallback keeps the screen working meanwhile (state machine prefers the
   binary message for 3 s, else NMEA RMC/GGA).
7. **Satellite blob.** `xxd`-less hexdump of `/run/walpad/gnss-sv.bin` (6248 bytes, `od -A d -t x1z`): compare
   with the GSV list (svid, elevation, azimuth, SNR) to recover the layout; then the signals no longer depend on
   NMEA.
8. **Switch behaviour.** From the UI: Quick Settings GPS switch on/off five times (the core keeps one start/stop
   at a time; `busy` while running); reboot with the switch on (the core starts it after boot and retries every
   minute until Wi-Fi is up); switch Wi-Fi off, then GPS on: expect "turn Wi-Fi on first".
9. **Aiding (only if the fix takes too long).** `mtk_agpsd` (`/vendor/bin/mtk_agpsd`, 2.2 MB, sockets `agpsd`,
   `agpsd2`, `agpsd3`) would fetch EPO/QEPO files; it needs internet and the carrier profile in
   `/vendor/etc/gnss/`. Try it only after 1-8 work. `hal2mnl_gps_delete_aiding_data` (0x6b, flags 0xffff) forces a cold
   start for timing tests; it is built (`wire::delete_aiding`) but not exposed.
10. **Soak.** Leave it on for an hour outdoors: memory of walpad-gnss (`/proc/$(pidof walpad-gnss)/status`,
    expect < 3 MB), CPU, `gnss.json` still updating, the pty not blocking mnld.

Safe fallbacks: `walpad-gps stop` (stops the daemon and mnld; the kernel module stays),
`echo 0 > /var/lib/walpad/gps-enabled`, or `walpad-ctl gps off`.
