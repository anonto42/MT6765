# Flash runbook: first WalPad OS image with the Rust components

Everything here is for **you** to run. Nothing in this document has been executed on the tablet, and
no step flashes anything until you start `scripts/flash_os.sh` yourself (AGENTS.md: never flash or
erase a partition without explicit owner approval).

## 0. What this image is (and is not)

| | |
|---|---|
| Boot image (`boot_a`) | **Unchanged.** `build/android-restore/boot_a-os.img` is what is already flashed (`config/boot/*` was not touched). |
| Root filesystem (`userdata`) | **New:** `build/rootfs-candidate/rootfs.ext4` (1.5 GiB, ext4, clean `e2fsck`). |
| Default runtime | **Still Python.** `/etc/walpad/runtime.conf` = `WALPAD_DEVICE/INPUT/SHELL=python`, `WALPAD_SHADOW=0`. A fresh boot behaves like the image you flashed on 2026-11-08, plus the additions below. |
| New but switched off | Rust `walpad-core`, `walpad-shell`, `walpad-ctl-rs` (installed, not started), safe mode, log caps, new `fbmirror` (pauses while the screen is off), `walpad-runtime`/`walpad-supervise` fallbacks. |
| Python-free image | Separate, opt-in (`WALPAD_NO_PYTHON=1 scripts/build_rootfs.sh`). **Do not flash it first.** |

Hashes at the time of writing (first 16 hex digits; full values in `build/rootfs-candidate/image.sha256`):

* candidate rootfs `5f84923c3b765001`
* last flashed rootfs `build/rootfs/rootfs.ext4` `2501122a42db339e` (your rollback copy: keep it, never overwrite it)
* boot image `build/android-restore/boot_a-os.img` `9fe2919b40ee50ac`

## 1. Before you flash (host)

1. `git status` is clean and `scripts/check.sh` prints `ALL CHECKS PASSED`.
2. Backups exist (they were made earlier; just confirm they are still there):
   `firmware/MT6765/backups/` and `build/android-restore/super-sparse-as-found.img` (4 GiB).
3. **Wi-Fi.** A new rootfs wipes `userdata`, and no network is baked into the image. Without Wi-Fi you
   cannot SSH in (the USB-network host side does not transmit), and the only way to join a network would be
   typing the passphrase on the tablet. Put your network into the image first, without rebuilding:

   ```
   printf '%s\n' 'YOUR-WIFI-PASSPHRASE' | scripts/image_set_wifi.sh build/rootfs-candidate/rootfs.ext4 'YourSSID'
   ```

   It reads the passphrase from stdin only, stores just the derived WPA2 key (mode 0600), refuses to touch
   `build/rootfs/rootfs.ext4`, and checks the filesystem afterwards. Run it again to replace the network.
   Your SSH key (`~/.ssh/id_ed25519.pub`) was already baked in when the image was built.
4. Optional but recommended: `sha256sum build/rootfs-candidate/rootfs.ext4` and note it (the patch in step 3 changes it).
5. Charge the tablet. Keep the USB cable at hand: the flash needs **one unplug/replug with no buttons held**.

## 2. Flash (you run this)

```
scripts/flash_os.sh build/rootfs-candidate/rootfs.ext4 build/android-restore/boot_a-os.img
```

Start it **first**, then unplug and replug the USB cable (no buttons). Watch `build/flash/os.result`; the
expected sequence is:

```
USERDATA_WRITTEN
USERDATA_VERIFIED       <- read-back compared with the image (do not continue if this says MISMATCH or FAILED)
BOOT_WRITTEN
BOOT_VERIFIED
RESET_RC=0
DONE
```

The boot image is re-written with the same bytes it already has; that is harmless and keeps the tool simple.
If any line other than these appears, stop and send me `build/flash/os.result` and the `build/flash/os.*.log` files.

## 3. First boot

The ramdisk waits up to 150 s for `/run/walpad-ok`. With the default (Python) runtime the Tk launcher writes it,
so expect the home screen within about 1-2 minutes. Find the tablet:

```
make wifi-find            # or: bash scripts/wifi_find.sh --ip-only
ssh root@<ip> sh -s < scripts/device/smoke.sh
```

`smoke.sh` must print only `PASS` lines (it also lists the manual checks: rotation flicker and direction, a
web page in Firefox, typing in the terminal).

The baseline (M0.3, `scripts/device/baseline.sh`) and fixture recorder (M0.4, `scripts/device/record_fixtures.sh`)
**have not been written yet**; they are not needed to boot or to run steps A-F. Skip them for the first flash.
Screenshots (`walpad-ctl-rs screenshot`) are saved as `.xwd` files in `/root/screenshots/` on the tablet.

**If the screen stays dark or no Wi-Fi after 3 minutes:** go to section 6 (rollback) and send me what you saw.

## 4. Turn the Rust parts on, one at a time

Edit `/etc/walpad/runtime.conf` over SSH, **reboot** (init reads it at boot), run the check, and only continue
when it passes. Every step is reversible by setting the line back to `python`.

