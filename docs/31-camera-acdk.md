# Camera through MediaTek ACDK (2026-11-10)

Status: API recovered from the shipped binaries, helper and script built and host-tested;
**not yet run on the tablet** (device test plan at the end).

Goal: still photos from the built-in cameras without the Android camera HAL, by calling MediaTek's
factory-test library `/vendor/lib64/libacdk.so` (ACDK, "Android Camera Development Kit") from our own
small client, `camacdk`.  `/vendor/bin/factory` can run through the bionic linker, but its GUI draws
through MediaTek's overlay engine and shows garbage on the panel, so the factory binary is only a
reference for the call sequence.

Pieces:

| Piece | Where |
|---|---|
| `camacdk` (bionic aarch64, freestanding C) | `tools/camacdk/camacdk.c`, built by `tools/camacdk/build.sh` into `data/camera/camacdk` |
| environment + launcher | `config/rootfs/usr/local/sbin/walpad-camera` |
| image install | `scripts/build_rootfs.sh` puts `camacdk` into `/usr/local/bin` (skipped if not built) |
| host tests | `tests/shell/run.sh` (fake root, fake mount/unshare/unzip) |

## How it was recovered

`vendor_a` was cut out of `build/android-restore/super-raw.img` with the offsets of
`data/super/layout.json` (`dd`, offset 1996488704, 305889280 bytes), `/bin/factory` (inode 90) and
`/lib64/libacdk.so` read with `debugfs -R "dump ..."`, disassembled with `aarch64-linux-gnu-objdump`
(a throwaway `debian:stable-slim` + `binutils-aarch64-linux-gnu` image, no `--privileged`).  Both
binaries are stripped but carry a `.gnu_debugdata` section (xz-compressed mini symbol table): it names
about 760 functions in `factory` and the `AcdkMain`/`AcdkMhal*`/`surfaceView` classes in `libacdk.so`,
and `libacdk.so`'s full dynamic symbol table is exported.  A small script annotated every `adrp`+`add`
pair with the string it addresses.  Addresses below are virtual addresses of the shipped files
(`factory` BuildID `07f239fc...`, `libacdk.so` BuildID `ce47cd73...`, Android 11 / API 30 vendor).

Confidence: **H** = read directly from code that every path goes through, **M** = read from code but
one step of inference (a field name, an enum meaning), **L** = guess from strings or plausibility.

## 1. The C API of libacdk.so (exports)

All six are in `AcdkIF.cpp` (log tag `ACDK_IF`, source path
`vendor/mediatek/proprietary/hardware/mtkcam/main/acdk/v4.0/src/acdk/AcdkIF.cpp`):

| Export | Address | Behaviour | Conf. |
|---|---|---|---|
| `MDK_Open()` | `0x10078` | if already open: log "ACDK device already opened", return 0. Else `AcdkBase::createInstance(0)` (allocates a 0x9f0-byte `AcdkMain`), stores it in `g_pAcdkBaseObj`, sets the opened flag, returns 1 | H |
| `MDK_OpenBypass(flag)` | `0x10130` | same with `createInstance(flag)`; **bit 2 (value 4) selects `fakeSurfaceView`**, see section 4 | H |
| `MDK_Init()` | `0x10298` | needs opened; `AcdkMain::init()` through vtable slot 1; returns 1 on success, 0 with "Fail to init AcdkCCTCtrl" | H |
| `MDK_IOControl(code, info)` | `0x10438` | needs opened; accepts `code` in `0x80001001..0x8000100f`, calls `AcdkMain::sendcommand` (vtable slot 10) and **always returns 1**; the command's own result is only logged. Not opened / bad code: 0 | H |
| `MDK_DeInit()` | `0x10360` | vtable slot 2 (`uninit`); returns 1 on success | H |
| `MDK_Close()` | `0x101e8` | vtable slot 0 (destructor) and clears the opened flag; returns 1 | H |

`MDK_IOControl` evidence:

```
10438 stp x29,x30,[sp,#-48]!            10498 mov  w4,w19                 ; log "Reveive IOControl Code:0x%x"
...                                      104a0 mov  w8,#0xefff ; movk w8,#0x7fff,lsl #16
10448 mov  x20,x1        ; x20 = info    104a8 add  w8,w19,w8 ; cmp w8,#0xe ; b.hi  can't-interpret
104e8 ldp  x2,x4,[x20]        ; pIn, pOut
104ec ldp  w3,w5,[x20,#16]    ; inLen, outLen
104f0 ldr  x6,[x20,#24]       ; pRealOutLen
104f8 ldr  x8,[x0]; ldr x8,[x8,#80]; blr x8   ; AcdkMain::sendcommand(this, code, pIn, inLen, pOut, outLen, pReal)
1051c mov  w0,#1                         ; result of sendcommand is dropped
```

so `ACDK_FEATURE_INFO_STRUCT` (32 bytes) is `{void *pIn; void *pOut; u32 inLen; u32 outLen; u32 *pRealOutLen;}` (**H**).

The factory reaches these through `Mdk_Open/Close/Init/DeInit/IOControl` wrappers (`entry_mdk.cpp`,
`factory` `0x9ada8..0x9b1c8`) that `dlopen("libacdk.so")` once (`getModuleEntry`, `0x9ae40`) and
`dlsym` the `MDK_*` name; the wrappers add nothing else (**H**).  Our client does the same.

## 2. Command numbers (ACDK_CMD_*)

`AcdkMain::sendcommand` (`libacdk.so 0x17aa8`) dispatches `code - 0x80001001` (`cmp w8,#0xb`) through a
halfword jump table at `0xcea6`; each handler logs its name ("[%s] ACDK_CMD_..."), which gives the
names (**H**):

| Code | Name | Handler | In | Out | Behaviour |
|---|---|---|---|---|---|
| `0x80001001` | PREVIEW_START | `0x17b18` | `ACDK_PREVIEW_STRUCT` (40 B) | - | logs width/height/HDREn/OPType, `AcdkMain::startPreview(cb)` |
| `0x80001002` | PREVIEW_STOP | `0x17c44` | - | - | `AcdkMain::stopPreview` (vtable slot 4) |
| `0x80001003` | CAPTURE | `0x17c94` | `ACDK_CAPTURE_STRUCT` (56 B) | - | validates the format, `AcdkMain::takePicture(...)` (synchronous) |
| `0x80001004` | QV | `0x17e68` | - | - | "no need to call QV command right now" (no-op) |
| `0x80001005` | RESET_LAYER_BUFFER | `0x17be0` | - | - | surface view `resetLayer(0)` |
| `0x80001006` | SET_SRC_DEV | `0x17e90` | `int` (4 B) | - | selects the sensor, `setSrcDev(int)` (vtable slot 8) |
| `0x80001007` | SET_OPERATION_MODE | `0x17ef4` | `int` | - | 1 -> 1, 2 -> 2, other -> 0 |
| `0x80001008` | SET_SHUTTER_TIME | `0x17f3c` | `u32` | - | `setShutterTime` |
| `0x80001009` | GET_SHUTTER_TIME | `0x17f70` | - | `u32` | reads `mGetShutTime` (`this+2504`) |
| `0x8000100a` | GET_CHECKSUM | `0x17f8c` | - | `u32` | sensor test-pattern checksum (`this+2508`); the factory auto test uses it |
| `0x8000100b` | GET_AF_INFO | `0x17fa8` | - | `u32` | AF state from the mhal |
| `0x8000100c` | GET_SENSOR_MODE_NUM | `0x17c20` | - | `u32` | number of sensor modes (`this+2516`) |

The factory uses only 1001, 1002, 1003, 1006 and 100a.  `0x8000100d..f` pass `MDK_IOControl`'s range check but
`sendcommand` rejects them (`cmp w8,#0xb; b.hi`).

`ACDK_CMD_SET_SRC_DEV` values (**H** for 1 and 2): the factory's `camera_main_preview_entry`
(`0x614e8`) stores 1 in a global and `camera_sub_preview_entry` (`0x614f8`) stores 2, then both fall into
`camera_entry`.  `sendcommand` passes 1 and 2 unchanged (legacy values 8/0x10/0x20/0x40 are remapped
through a table at `0xcee0`).  So **main = 1, sub = 2**.