| Step | Change | Check | What you should see |
|---|---|---|---|
| A | `WALPAD_SHADOW=1` | `walpad-core` logs `WOULD ...` lines to `/var/log/walpad-core.log`; do the scripted session (rotate 4 ways, idle until dim/off, wake by touch and key, every volume gesture, cover, power short/long), then on the host `scripts/device/shadow_diff.sh PYTHON_LOGS... --core CORE_LOG` | `CLEAN`, or every divergence explained (M2.4) |
| B | `WALPAD_DEVICE=rust` | `ssh root@<ip> sh -s < scripts/device/m3_check.sh` | all `PASS`, then its manual list (dim/off/wake, power key, rotation, cover, battery) |
| C | `WALPAD_INPUT=rust` (needs B) | `m3_check.sh` again + manual list (touch accuracy, volume gestures, long power opens the menu, quad-tap opens the browser) | all `PASS` |
| D | `WALPAD_SHELL=rust` | Home appears in the new theme; walk the UI checklist in `docs/rust/UI.md` (all five tabs, Wi-Fi scan and join by touch, brightness, rotation, power menu with two taps, dock over Firefox) | M5.9 checklist ticked |
| E | Wi-Fi from the shell | `printf '%s\n' 'PASS' | ssh root@<ip> sh -s -- 'SSID' < scripts/device/m4_check.sh` (joins and forgets a test network) then a **cold boot** and `smoke.sh` | existing networks still auto-connect |
| F | Apps by touch | `ssh root@<ip> sh -s < scripts/device/m6_check.sh` + its touch-only list (open a site, type in the terminal, return Home, `pkill -9 firefox-esr` recovers) | all `PASS` |

Measurements for the display and memory cards (no numbers exist yet, so none are claimed):

```
ssh root@<ip> sh -s < scripts/device/measure_fbmirror.sh      # M8.2: CPU with the screen on / off
ssh root@<ip> sh -s < scripts/device/measure_firefox_mem.sh   # M8.3 (Firefox closed first)
```

Hardening checks (M9): safe mode (power off, hold Volume Down, press power, keep holding until the rescue UI
appears, release; SSH should still work), `WALPAD_LOG_MAX=1000 WALPAD_LOG_KEEP=300 walpad-logcap` shrinks logs,
`walpad-ctl-rs key-held volumedown; echo $?` prints `1` untouched and `0` while held. Then M9.4 (listening ports,
key-only SSH, secret file modes) and M9.5 (5 cold boots, Wi-Fi loss, app/shell/core crash).

## 5. What to send back

Paste, per step: the command output (`smoke.sh`, `m3_check.sh`, ... print one PASS/FAIL line per check), and for
anything visual a short description or a photo of the screen.
Never paste your Wi-Fi passphrase; logs are redacted but check before sharing.

## 6. Rollback (cheapest first)

1. **Runtime switch:** set the line you changed back to `python` in `/etc/walpad/runtime.conf`, reboot. The supervisor also
   falls back to Python by itself after three quick crashes of the Rust core or shell.
2. **Safe mode:** hold Volume Down while powering on until the rescue UI (`padui`) shows; SSH and Wi-Fi still start.
3. **Boot ramdisk fallback:** if the session never reports healthy, the ramdisk starts `padui` after 150 s on its own.
4. **Reflash the previous image:** `scripts/flash_os.sh build/rootfs/rootfs.ext4 build/android-restore/boot_a-os.img`
   (same procedure as section 2). Check `sha256sum build/rootfs/rootfs.ext4` starts with `2501122a42db339e` first.
5. **Last resort:** `build/android-restore/super-sparse-as-found.img` and `firmware/MT6765/backups/` (see `docs/99-status-log.md`).

## 7. Known risks and what is unverified

* **Nothing Rust has run on the tablet.** The host tests use fake roots, recorded-style event streams, a real X server and
  fake `wpa_supplicant`; the real kernel, touch controller, accelerometer, MediaTek Wi-Fi stack and openbox are untested.
* Unverified details: the `gsensor` ioctl, `EVIOCGKEY` (safe mode), `mknod` of input nodes, Firefox/`st` WM_CLASS and
  `_NET_WM_PID` under openbox, `st` exit codes (a false "closed unexpectedly" may need `"exit_ok":[1]` in `apps.json`),
  the new `fbmirror` (its `stat` offset and `clock_gettime` syscall number; if either is wrong it just keeps copying).
* Wi-Fi from a cold boot was already unverified before this work (docs/20-real-device-plan.md).
* Touch calibration is still inaccurate, the battery gauge reads -1 (a voltage estimate is shown and labelled), audio,
  Bluetooth, SD card and USB OTG are not working or not attempted, and the camera is unsupported.
* Telnet on the USB addresses is root without a password (Settings > Security shows its scope).
* The `.ext4` image is not byte-for-byte reproducible (package list and config hashes are; see `docs/rust/reports/M9.1-M9.3.md`).