## 3. Struct layouts

Sizes come from the factory's `acdk_feature_info.inLen` constants (`ldr d0,[0x2bbd8]` = `{40,0}` for
preview, `[0x2bbe8]` = `{56,0}` for capture, `[0x2bb08]` = `{4,0}` for SET_SRC_DEV); field offsets come
from the `sendcommand` handlers' loads and from the log strings.

### ACDK_PREVIEW_STRUCT (40 bytes) - `sendcommand` handler `0x17b18`, built in `camera_preview_test` (`factory 0x61a10`)

| Off | Type | Meaning | Conf. |
|---|---|---|---|
| 0 | u32 | preview width (factory: 320) | H |
| 4 | u32 | preview height (factory: 240) | H |
| 8 | u16 | stored in `AcdkMain+472`; factory passes 1 | M (name unknown) |
| 10 | u16 | "Preview HDREn" -> `+508`; factory 0 | H |
| 16 | ptr | preview callback, passed to `startPreview(cb)`; the factory passes **NULL** and nothing in `libacdk` calls it | M |
| 24 | u32 | factory passes 2, not read by `sendcommand` | L |
| 28 | u32 | "Preview OP Type", 0 = normal (`this+468`); factory 0 (1 in a boot-mode branch) | H |

Factory code: `ldr d1,[x8,#3000]` loads `{320,240}`, `strh w8,[sp,#24]`, `stp w20,w0,[sp,#40]`,
`mov w0,#0x1001; movk w0,#0x8000,lsl #16; bl Mdk_IOControl` (`0x61ad4..0x61ae8`); a failure prints
"ACDK_PREVIEW_STRUCT Fail" (the anchor string).

### ACDK_CAPTURE_STRUCT (56 bytes) - handler `0x17c94`, built in `camera_cap_key_handler` (`factory 0x61658`)

| Off | Type | Meaning | Conf. |
|---|---|---|---|
| 0 | u32 | "Capture Mode"; factory 1 | M |
| 4 | u32 | "Operation Mode" (`this+76`); manual capture passes 2, auto test 1 | M |
| 8 | u32 | output format. Accepted: 4, 5, 8, 16, 32, 64 (bit mask `0x1000000010001011` at `0x17e28..0x17e3c`); **0x40 = JPEG** (hardware encoder, section 5); 8 = PURE_RAW10, 0x80 appears in the YUV callback | H for 0x40, M for others |
| 12 | u16 | width, 0 = "use original capture size" (`0x14bd8`) | H |
| 14 | u16 | height | H |
| 16 | ptr | capture callback `void cb(void *)` | H |
| 24 | u32 | "cap cnt"; factory 1 | H |
| 28 | u32 | "isSave"; factory 1 | M |
| 36 | u32 | "MFLL_En" | H |
| 40 | u16 | "mHDR_En" | H |
| 44 | u32 | "eMultiNR" | H |

Manual capture in the factory: `{1,2}`, format `0x40`, callback `vCapCb` (`0x62168`, just sets a flag),
count 1, save 1 (`0x616f8..0x61764`), then `Mdk_IOControl(0x80001003, ...)`, then waits for the flag.
Auto test (`vAutoCapCb_patg`, `0x61b68`) uses format 8 (or `0x100` if a prop says so, which
`sendcommand` rejects) and first calls `0x8000100a` (checksum).

### Capture callback argument (JPEG) - built in `takePicture` (`0x1522c..0x156ec`)

| Off | Type | Meaning | Conf. |
|---|---|---|---|
| 0 | u32 | type = capture format (0x40) | H |
| 8 | ptr | JPEG bitstream (`AcdkMain+968`); valid only during the callback | H |
| 16 | u32 | width | M |
| 20 | u32 | height | M |
| 24 | u32 | `imgBufInfo.imgSize` = **bitstream buffer size** (`this+960`, "create Jpeg bitstream buff"), not the encoded length (the encoded length is a local, logged as "Jpeg end done size") | M |

Consequence: the helper copies `size` bytes in the callback and trims at the last `FF D9`.

## 4. Open flow, and why the overlay engine can be bypassed

`AcdkMain::AcdkMain(_ACDK_BYPASS_FLAG)` (`libacdk.so 0x10590`) calls
`AcdkSurfaceView::createInstance((flag >> 2) & 1)` (`0x1094c`: `ubfx w0,w20,#2,#1`).
`AcdkSurfaceView::createInstance(int)` (`0x2a770`) logs "use fakeSurfaceView" and builds the fake
object for any non-zero argument; for 0 it builds the real `surfaceView` (`0x3a8` bytes) whose `init`
opens `/dev/graphics/fb0`, mmaps it and issues `MTKFB_SET_VIDEO_LAYERS` ioctls (`0x2ab38..0x2afe0`: "use
overlay", "use mmap").  That is the display takeover that garbles the panel.  Every method of
`fakeSurfaceView` is `mov w0,wzr; ret` (`0x24a58`; `getSurfaceInfo` reports 480x640, `getNumLayer` 4).
**`MDK_OpenBypass(4)` therefore gives a camera session with no framebuffer access** (**H**).

Other facts from `AcdkMain::init` (`0x110e0`):

- reads `/sys/class/BOOT/BOOT/boot/boot_mode`; a failure is logged but non-fatal (stores 0) (**H**)
- creates two `AcdkMhal` objects (`AcdkMhalBase::createInstance(0)` Eng and `(1)`), an `AcdkUtility`
  and calls `AcdkMhal::acdkMhalInit` on both; reads `vendor.camera.acdk.debug`
- `sensorInit` (`0x11648`) enumerates sensors through `NSCam::IHalSensorList::get()` (exported by
  `libcam.halsensor.so`) and `getMtkcamModuleFactory`, logs "name:%s type:%d" and
  "index:%d, SensorDevIdx:%d", fails with "Sensor Not found" otherwise
- the `/proc/device-tree/chosen/atag,boot` check ("Factory mode boot!") is in the **factory binary**
  (`getBootMode` `0x4ad68`, reads `{u32 size; u32 tag; u32 mode}`, accepts mode 4 and 6), **not** in
  `libacdk.so` (no `atag,boot` string in it).  `libcam.halsensor.so` and `libcam.iopipe.so` read
  `/proc/device-tree/chosen/atag,devinfo`, which is a different node.

## 5. Capture path, preview and output files

`ACDK_CMD_CAPTURE` -> `AcdkMain::takePicture(mode, fmt, cb, w, h, count, isSave)` (`0x14968`),
**synchronous**: it runs inside the `MDK_IOControl` call.

1. state manager check (a repeated command returns 7, "redundent command"), reads
   `vendor.camera.acdkdump.raw`
2. for format `0x40`: the sensor frame is captured, `AcdkUtility::queryImageSize` / `createMemBuf`
   (ION) allocate the YUV and bitstream buffers
3. hardware JPEG: `JpgEncHal` (`libJpgEncPipe.so`, `/proc/mtk_jpeg`): `LevelLock`, `setEncSize`,
   `setSrcAddr`, `setQuality(100)` (hard-coded), `setDstAddr`, `enableSOI`, `start` ("Jpeg end done size:
   %d", `0x14f24..0x15068`)
4. the callback (`blr x28` at `0x156ec`) receives the structure above
5. `takePicture` stops the preview itself ("stop preview", `0x15730`), so the later
   `ACDK_CMD_PREVIEW_STOP` from the factory logs a harmless redundant-command

Files libacdk writes by itself, all below the fixed directory `/data/vendor/` (string at `0x8d6b`; the
directory must exist):

- `/data/vendor/acdkCap.jpg`: `AcdkMain::handleCapCB` (`0x16fb0`) writes the capture callback data
  (`fopen/fwrite/fflush/fsync`); `saveJPEGImg` (`0x172f8`) writes the same name with the exact encoded
  length.  (The factory's own "[ATA] Open /data/acdkCap.jpg" is a separate read, not libacdk's path.)
- with `vendor.camera.acdkdump.enable` = 2 or 3 (read per frame, `0x16720..0x16750`), `handlePreviewCB`
  (`0x164f8`) dumps each preview frame to `/data/vendor/acdkPrv2_<w>x<h>_<stride>.yuy2`, always the same
  name (overwritten).  Value 4 dumps capture data, other values select RAW dumps.  `camacdk preview` uses
  value 2 and polls that file; `capture` forces 0.
- `handlePreviewCB` copies each frame into the surface buffer, never calls the preview callback of
  `ACDK_PREVIEW_STRUCT`.  With the fake surface view the surface buffers still exist (`AcdkMain+0x418`,
  stride 0x28 per entry); whether their size field is filled with the fake view is **not verified**
  (the dump writes `[entry+1048]` bytes from the preview VA), hence the preview command is the least
  certain part (**L/M**).

Other files and nodes touched by the libraries (strings): `/dev/camera-isp`, `/dev/kd_camera_hw`
(`libcam.halsensor.so`, `libcamdrv_isp.so`), `/proc/m4u`, `/dev/gz_kree`
(`libcamdrv_imem.so`), `/proc/mtk_jpeg`, ION, `/mnt/vendor/nvcfg/camera`
(`libcam.hal3a.v3.nvram.so`), `/data/vendor/camera_dump/...` (3A dumps, only with their own props).

## 6. The recovered call sequence (what the factory does, and what camacdk does)

Factory (`camera_entry` `0x60868`, `acdkIFInit` `0x613a8`):

```
Mdk_Open()                                  ; MDK_Open
Mdk_IOControl(0x80001006, {&srcDev, 4})     ; main=1 / sub=2         "ACDK_FEATURE_SET_SRC_DEV Fail" on 0
Mdk_Init()                                  ; MDK_Init
Mdk_IOControl(0x80001001, ACDK_PREVIEW_STRUCT{320,240,...})   (camera_preview_test)
  ... user presses the key ...
Mdk_IOControl(0x80001003, ACDK_CAPTURE_STRUCT{1,2,0x40,...,cb,1,1})   -> /data/vendor/acdkCap.jpg
Mdk_IOControl(0x80001002, {})               ; camera_preview_stop (0x60d10)
Mdk_DeInit(); Mdk_Close()
```

`camacdk capture <main|sub> <out.jpg>`:

```
MDK_OpenBypass(4)  (CAMACDK_BYPASS to change; 0 = MDK_Open)  -> SET_SRC_DEV -> MDK_Init
PREVIEW_START 320x240 -> sleep 2.5 s (3A) -> CAPTURE{1,2,0x40,0,0,cb,1,1}
cb copies the bitstream; trim at the last FF D9; write out.jpg (tmp + rename)
fallback if the callback never fired: /data/vendor/acdkCap.jpg newer than the start
PREVIEW_STOP -> MDK_DeInit -> MDK_Close
```

Every phase runs under `alarm()` (open 30 s, init 60 s, preview start 30 s, capture 45 s, ...); the
handler logs "TIMEOUT in phase '<name>'" and exits 124.  Exit codes: 0 ok, 1 usage, 2 libacdk missing,
3 open, 4 set source, 5 init, 6 preview, 7 no JPEG, 8 cannot write, 124 timeout.  Because
`MDK_IOControl` swallows `sendcommand` errors the only reliable failure signals are `MDK_Open/Init`
returning 0 and the absence of a JPEG.

## 7. Environment (exactly what walpad-camera sets up)

Verified earlier on the tablet for `/vendor/bin/factory`; `walpad-camera start` reproduces it:

| Item | How |
|---|---|
| bionic linker | `/system/system/bin/bootstrap/linker64`, started via a symlink `/run/walpad/android-bin/<name>` |
| properties | `LD_PRELOAD=/usr/local/lib/libwpdprops.so`; `/tmp/wpd-props` rebuilt from `/system/system/build.prop`, `/vendor/build.prop`, `/vendor/default.prop`, plus `init.svc.vndservicemanager=running`, `vendor.camera.acdkdump.enable=0`, `...acdkdump.raw=0`, `vendor.camera.acdk.debug=0` |
| libraries | `LD_LIBRARY_PATH=/system/system/lib64/bootstrap:/vendor/lib64:/run/walpad/apex/vndk/lib64:/system/system/lib64`; the VNDK apex payload is unzipped and loop-mounted at `/run/walpad/apex/vndk` exactly like `walpad-gps` |
| binder | `mount -t binder binder /dev/binderfs` (kernel has `CONFIG_ANDROID_BINDERFS=y`, devices `binder,hwbinder,vndbinder`), symlinks `/dev/{binder,hwbinder,vndbinder}`; `vndservicemanager /dev/vndbinder` through the symlink `android-bin/vndservicemanager` (from `/vendor/etc/init/vndservicemanager.rc`) |
| SELinux | `mount -t selinuxfs selinuxfs /sys/fs/selinux`; no policy is ever loaded |
| boot mode | `unshare -m` + `mount --bind /run/walpad/fake-atag-boot /sys/firmware/devicetree/base/chosen/atag,boot` (12 bytes: `0c 00 00 00 | 01 08 00 41 | 04 00 00 00` = size, tag, **mode 4**) around the helper only.  Needed by the factory; libacdk does not read it, so a failure here is not expected to matter (`WALPAD_CAM_FAKE_ATAG=0` skips it) |
| log | `socat -u UNIX-RECV:/dev/socket/logdw OPEN:/var/log/walpad-android.log,creat,append` when socat exists: read with `strings`; `ACDK_IF` / `AcdkMain` lines show every command |
| directories | `/data/vendor` (libacdk's output dir), `/dev/socket` |
| timeouts | `timeout -k 5 ${WALPAD_CAM_TIMEOUT:-120}` around the helper; the helper has its own alarms |
| one at a time | `/run/walpad/camera.lock` (pid; a dead pid is taken over) |

## 8. What is uncertain

- **Everything in this document is host-only evidence**; nothing has run on the device.
- `camacdk` has its own `_start` that calls `__libc_init` like bionic's `crtbegin` (copied from
  `factory 0x47000..0x47044`, `structors` arrays `[-1, 0]`), linked against stub `libc.so`/`libdl.so`
  (build.sh).  If the linker rejects it or libc misbehaves, that is the first suspect (symptom: crash
  before the first "camacdk: phase dlopen" line).
- `MDK_OpenBypass(4)` was never called by shipped code that we found; its meaning comes from the
  constructor only.  Fallback: `CAMACDK_BYPASS=0` uses `MDK_Open` (real overlay, writes fb0).
- `/sys/class/BOOT/BOOT/boot/boot_mode` may not exist; non-fatal in `AcdkMain::init`, but other
  libraries also read boot-mode files.
- NVRAM / calibration: the HAL3A and sensor-driver libraries read `/mnt/vendor/nvcfg/camera` and
  nvdata files; if those are absent, defaults may be used (3A tuning poor, still works) or `MDK_Init`
  may fail.  nvdata holds the IMEI: never mount it read-write for this test (backups in
  `firmware/MT6765/backups/modem-20261110/`).
- Capture callback layout for width/height (M), the meaning of capture-struct offsets 0/4 (M), the
  exact encoded length (we trim at EOI).
- Preview frames: see section 5, may produce an empty or wrong-sized dump.  `camacdk` stops after the
  timeout with exit 6 and logs why.
- `/dev/ion`, `/dev/camera-isp`, `/dev/kd_camera_hw` must exist (`walpad-camera status` lists them);
  without udev they may need `mknod` from `/sys/class/...` (not automated here).
- Sensor names (`gc5035, gc8034, gc08a3, gc2385, sc200cs` from the kernel config) are not resolved
  here: `camacdk list` prints `/proc/driver/camera_info` and probes main and sub.

## 9. Device test plan

Order: read-only first, then the environment, then the helper.  Nothing flashes or writes a
partition.  Use the `walpad` SSH alias (Wi-Fi or USB ethernet); never run two sessions at once.
The image must contain `walpad-camera` and `camacdk`: either rebuild the rootfs, or push the two files
without a flash:

```
# on the host, in the repo
bash tools/camacdk/build.sh
scp config/rootfs/usr/local/sbin/walpad-camera walpad:/usr/local/sbin/ && scp data/camera/camacdk walpad:/usr/local/bin/
ssh walpad 'chmod 755 /usr/local/sbin/walpad-camera /usr/local/bin/camacdk; ls -l /usr/local/lib/libwpdprops.so /system/system/bin/bootstrap/linker64 /vendor/lib64/libacdk.so'
```

(`/usr/local/lib/libwpdprops.so` already exists on the tablet from the factory test; copy it from
`data/wifi/libwpdprops.so` otherwise.  The rootfs is ext4 on `userdata`: the pushed files survive reboots, a re-flash removes them.)

1. **Read-only probe**

   ```
   ssh walpad 'cat /proc/driver/camera_info; ls -l /dev/camera-isp /dev/kd_camera_hw /dev/ion /dev/mtk_jpeg /proc/mtk_jpeg /proc/m4u 2>&1; cat /sys/class/BOOT/BOOT/boot/boot_mode; dmesg | grep -i -E "imgsensor|camera|seninf|ccu" | tail -20'
   ssh walpad 'walpad-camera status'
   ```
   Look for: sensor names in `camera_info`; `/dev/camera-isp` and `/dev/kd_camera_hw` present.  Missing
   nodes: `cat /sys/class/misc/*/dev` and `mknod` by hand before going on, and note it.

2. **Display**: with the fake surface view the helper does not touch fb0, so nothing needs stopping.  Keep a second SSH session open anyway.

3. **Environment**

   ```
   ssh walpad 'walpad-camera start; echo rc=$?; walpad-camera status; tail -5 /var/log/walpad-camera.log'
   ```
   Look for `[camera] environment ready`, `vndservicemanager: running`, `vndbinder: present`.
   Failure `binderfs not available`: `ssh walpad 'grep binder /proc/filesystems; dmesg | grep -i binder'`.

4. **Library load and sensors, still no capture**

   ```
   ssh walpad 'WALPAD_CAM_TIMEOUT=90 walpad-camera list; echo rc=$?'
   ssh walpad 'strings /var/log/walpad-android.log | grep -E "ACDK_IF|AcdkMain|sensorInit" | tail -40'
   ```
   Look for `main: ok` and/or `sub: ok`, lines `sensorInit ... name:... type:...`, no `Sensor Not found`.
   `FAILED (exit 2)` = `dlopen` of libacdk failed: read the line above it (missing library,
   namespace error).  `exit 5` = `MDK_Init` failed: the Android log names the step.  `exit 124` = a hang:
   the `TIMEOUT in phase` line says where.

5. **One still** (main camera, in light)

   ```
   ssh walpad 'WALPAD_CAM_TIMEOUT=120 walpad-camera capture main /root/x.jpg; echo rc=$?; ls -l /root/x.jpg /data/vendor/acdkCap.jpg'
   ssh walpad 'cat /root/x.jpg' > x.jpg && file x.jpg
   ```
   Look for `camacdk: capture callback: type=0x40 ... buffer=N` then `OK /root/x.jpg N bytes`.
   If the callback never fires but `/data/vendor/acdkCap.jpg` exists, the helper uses it (log line
   "using /data/vendor/acdkCap.jpg").  Open `x.jpg` on the host: a real photo (sensor black or green
   means the sensor streams but 3A/ISP calibration is missing: raise `CAMACDK_SETTLE_MS=6000`).

6. **Sub camera**: `ssh walpad 'walpad-camera capture sub /root/y.jpg'`, copy back the same way.

7. **Preview frames** (least certain): `ssh walpad 'walpad-camera preview main /root/prv 5; ls -l /root/prv'`,
   then `ssh walpad 'cat /root/prv/prv000.ppm' > prv000.ppm`.  Exit 6 "no preview frame appeared": see
   section 5 and send `/var/log/walpad-android.log` strings around `handlePreviewCB`.

8. **If real-overlay behaviour is needed** (only for debugging the bypass): `CAMACDK_BYPASS=0 walpad-camera
   capture main /root/z.jpg` uses `MDK_Open`, writes fb0 (stop `fbmirror` first, restart the session after).

9. **Clean up**: `ssh walpad 'walpad-camera stop'`.  The mounts stay until reboot.

What to send back if a step fails: the command, its output, `tail -50 /var/log/walpad-camera.log`,
`strings /var/log/walpad-android.log | tail -80`, and `dmesg | tail -40`.
