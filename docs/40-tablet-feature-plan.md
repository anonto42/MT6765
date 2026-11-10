# WalPad OS: tablet feature plan (2026-11-10)

Status: research and planning only. No source, device or flash was touched. Requested by the owner
(2026-11-10): make WalPad a complete tablet and a general-purpose Linux machine, including a
security-testing mode for the owner's OWN equipment (labs, CTFs, authorised tests only).

## Table of contents

1. Scope, constraints and how to read this plan
2. What the reference platforms provide (survey)
3. What WalPad has today
4. Feature groups (what, why, today, effort, limits, implementation, acceptance test)
   - 4.1 Input and editing
   - 4.2 On-screen keyboard
   - 4.3 Gestures, navigation, windows
   - 4.4 Notifications
   - 4.5 Lock screen, security, multi-user
   - 4.6 Settings breadth (display, sound, network, Bluetooth, storage, battery, apps, accessibility, time, updates, backup)
   - 4.7 Media
   - 4.8 Connectivity and sharing
   - 4.9 Productivity
   - 4.10 Phone features (SMS, calls, data)
   - 4.11 System services
   - 4.12 Developer features
   - 4.13 Automation
   - 4.14 Security features of the OS itself
5. Security-testing mode (Kali chroot)
6. Launcher and home screen
7. Default app catalogue (Alpine packages and sizes)
8. Phases
9. OpenSpec changes created
10. Open-source projects worth reusing
11. Sources

## 1. Scope, constraints, reading guide

Hard facts that shape every estimate (from `data/kernel/walpad-4.19.127.config`, `hardware.md`,
`known-issues.md`, `docs/rust/*`):

| Limit | Consequence |
|---|---|
| Stock kernel 4.19.127, MODVERSIONS=y, MODULES=y, no module signing | New drivers must be out-of-tree `.ko` built for exactly this kernel (as `hci_vhci.ko` was). |
| `CONFIG_USER_NS` not set; no SYSVIPC; no CGROUP_DEVICE/PIDS | No Docker/Podman/Flatpak/bubblewrap sandboxes, no unprivileged containers. Sandboxing = chroot + separate uid + seccomp (`SECCOMP_FILTER=y`) + optional net-namespace (NAMESPACES=y, `unshare -n` works as root). |
| No GPU user space (Xvfb + software Mesa, fbmirror copies the framebuffer) | No Wayland compositor with GL, no smooth video scaling, no WebGL; 2D UI via tiny-skia only. Heavy apps (LibreOffice, Thunderbird) run but slowly. |
| Display 800x1280 panel, X session 1280x1280, 4 GB RAM, ~50 GB userdata | RAM is the budget: prefer small GTK2/GTK3/Rust apps, ZRAM is built (`ZRAM=y`) and should be enabled. |
| Legacy MTK audio driver: only 44.1 kHz, 25 ms/100 ms period/buffer safe; other settings hang the tablet | All audio must pass through one ALSA `dmix`/`softvol` plug at 44.1 kHz (existing `asound.conf`); PipeWire/PulseAudio only if pinned to that config (risk: M). Never `speaker-test` raw. |
| Wi-Fi chip (CONNSYS gen4m) has no monitor mode, no AP+STA together | Monitor/injection only via a USB Wi-Fi adapter + out-of-tree driver. Hotspot pauses the client (already so). |
| Boot image/ramdisk frozen (`config/boot/*`), root is a chroot of userdata | Full-disk encryption at boot is not possible without a boot image change (owner approval); encrypted data containers are possible (dm-crypt + XTS built in). |
| No `BINFMT_MISC`, no `NF_TABLES` (iptables legacy only), no WireGuard module, no `FS_ENCRYPTION` | VPN WireGuard = `wireguard-go` (userspace over `TUN=y`); firewall UI drives legacy iptables; fscrypt not available. |
| USB OTG host via musb, ID-pin role switching; gadget configfs has HID, ACM, ECM, NCM, RNDIS, mass storage, FunctionFS, UVC, MIDI (`USB_CONFIGFS_F_HID=y`); no UAC, no EEM, no printer, no MTP function (FunctionFS can carry a userspace MTP responder) | USB gadget modes are feasible in principle, but the boot ramdisk already binds a gadget to the single UDC (recovery network): switching means unbind/rebind at runtime. Host and gadget roles are exclusive (one port). |
| No `USB_SERIAL`, no `MEDIA_USB_SUPPORT` (no uvcvideo), no `CONFIG_BT_HCIBTUSB/RFCOMM/BNEP/HIDP` | USB serial adapters (FTDI, CH340, CP210x), webcams and USB Bluetooth need out-of-tree modules; ACM works. RFCOMM/HIDP are needed for Bluetooth keyboards (HIDP) and serial: also modules to build. |
| Vendor modem: AT over `/dev/radio/pttycmd1`, data link (ccmni) not flowing yet; GPS mnld has no positions yet; camera only through the Android HAL or a USB webcam module | SMS and signal feasible now; calls need audio routing (not available); data and location depend on open bring-up items. |

Effort scale: S under 1 day, M 1 to 3 days, L over 3 days of agent work (host checks included).
Every feature uses apps launched by id, no secrets in logs/argv (see `openspec/config.yaml`).

## 2. What the reference platforms provide (survey)

| Platform | Notable features a user expects | Takeaway for WalPad |
|---|---|---|
| Android tablet | Clipboard + long-press selection handles, gboard (languages, emoji, voice, prediction), back/home/recents, split screen, notification shade with heads-up/DND/per-app channels, lock screen (PIN/pattern/password, auto-lock, screen pinning), multi-user, huge Settings (display/sound/network/VPN/storage/battery/apps/permissions/accessibility), Files, Play store, camera/gallery, backups, OTA with rollback (A/B), screen recording, casting | The expectation baseline. WalPad should copy behaviours, not architecture. |
| iPadOS | Slide Over/Split View/Stage Manager, Control Centre, Focus modes (DND), Files, drag and drop, Handwriting/Scribble, Notes, Screen Time, Find My | Quick Settings, Focus modes, drag and drop, and a simple Files app are the visible wins. |
| Ubuntu Touch | Lomiri shell, edge-swipe app switcher, indicators, Click packages, convergence, ofono/RIL telephony | Telephony stack idea (ofono), indicator model, confinement for apps. |
| postmarketOS with Phosh / Plasma Mobile | Mainline-ish Linux phones, Phosh: lockscreen, gestures, quick settings, notification list, on-screen keyboard (squeekboard), Wayland, ModemManager/Callaudio/Chatty/Calls | Reuse ideas and small apps: squeekboard layouts, Chatty (SMS), Calls, GNOME Contacts/Calendar/Clocks/Weather/Maps ("Linux mobile" app set). Phosh needs Wayland + GPU-less path (Cage/phoc work with pixman) but would replace the whole shell: rejected, we keep the Rust shell. |
| Sailfish OS | Gesture-first navigation (edge swipes), ambiences, Android app support, pulley menus | Gestures and "ambience" (theme per wallpaper) are good polish ideas. |
| ChromeOS | Launcher with search, Files with Drive/SMB, Linux container (Crostini), desktop-style windows, Phone Hub, Quick Settings | Launcher search, Files with network shares, and a Linux environment are exactly our dual use. |
| Kali NetHunter | Android + Kali chroot (`kali-linux-nethunter` tool set), chroot manager, KeX (VNC desktop), HID keyboard attacks via kernel gadget, OTG wireless with patched kernels, Bluetooth/SDR tools | Our Kali mode = chroot manager + KeX-like X11 integration (we already have X), kernel limits decide HID and wireless. |

## 3. What WalPad has today (code read 2026-11-10)

- Shell (`crates/walpad-shell`, ~11k lines, tiny-skia): sidebar (Home, four pinned apps, Settings, Apps chevron),
  header pill (signal, Wi-Fi, battery), Quick Settings (Wi-Fi, hotspot, mobile data, Bluetooth row disabled,
  battery, brightness, rotation, screen off), Apps panel (active apps with close badges, TOOLS grid with
  only "Mouse pad"), Home grid (5 columns portrait), Settings (Display, Input, Power, Network, Mobile network,
  Security, System, Clock, Maintenance, About, Logs), Device screen, Wi-Fi/hotspot screen, Mobile screen,
  system keyboard (single QWERTY layout, XIM-driven, one-shot Shift/Ctrl), mouse pad, collapsible dock
  (Home/Keyboard/Close app), power menu, toasts and banners.
- Core (`crates/walpad-core`): IPC control socket, status.json, Wi-Fi/hotspot, modem (AT), storage, audio,
  input and key mapping (keys.json), app launcher (apps.json: browser, terminal st, files PCManFM, wifi).
- Scripts: `walpad-storage` (SD/USB, exFAT/NTFS via FUSE), `walpad-audio`, `walpad-gps` (mnld),
  `walpad-bluetooth` + `walpad-btbridge` (hci_vhci, not yet verified on the device), `walpad-modem`,
  `walpad-kali` (a first Kali chroot prototype using the NetHunter minimal rootfs; see section 5).
- Missing entirely: clipboard manager, notifications, lock screen, app switcher/recents, multi-window, Bangla/other
  layouts, emoji, wallpaper chooser, volume UI, VPN, package manager UI, media/PDF/gallery/camera apps,
  alarms, calendar, SMS UI, firewall UI, updates, backups.

## 4. Feature groups

Priority: P1 daily-driver, P2 media/connectivity, P3 security mode, P4 polish. "Change" names the OpenSpec change
that implements it (section 9); "(none)" means under a day or a package install.

### 4.1 Input and editing

| Feature | What / why | Today | Eff. | Limits | Implementation | Acceptance test |
|---|---|---|---|---|---|---|
| Copy/paste + clipboard history (P1, `clipboard-selection`) | System clipboard across apps; history of the last N items, pin, clear. Every tablet has it; X11 selections vanish when the source app closes. | Nothing (xclip present). | M | X11 only; PRIMARY vs CLIPBOARD; no Wayland. | Shell becomes the clipboard manager: XFIXES selection-owner watch, takes ownership and serves UTF8_STRING/TARGETS (text only first), ring of 25 in `/run/walpad/clipboard` (RAM, never disk, never logged). Small history sheet on the keyboard (clip key) and Quick Settings tile. | Copy text in Firefox, close Firefox, paste in `st`: text appears. History shows 3 entries; clear empties it; password-field copies are not recorded when the app sets `x-kde-passwordManagerHint`-style hints (best effort). |
| Text selection and long-press menu (P1, same change) | Touch selection handles and Cut/Copy/Paste/Select all popup. Without it a touch-only tablet cannot edit. | Long-press only on app icons. | M | GTK/Firefox draw their own handles badly without touch events: our touch arrives as pointer events (touchd, protocol A). | Shell long-press detector on the touch stream (already `gesture.rs`) injects a right-click equivalent; popup overlay (Cut/Copy/Paste/Select all) sends Ctrl+X/C/V/A with XTEST. Firefox touch handling (`MOZ_USE_XINPUT2=1`) is a research item. | Long press in a text field opens the popup within 700 ms; Select all + Copy then Paste elsewhere works in Firefox, st, PCManFM. |
| Drag and drop (P4, none) | Move files/text between windows. | Works inside PCManFM with pointer drag only. | M | Needs XDND-compatible apps, finger drag = pointer drag so basically works; long-press-to-drag threshold missing. | After long-press on a file/icon allow drag with a visual lift; implement in shell gesture layer for launcher icons first (reorder); XDND between apps is app-side. | Drag a file from PCManFM into a folder with finger; drag an app icon in the Home grid to reorder. |

### 4.2 On-screen keyboard

Today: one QWERTY layout, symbols layer, XIM-driven show/hide, no Bangla, no emoji, no suggestions.

| Feature | What / why | Eff. | Limits | Implementation | Acceptance test |
|---|---|---|---|---|---|
| Layouts, language switch, Bangla (P1, `keyboard-languages-bangla`) | Bangla is the owner's language (Walton, Bangladesh). Needs layouts (QWERTY, Bangla fixed/phonetic), globe key, per-field remembered language. | L | Keys are sent through XTEST keycodes; Bangla characters need Unicode commit, not keysyms. The shell already is the XIM server, so commit text through XIM `XIM_COMMIT` (XIMPreeditNothing only today; add commit of strings). Fonts: `font-noto-bengali` (13 MB). | Layout table in a data file (`/etc/walpad/keyboards/*.json`: name, rows, keys with label, commit string, shift variant). Ship `en-us`, `bn-phonetic` (Avro style) and `bn-inscript`. Phonetic engine: reuse the Rust crate `riti` (OpenBangla Keyboard, MPL-2.0) or port the Avro rule set (table driven, about 1000 lines). Globe key cycles, long-press globe shows a language list. | With Bangla active, typing `ami` in Firefox's search box commits আমি; switching back gives Latin; fonts render in Firefox and st. |
| Emoji and symbols panel (P1, same change) | Emoji picker, recent row. | S-M | `font-noto-emoji` is in the image (colour emoji needs `font-noto-emoji` COLR/bitmap: Firefox ok, st mono only). | Emoji layer from a bundled Unicode list (CLDR short names for search), commit text via XIM. | Emoji key shows a grid; tapping inserts the character in Firefox. |
| Key popups, long-press accents, key repeat, haptics, sound (P4) | Polish. | S | No vibrator driver known (check `/sys/class/timed_output`); click sound via the speaker only. | Long-press variants in the layout JSON; optional click through `aplay` of a short wav (risk with the audio quirk: only through the 44.1 kHz dmix). | Hold `e` shows é; key repeat on Backspace. |
| Predictive text / autocorrect (P4, later change) | Word suggestions strip. | M-L | RAM small, fine. | Pure Rust: bundled frequency list (wordfreq, English + Bangla) + prefix trie + simple bigram; suggestions strip above keys; `hunspell` (0.5 MB + dicts) only for spell-check. No cloud. | Typing "teh" offers "the"; accepting inserts it. |
| Voice typing (optional, P4) | Dictation. | L | No microphone path known on the legacy audio driver; offline STT (Vosk/whisper.cpp small) heavy on 4 GB CPU. | Defer; investigate mic capture (`arecord`) after speaker work, then `vosk-api`. | Out of plan until mic capture is proven. |
| Hardware keyboards (Bluetooth/USB) (P2, none) | Keyboard dock. | S | USB HID works (`HID_GENERIC`); Bluetooth HID needs the HIDP module. | `walpad-bluetooth` + `hidp.ko`. | Plug a USB keyboard; keys reach st. |

### 4.3 Gestures, navigation, windows

| Feature | What / why | Today | Eff. | Limits | Implementation | Acceptance test |
|---|---|---|---|---|---|---|
| Back/Home/Recents and app switcher (P1, `app-switcher-navigation`) | Android users expect Back, Home, Recents. Today the dock has Home/Keyboard/Close; the Apps panel has "active apps". | Partial. | M | No compositor: no live thumbnails unless `XGetImage`/`xcb` capture per window (software, fine for 6 apps). Back is app-defined: Alt+Left in browser/files, Esc elsewhere. | Edge-swipe from the bottom (gesture.rs): up = Home, up-and-hold = Recents card strip with thumbnails (XCompositeNameWindowPixmap not available: use `XGetImage` of the mapped window, cached on blur); swipe card = close. Back = sends Alt+Left, else Esc, else `WM_DELETE_WINDOW` fallback for secondary windows. Dock Back button. | Open 3 apps, swipe up and hold: three cards, tap one focuses it, swipe a card away closes it, memory drops. |
| Split view / floating windows (P4, none) | Two apps side by side. | Not possible: openbox undecorated and maximised. | M | Apps assume maximised; small RAM. | openbox rc keybindings via `_NET_MOVERESIZE_WINDOW`: shell tells core to tile two windows (50/50 or 1/3 2/3), floating mode with a drag handle; setting per-app "allow split". | Choose "Split" on a card: browser top, terminal bottom; rotate: stays tiled. |
| Rotation lock, rotation, scaling (P1, partly there) | Auto/lock exists in Quick Settings. | Done; direction unverified. | S | Accelerometer direction unverified. | Verify on device (separate task) and expose flip in Settings > Display. | Rotate: UI follows; lock holds. |
| Screen pinning / kiosk (P4, none) | Lock the tablet to one app. | None | S | Needs lock screen first. | Core refuses `home` while pinned; unpin by holding Back+Recents (or power long press + PIN). | Pin Firefox, Home does nothing until PIN. |

### 4.4 Notifications

| Feature | What / why | Today | Eff. | Limits | Implementation | Acceptance test |
|---|---|---|---|---|---|---|
| Notification centre, heads-up, DND, per-app (P1, `notification-centre`) | Apps (Firefox, Thunderbird, our alarms, downloads, battery) need to notify. | Toasts only inside the shell. | L | Needs a D-Bus session bus (`dbus` is in the image; start `dbus-daemon --session` in walpad-session). | Shell implements `org.freedesktop.Notifications` (pure Rust, crate `zbus`, no libdbus) with `Notify`, `CloseNotification`, `GetCapabilities` (body, actions, icon-static), `NotificationClosed`/`ActionInvoked`. Heads-up banner 5 s; shade = pull-down from the header (existing pill extended); grouping per app; DND flag in `/var/lib/walpad/dnd`; per-app mute list; urgency=critical bypasses DND. Core and scripts use `walpad-ctl notify` which calls the same interface (no extra daemon: `dunst` is only a fallback). | `notify-send "Hi" "body"` shows a heads-up, lands in the shade, tapping it clears it; DND silences banners but keeps entries. |

### 4.5 Lock screen, security, multi-user

| Feature | What / why | Today | Eff. | Limits | Implementation | Acceptance test |
|---|---|---|---|---|---|---|
| Lock screen: PIN/password, pattern, auto-lock (P1, `lock-screen`) | A tablet that leaves the house needs one. | None; `Security` settings page shows exposure (telnet). | M-L | X11 has no secure lock without a compositor: a fullscreen override-redirect window with `XGrabKeyboard/XGrabPointer`, `WM_S0` not needed. A determined local attacker with USB ethernet/telnet (recovery on USB 192.168.42.1, unauthenticated telnetd, frozen boot) defeats it: say so in the UI. | Core stores PBKDF2-HMAC-SHA256 (or argon2 crate) hash + salt in `/var/lib/walpad/lock.json` (0600), rate-limits attempts (5 tries then 30 s back-off, doubling); shell draws the lock: clock, notifications summary, PIN pad (56 px keys) and password via the system keyboard; pattern 3x3 optional; auto-lock after screen-off N seconds (hooks `devd.json` `off_after_s`); power short press locks. Emergency/Wi-Fi quick toggles stay. | Set PIN 1234, screen off, wake: lock shown; wrong PIN delays; right PIN returns to the previous app; reboot comes up locked. |
| Encryption (P4, feasibility only) | Protect data at rest. | None. | L | Frozen ramdisk mounts userdata plainly; EXT4/F2FS fscrypt not in kernel; dm-crypt + XTS + AES-ARM64-CE are built in. | Realistic: an encrypted loop container (cryptsetup LUKS2 on `/var/lib/walpad/vault.img`) holding `/root` user data and the Kali chroot, opened at unlock with the PIN-derived key (cryptsetup/`lvm2` 0.4 + 2.2 MB). True full-disk encryption needs a boot image change (owner). | Lock then unlock: vault mounts only after PIN; `cat` of the raw image shows no plaintext. |
| Multi-user (P4, not planned) | Several profiles. | None | L | No user namespaces, one X session, one app tree; real separation means uid per profile and separate `$HOME`. | Later: uid per profile, `su`-style session switch restarting the shell; low value for a single owner. Document as not planned. | n/a |

### 4.6 Settings breadth

Existing pages: Display, Input, Power, Network, Mobile, Security, System, Clock, Maintenance, About, Logs.
Proposed top-level order after this plan: Network and internet (Wi-Fi, Mobile, Hotspot, VPN, DNS/Proxy, Bluetooth), Display, Sound,
Notifications, Apps, Storage, Battery, Security and privacy, Accessibility, Language and input, Date and time, System (updates, backup, developer), About.

| Feature | What / today | Eff. | Limits | Implementation | Acceptance test |
|---|---|---|---|---|---|
| Display: brightness (done), auto-brightness, night light, font size, UI scale, wallpaper chooser, themes/dark (P1, `display-sound-settings`) | Brightness slider and rotation exist; one fixed charcoal theme and a single owner wallpaper. | M | No ambient light sensor known (only accel `kxd`): auto-brightness = time-of-day schedule; night light = software gamma via `xrandr --gamma`/`xcalib`-style (Xvfb has no gamma: instead tint overlay or fbmirror gamma LUT: needs C change in tools/fbmirror, so use a shell overlay tint with a multiply-like alpha, low cost). UI scale: tokens in walpad-ui scale factor (text size S/M/L = 0.9/1/1.2). Themes: dark (current) and light token set; wallpaper chooser from `/usr/share/walpad/wallpapers` + `~/Pictures`. | Choose "Large" text: Settings rows grow, no clipping; pick a wallpaper: Home/Quick Settings refresh; night light on at 22:00 shows warm tint. |
| Sound: volume, ringtones, per-app, mute (P1, same change) | Volume keys move focus in menus; `walpad-audio` + amixer. | M | The audio quirk (section 1). Per-app volume needs PulseAudio/PipeWire: avoid, offer master/media/alarm/notification via `softvol` plug controls in asound.conf. | Sliders for Media, Alarm, Notification (three softvol PCMs, all feeding one 44.1 kHz dmix); notification sounds are short wavs played through `aplay -D notify`. Silent/vibrate not possible (no vibrator): Silent / Normal / DND. | Volume rocker changes media volume and shows a volume bubble; notification tone plays at its own level; playing two sounds at once does not hang the tablet (tested with a 60 s soak). |
| Network: airplane (done), VPN, proxy, DNS (P1, `vpn-and-dns`) | Wi-Fi, hotspot, mobile exist. No VPN, proxy, private DNS. | M | No WireGuard module: `wireguard-go` (3.3 MB) works over `TUN=y`; OpenVPN 0.8 MB; IPsec via `strongswan` (XFRM present). Per-app VPN not possible (no netns per app; uid-based `ip rule` is). | `walpad-vpn` script (up/down/status/import) + shell screen: import `.conf`/`.ovpn` from Files, toggle, kill switch (iptables rule), DNS server override (`/etc/resolv.conf` writer with `dnscrypt-proxy`/`unbound` optional), system proxy (writes `http_proxy` env for launched apps and Firefox policies). | Import a WireGuard config, toggle on: `wg show` has a handshake and `curl ifconfig.me` shows the VPN IP; toggle off restores the route; kill switch blocks traffic when the tunnel drops. |
| Network: tethering (P2) | Wi-Fi hotspot exists. USB tethering needs the gadget (RNDIS/NCM in kernel) | S-M | One UDC shared with the recovery gadget; recovery link must stay. | Add `usb-tether` mode to the runtime gadget manager (`walpad-usbmode`, see 4.8). | Plug into a PC: PC gets an IP and internet from the tablet. |
| Bluetooth (P1-2, `bluetooth-ui`) | Quick Settings row disabled. BlueZ + `hci_vhci.ko` + btbridge exist, unverified. | M | Device test pending; needs `hidp`/`rfcomm`/`bnep` modules for keyboards, serial, PAN; audio A2DP needs BlueALSA (`bluez-alsa`) and respecting the audio quirk. | Settings page and Quick Settings tile over `bluetoothctl`/D-Bus `org.bluez` (zbus): scan, pair (passkey sheet), connect, forget, rename, file receive via `bluez-obexd` into `~/Downloads`. | Pair a phone, send a file to the tablet, see it in Files; pair a BT keyboard (after HIDP). |
| Storage manager (P2, `network-file-sharing`) | `walpad-storage` handles mounts. No UI beyond Files. | S | exFAT/NTFS via FUSE. | Settings > Storage: bars (internal, SD, USB), Eject/Format (confirm), "free up space" (cache, thumbnails, logs, Kali chroot size). | Eject a USB stick safely; internal bar matches `df`. |
| Battery stats and saver (P1/P2, within `display-sound-settings` follow-up or the Power page) | Battery % is a voltage estimate (gauge reads -1). | S-M | No fuel gauge: stats are estimates; `CPU_FREQ` governors available. | Saver mode: dim 30 s, off 60 s, cpufreq `powersave` governor, Wi-Fi power save, stop sync tasks; history chart from `status.json` samples (24 h ring). | Saver on: governor changes, brightness capped, header shows saver icon. |
| Apps: install/uninstall, defaults, permissions (P1, `app-catalogue-manager`) | Apps are `apps.json` entries. | M-L | No store, no sandbox: "permissions" are honest labels (network, files, camera) that map to groups and iptables owner rules only for Kali/sandboxed apps. Flatpak/Snap impossible (no user NS). | `walpad-pkg` wrapper over `apk` (install/remove/upgrade with progress), catalogue file `/etc/walpad/catalogue.json` (id, name, apk packages, desktop exec, icon, size), generated `apps.json` entries, "Default apps" (browser, files, PDF, video, music, image, terminal) as `/etc/walpad/defaults.json` that launchers and `xdg-open`/mimeapps use. APK sideload = `apk add --allow-untrusted` is refused; only the signed Alpine repo and our own repo (section 4.6 updates). | Install "VLC" from the catalogue: icon appears on Home; uninstall removes it and frees the space; set default PDF viewer then open a PDF from Files. |
| Accessibility (P4) | None. | M | No TalkBack equivalent without AT-SPI (GTK_A11Y=none for speed). | Magnifier (shell overlay using XGetImage of the screen region, 2x), high-contrast tokens, bold/large text (from the scale factor), touch-and-hold delay, mono audio; screen reader: `espeak-ng` (0.5 MB) + `orca` over AT-SPI as an experiment (heavy; needs `GTK_A11Y` re-enable). | Triple-tap with three fingers zooms 2x and pans; High contrast shows white-on-black. |
| Date/time/locale/timezone (P1 small, none) | NTP+RTC, Asia/Dhaka fixed, Clock page. | S | `tzdata` present. | Timezone list picker, auto timezone from GPS/mobile network only when those work; 12/24 h (done), first-day-of-week; locale = `bn_BD`/`en_US` for Firefox and UI via `LANG` for launched apps. | Switch timezone: header clock follows without reboot. |
| About/updates/OTA from our repo + rollback (P4) | Maintenance page, `scripts/deploy.sh` over SSH. | L | A/B slots exist in the partition table but the boot chain is frozen; rollback = two rootfs trees on userdata (`/system-a`, `/system-b`) chosen by `walpad-init` (needs an init change inside the rootfs, not the ramdisk, which only runs `/sbin/walpad-init`: the chroot target can be a symlink flipped by the updater). | Signed tarball (minisign/ed25519 `signify` is in Alpine) of the image delta, fetched from our own HTTPS repo; `apk` repo for packages with our own key; new tree prepared offline, health check (`/run/walpad-ok` within 150 s) flips the symlink back on failure (matches the existing health logic). | Update to a test build over Wi-Fi; reboot into it; a deliberately broken build rolls back automatically. |
| Backup/restore (P4) | Manual NV backup only. | M | Space on SD/USB. | `walpad-backup` (tar + zstd or `borg`/`restic` to SD/USB/SMB), selection: settings, `~/`, apk world file, NV partitions optionally. | Backup, wipe `~/Documents`, restore: files return with the same hashes. |

### 4.7 Media

| Feature | Notes | Eff. | Limits | Implementation | Acceptance test |
|---|---|---|---|---|---|
| Video player (P2, `media-apps-capture`) | Expect YouTube-style and local video. | M | No GPU, no HW decode user space: 720p H.264 software on 8 cores is plausible, 1080p marginal, HEVC/VP9 heavy; output is X11 + software scaling. | `mpv` (2.6 MB + libs ~40 MB) with `--vo=x11` or `--vo=xv` (no), `--hwdec=no`, `--framedrop`, `--profile=sw-fast`; touch overlay controls via mpv `input-ipc` from a small Rust UI or the `mpv` OSC `modernz`/`uosc` scripts; `yt-dlp` optional. | Play a 720p30 H.264 MP4 for 5 minutes with under 5 % dropped frames and sound in sync through the 44.1 kHz path. |
| Music player (P2) | Local library, playlists, background play. | S-M | Audio quirk: resample to 44.1 kHz. | `mpd` (1.4 MB) + a Rust UI client or `audacious` (1.2 MB + Qt/GTK); simplest: mpv `--no-video` in a minimal Rust "Music" app that browses `~/Music`. Background play = app keeps running; lock-screen controls via MPRIS. | Play MP3 and FLAC; lock screen shows controls; no underruns for 10 minutes. |
| Photo gallery / image viewer (P2) | | S | | `ristretto` (0.3 MB) or `imv` (0.1 MB) for viewing, `gthumb` (5.8 MB) heavier gallery; or a Rust grid app on `image` crate (touch pinch not available: buttons for zoom). Thumbnails cache in `~/.cache/thumbnails`. | Open 200-photo folder: thumbnails appear in under 20 s; tap opens, swipe changes. |
| Camera app (P2) | Built-in camera is in `openspec/changes/camera-bringup` (ACDK helper) or USB webcam via `uvcvideo.ko`. | M (after bringup) | Camera state is research. | Rust camera app per the existing change; fallback `ffmpeg -f v4l2` for a USB webcam module. | Per `camera-bringup`. |
| Screenshots and screen recording (P2, `media-apps-capture`) | Screenshot key action exists (`keys.json`). | S-M | Software capture only. | Screenshot: shell grabs the root (XGetImage) to `~/Pictures/Screenshots` + notification. Recording: `ffmpeg -f x11grab -framerate 15 -c:v libx264 -preset ultrafast` (0.6 MB + libs ~50 MB); CPU cost limits to 15 fps. Audio capture not available. | Power+Volume Down chord saves a PNG; the recording tile records 10 s and the MP4 plays. |
| PDF/document viewer (P2) | | S | | `zathura` + `zathura-pdf-mupdf` (0.6 MB) is keyboard-centric; `evince` (0.9 MB + libs, GTK3, scroll by pointer drag) is the first pass, `foliate` for e-books, `okular` is Qt and heavy. | Open a 100-page PDF: scroll smooth, zoom buttons, search text. |
| Office suite (P2/P4) | | S | RAM and software rendering; LibreOffice common is ~311 MB plus Writer 11 MB. | `abiword` (17.7 MB) and `gnumeric` (9.8 MB) as light defaults; `libreoffice-writer/calc` optional install from the catalogue; cloud docs via Firefox. | Open a .docx in Abiword and a .xlsx in Gnumeric. |

### 4.8 Connectivity and sharing

| Feature | Eff. | Limits | Implementation | Acceptance test |
|---|---|---|---|---|
| Samba/NFS client in Files (P2, `network-file-sharing`) | M | `CIFS`/`NFS` kernel clients: check config (CIFS built? verify `CONFIG_CIFS`; fallback FUSE `smbnetfs`/`gvfs-smb`). | PCManFM supports `smb://` via gvfs (`gvfs`, `gvfs-smb`, `samba-client`, `cifs-utils`, `nfs-utils`); a shell "Network places" screen to add servers; credentials in `/var/lib/walpad/secrets` 0600, never argv (use credentials files). | Mount a Samba share, copy a file both ways, unmount on network loss without hanging Files (timeouts). |
| Sharing from the tablet (P2, same change) | S | | `samba` server optional (profile "Share Downloads", off by default, LAN only, password required); `dropbear` SSH/SFTP already there (`openssh-sftp-server` 0.1 MB gives the missing sftp: known issue notes no scp). | A PC browses `\\walpad\Share` with a password. |
| USB mass storage/MTP gadget (P2/P4, research) | M | One UDC shared with the recovery gadget (frozen). Mass storage function exists in the kernel; MTP via FunctionFS needs `umtp-responder` (userspace). | `walpad-usbmode` (script) unbinds `UDC`, rebuilds the configfs gadget (keeping the recovery RNDIS/ECM function in a composite config so 192.168.42.1 still works), modes: `recovery` (default), `storage` (exports a chosen image), `mtp`, `tether`, `hid` (section 5). Always revert on a 60 s timer unless confirmed (rollback). | Mode Storage: PC mounts the SD image; mode back: recovery IP answers. Open question: does the UDC allow composite reconfiguration while the PC is attached. |
| Wi-Fi Direct (P4) | M | p2p0 exists; interface combination limits; supplicant supports P2P. | Low priority; use hotspot. | n/a |
| KDE-Connect-like (P4) | M | `kdeconnect` in Alpine is 71 MB (Qt/KF5 heavy). | Alternative: `kdeconnect-cli`-free: run `kdeconnectd` headless only for notification mirroring and file receive, or use Syncthing (24 MB) for folders. | Pair with a phone, receive a file. |
| Bluetooth file transfer (P2) | S | needs BlueZ working | `bluez-obexd` + notification accept dialog. | Receive a photo from a phone. |
| Casting (P4) | L | No GPU encode. | Screen mirroring via `x11vnc` (1.6 MB) + `novnc` to a browser on the LAN (also useful for KeX-style use); Miracast/AirPlay not planned. | A PC browser at `http://walpad:6080` shows and controls the tablet (password, LAN only, off by default). |

### 4.9 Productivity

| App | Eff. | Plan | Package (installed size) | Acceptance test |
|---|---|---|---|---|
| Clock/alarms/timer/stopwatch (P1/P2, `clock-alarms-productivity`) | M | A real alarm must fire with the screen off: a core scheduler (timerfd) with a wake-capable alarm (`/sys/class/rtc/rtc0/wakealarm` for suspend) + shell alarm screen + sound via the safe audio path. | Pure Rust in core/shell (0 MB); `gnome-clocks` 0.5 MB is GTK4/libadwaita (too heavy). | Alarm in 2 minutes with the screen off rings with the notification and Snooze/Dismiss. |
| Calculator (P1, same change) | S | Rust calculator screen (standard + scientific) or `galculator` (0.8 MB, GTK) as stop-gap. | `galculator` | 12*(3+4) = 84. |
| Notes (P2) | S | Plain Markdown notes app in Rust (list + editor using the system keyboard) with a folder under `~/Notes`; stop-gap `mousepad` (0.7 MB). | `mousepad` | Create a note, reboot, still there. |
| Calendar and contacts (P2) | M | Local: `khal` + `vdirsyncer` (0.6 MB, CalDAV/CardDAV sync with the owner's server) behind a Rust UI, or `gnome-calendar` (0.8 MB + evolution-data-server 9 MB) as the GTK route. Start with `calcurse` (0.3 MB, TUI in st) to be real quickly. | `khal`, `vdirsyncer`, `khard`; `calcurse` | Add an event, alarm fires as a notification. |
| Email (P2) | S | `claws-mail` (3.5 MB, GTK) or `geary` (8 MB) or Firefox webmail; `neomutt` (2 MB) in st. | `claws-mail` | Receive mail over IMAP with TLS. |
| Weather (P4) | S | Shell widget with data from open-meteo (no key) fetched over HTTPS (`curl` + serde_json). | none | The widget shows current temperature; offline shows the last value with its age. |

### 4.10 Phone features (SIM)

State: SIM ready, registered on Grameenphone, AT works, PDP gets an IP, `ccmni` data does not flow yet. No audio routing from the modem (voice needs the MD speech path into the codec: not available on the legacy driver without the vendor audio HAL).

| Feature | Verdict | Plan |
|---|---|---|
| SMS (P2, a later change `sms-messaging` once data bring-up is done) | Feasible now over AT (`AT+CMGF=1`, `AT+CMGL`, `AT+CMGS`, `+CMTI` URC) | Core module `sms` (SQLite in `/var/lib/walpad/sms.db`, 0600), notifications for incoming messages, a Messages app (list + thread + compose with the system keyboard), UCS-2 for Bangla, long messages (concat). `gammu`/`gammu-smsd` (0.6 MB) is an alternative engine over the same AT port but fights for the single opener of `pttycmd1`: keep it in core. |
| Mobile data (P2) | Open bring-up item (NetAgent `AT+EIF ... ifst`). | Out of this plan; the shell UI exists. Alternative: USB LTE modem via `qmi_wwan`/`cdc_ncm` (both built) as a stop-gap. |
| Voice calls (P4, research) | Blocked by audio routing. | Only dialer UI + `ATD`/`ATA` plumbing is possible; audio would need the MTK `ccci` speech path and the codec route: open question, do not promise. |
| USSD, balance (P2) | AT `+CUSD`: small | A "USSD" dialog: `*121#` shows the response; Bangla operator codes work. |
| Location/GPS (P2) | mnld running, no positions | Finish `mnl2hal` protocol, expose `gpsd`-style JSON; `geoclue` not needed. |

### 4.11 System services

| Feature | Eff. | Plan |
|---|---|---|
| Power management, suspend/wake (P2) | M-L | Kernel has SUSPEND and PM_SLEEP; vendor suspend may hang the modem/Wi-Fi; try `echo mem > /sys/power/state` with RTC wake and a watchdog guard (the ramdisk pets the watchdog: a hung suspend would reboot). Until proven, use deep idle: screen off + cpufreq powersave + radios idle. |
| Charging UI (P2) | S | Charge state exists; add "Charging, 1h 20m left" estimate, and the `charge_keeper` script integration; battery warning notifications. |
| Thermal (P2) | S | `/sys/class/thermal` read-only in Device screen; throttle banner at 75 C, notification at 80 C, shutdown prompt. |
| Logs viewer (done), task manager (P2) | S-M | Logs page exists. Task manager: process list (CPU/RAM per app, kill) as a Tools entry; `xfce4-taskmanager` 0.2 MB as stop-gap; zram stats. |
| Crash reporter (P4) | S | Core detects non-zero exit of app ids and shell restarts, writes `/var/log/walpad/crash-<ts>.txt` (last logs, no secrets) and posts a notification "App crashed - View report". |
| Low-memory killer (P2) | S | `earlyoom` (Alpine package) or core policy: kill background apps at 10 % free, with a notification. |

### 4.12 Developer features

| Feature | Eff. | Plan |
|---|---|---|
| SSH (done), ADB-like remote shell (P2) | S | dropbear exists; add key-only, a Settings > Developer toggle, USB/Wi-Fi only (never the internet), telnet off by default (known recovery link remains). `openssh-sftp-server` for file push (fixes the missing scp/sftp). |
| Serial console / USB serial (P2) | S-M | `USB_ACM=y` works now; FTDI/CH340/CP210x need `usbserial.ko`+drivers (one module batch). `picocom` 0.1 MB, `minicom` 0.4 MB, a "Serial" app profile in the terminal. |
| Terminal tabs and profiles (P1/P2, within `app-switcher-navigation`'s follow-up) | M | `st` has no tabs: use `tmux` (0.8 MB) with a touch toolbar (Ctrl/Alt/Esc/Tab/arrows row on the system keyboard when the terminal is focused), or `xfce4-terminal` (0.3 MB, tabs) / `lxterminal`. Terminal "profiles": Alpine shell, Kali shell (section 5), Serial. |
| Docker | not possible | No user namespaces/cgroup pids. Use chroots (Kali) or `qemu-aarch64`-free rootfs trees. |
| Remote desktop (P4) | S | `x11vnc` + `novnc` over LAN; used for debugging the screen. |

### 4.13 Automation (P4)

A small rules engine in core: triggers (time, Wi-Fi SSID connect, charging state, app open, key chord, NFC none), actions (launch app, set toggle, run a fixed script from `/etc/walpad/actions.d`, notify). Rules in JSON, no arbitrary shell from the UI (apps launch by id only). Effort M. Acceptance: "When connected to SSID Home, turn hotspot off".

### 4.14 Security features of the OS itself

| Feature | Eff. | Plan / limits |
|---|---|---|
| Firewall UI (P2/P3) | M | iptables legacy (`IP_NF_FILTER`, `NETFILTER_XT_MATCH_OWNER`, `NF_NAT`); no nftables. `walpad-firewall` with default policy (drop inbound except established, SSH on LAN if enabled), per-uid outbound rules (owner match) for sandboxed users, UI toggles (Allow SSH, Allow VNC, Allow Samba) with a safe-revert timer. |
| App sandboxing without user namespaces (P3) | M | Run risky apps (Firefox with extra content, Kali tools) as separate uids inside chroots with `unshare -n`/`ip netns` for network isolation (NAMESPACES=y, NET_NS available to root), seccomp filters via a small launcher (`SECCOMP_FILTER=y`), `setpriv`/`su-exec`, drop capabilities. Honest limit: root in the chroot can break out; document, do not oversell. |
| Secure boot, verified boot (not possible) | - | Bootloader is the owner's unlocked MTK flow; do not plan. |
| Verified updates (P4) | M | Ed25519-signed update bundles + apk signing keys for our repo (`/etc/apk/keys`). |
| Remove unauthenticated telnet on USB (P1, recommend) | S | `known-issues`/contracts list telnetd on USB 192.168.42.1: gate it behind the lock PIN or a Settings toggle once lock exists (boot image is frozen: needs an approved boot change or a firewall rule in the chroot). |

## 5. Security-testing mode (Kali chroot)

Framing: this is for the owner's own devices, a lab, CTFs and authorised tests. The plan lists tool availability and environment
setup only, no attack procedures. A first prototype already exists in the repo as `walpad-kali`
(install of the NetHunter minimal rootfs with checksum check, bind mounts, shell/run/setup). The OpenSpec change
`kali-chroot-mode` turns it into a supported feature.

### 5.1 Installing Kali

- Place: `/opt/kali/rootfs` on userdata (about 53 GB total; the NetHunter minimal rootfs is a little over 100 MB compressed,
  the full one about 600 MB; typical tool sets 2 to 8 GB unpacked). Official minimal rootfs URL pattern:
  `https://kali.download/nethunter-images/current/rootfs/kali-nethunter-rootfs-minimal-arm64.tar.xz` plus `SHA256SUMS`
  (the script verifies the checksum before unpacking; keep that rule).
- Alternative: `debootstrap --arch=arm64 --variant=minbase kali-rolling /opt/kali/rootfs http://http.kali.org/kali`
  (the `debootstrap` package in Alpine is 0.2 MB but needs the Kali archive keyring `kali-archive-keyring.gpg` passed with
  `--keyring`; run on a PC in a container and ship the tarball if the tablet's busybox lacks pieces, always verify hashes).
- Suggested layout: rootfs tree, a `kali.tar.zst` backup snapshot (via the Backup feature), logs under `/opt/kali/logs`.

### 5.2 Mounts and launching

`walpad-kali` already binds `/dev`, `/dev/pts`, `/proc`, `/sys`, `/tmp` (the X11 socket lives in `/tmp/.X11-unix`) and `/mnt`
(SD/USB). Needed refinements:
1. Bind `/run/walpad` (read-only) and a private `/root/kali-share` shared with Alpine; mount SD/USB storage under `/mnt/<label>`
   (propagation: use `mount --rbind` so hot-plugged drives appear; or re-bind on `walpad-storage` events).
2. `chroot` with a clean env; `APT::Sandbox::User "root"` (done: no `_apt` network groups on this kernel).
3. DNS: copy `/etc/resolv.conf` at every start (done) and again after Wi-Fi/VPN changes (hook from walpad-wifi).
4. Unmount safety: refuse unmount while processes use the tree (`fuser`), `umount -l` fallback only on shutdown.
5. Terminal profile: apps.json entry `kali-terminal` with `exec: ["st","-e","walpad-kali","shell"]`, plus `kali-run` for X11 tools.

### 5.3 Package sets (inside the chroot; `apt install` after `apt update`)

| Metapackage | Use | Sensible? | Notes |
|---|---|---|---|
| `kali-linux-core` | Base system | Yes | Minimal. |
| `kali-linux-headless` | Default tools without a GUI | Yes (second step) | Large (several GB): install group by group. |
| `kali-tools-top10` | The ten most used tools | Yes | Start here; includes nmap, Metasploit, Wireshark, Burp (GUI), etc. depending on the release. |
| `kali-tools-information-gathering`, `kali-tools-web`, `kali-tools-sniffing-spoofing`, `kali-tools-passwords`, `kali-tools-forensics`, `kali-tools-reverse-engineering`, `kali-tools-crypto-stego`, `kali-tools-vulnerability` | Per-menu groups | Yes, on demand | Install only what you use (storage and RAM). |
| `kali-tools-wireless`, `kali-tools-802-11`, `kali-tools-bluetooth`, `kali-tools-rfid`, `kali-tools-sdr`, `kali-tools-hardware` | Radio/hardware | Only with matching hardware | See limits below. |
| `kali-linux-nethunter` | What NetHunter ships | Optional | Tools suited to a handheld. |
| `kali-linux-arm` | ARM-friendly set | Optional | |
| `kali-desktop-core` / `kali-desktop-xfce` | Full desktop | No | We use our own shell and openbox; run single GUI tools instead. |
| `kali-linux-large`, `kali-linux-everything`, `kali-tools-gpu` | Huge / GPU | No | GPU cracking is useless (no GPU userspace). |

Tool installs happen through apt in the chroot; the tablet's Alpine side stays minimal. Memory: 4 GB RAM with ZRAM;
limit Java GUI tools (Burp, ZAP) to one at a time; Metasploit needs ~1 GB and PostgreSQL. **Open question**: `memory/hardware.md` says no local
PostgreSQL (SYSVIPC is off); PostgreSQL 12+ can use POSIX shared memory (needs a tmpfs `/dev/shm` in the chroot), so test it
before promising `msfdb`.

### 5.4 X11 tools in the existing session

The chroot bind-mounts `/tmp/.X11-unix`; `DISPLAY=:0` already works with Xvfb `-ac` (no auth) and `-nolisten tcp`. GUI tools
(Wireshark, Burp, Zenmap) simply appear as windows managed by openbox, with the system keyboard through XIM (set
`XMODIFIERS=@im=walpad GTK_IM_MODULE=xim QT_IM_MODULE=xim` in the chroot launcher; the chroot needs `xim` immodule libs).
For KeX-like remote use, `x11vnc` + `novnc` on the Alpine side exposes the same display on the LAN (off by default).
Fonts: bind `/usr/share/fonts` read-only or install `fonts-noto-core` in Kali. Rotation/scale follow the session automatically.

### 5.5 Hardware limits and options

| Capability | Status on WalPad | What would be needed |
|---|---|---|
| Wi-Fi monitor/injection on the built-in chip | Not possible (CONNSYS gen4m, no monitor mode, stock firmware) | Use an OTG USB adapter. |
| USB Wi-Fi adapter | The kernel has `CFG80211=y`, `MAC80211=y`, `WLAN=y` with vendors mostly off, no RTL8xxxu/ath9k_htc/mt76; USB host works | Pick a chipset with a driver that builds out-of-tree against 4.19.127 (mac80211 present): e.g. Atheros AR9271 (`ath9k_htc`, in-tree driver compiled as module against the kernel headers) or MT7612U/MT7610U (`mt76x2u`) or RTL8812AU/RTL8821AU community drivers (aircrack-ng `rtl8812au`). Needs: module build from `tools/kernel-mt6765-4.19` (same path as `hci_vhci.ko`: `scripts/build_bt_module.sh`), firmware files copied into `/lib/firmware`, `iw`/`aircrack-ng` (2.5 MB Alpine) or inside Kali. Risk: MODVERSIONS vermagic must match; USB power draw on a tablet OTG port. |
| HID gadget (keyboard/mouse emulation to a PC, authorised testing of own machines/labs) | Kernel has `USB_CONFIGFS_F_HID`, `USB_F_HID`, `USB_CONFIGFS=y`, a UDC exists (the boot ramdisk binds it) | Runtime configfs gadget composed by `walpad-usbmode hid`: unbind UDC, add `functions/hid.usb0`, link, rebind; revert with a timer. Open questions: whether the musb UDC rebinds cleanly while the recovery gadget runs, and whether role switching (ID pin) leaves the port in peripheral mode with a cable attached. Test on the bench PC before promising. |
| USB Ethernet / tethering gadget | `RNDIS`, `ECM`, `NCM` in kernel | Same `walpad-usbmode` tool. Note: host-to-tablet USB ethernet never transmitted (hardware.md). |
| USB Ethernet adapter (host) | `AX88179`, `RTL8152`, `CDC_NCM/ECM`, `SMSC95xx` built in | Works: plug in, DHCP; useful for lab sniffing on a wired segment. |
| Bluetooth | BlueZ + vhci bridge, unverified | Needs the device test; no `hci_usb` for BT dongles (module), no HIDP/RFCOMM. |
| Packet capture on own networks | `AF_PACKET=y`; `tcpdump`, `wireshark`, `tshark` work in managed mode on wlan0/eth | In Kali: `tcpdump`, `tshark`, `ettercap`... use only on networks you own. Monitor mode only via USB adapter. |
| SDR/RFID/serial | `usbserial` missing (module), `USB_ACM` ok, SDR via libusb (rtl-sdr works through usbfs without a kernel driver: `libusb` userspace) | Check `CONFIG_USB_DEVICEFS`/usbfs present (verify); build `usbserial` + `ftdi_sio`, `cp210x`, `ch341` modules. |
| Raw sockets, iptables in chroot | Works (root in chroot is real root) | Same kernel; firewall rules are global. |

Safety defaults for the mode: banner on first start stating "own equipment and authorised targets only", logging of the commands is
the user's choice (local only), the mode never starts at boot, network egress firewall profile "lab" selectable (VPN/kill switch).

## 6. Launcher and home screen

Current Home: static 5-column grid of every app, no folders, no search, no widgets, one wallpaper.

Proposed (change `launcher-home-screen`, P2):
- Pages: Home page (widgets + favourites) and App drawer (all apps A-Z), with a search field at the top that also searches settings entries, files (name only, `find`-based index in `~/.cache/walpad/index`), contacts and notes.
- Folders: drag one icon onto another (long-press lift); folder opens as a popover grid; stored in `/var/lib/walpad/launcher.json`.
- Widgets (native Rust, drawn by the shell, no X apps): clock/date, weather (open-meteo), battery, next alarm/event, quick notes, media controls (MPRIS), shortcuts row.
- Wallpaper: chooser (built-ins + `~/Pictures`), per-orientation crop, optional dim; ambience-like theme accent colour (white line art stays default).
- Dock/sidebar: keep pinned apps; badge counts from notifications.
- Edit mode: long-press on empty area (Wallpaper, Widgets, Home settings).
Acceptance: create a folder, search "wifi" opens the Network settings page, resize a widget, survive reboot.

## 7. Default app catalogue

Sizes are the Alpine v3.22 aarch64 package "installed size" of the package alone (approximate; dependencies add more).
Firefox ESR is 224 MB; pick one app per role and keep the base image small. Fonts: `font-noto-bengali` 13 MB, `font-noto-emoji` 10 MB.

| Role | Default | Alpine package (size) | Alternatives |
|---|---|---|---|
| Browser | Firefox ESR (shipped) | `firefox-esr` (224 MB) | `epiphany`, `netsurf` (tiny) |
| Files | PCManFM (shipped) | `pcmanfm` (0.4 MB) + `gvfs`,`gvfs-smb`,`gvfs-mtp` (2.3 MB) | |
| Terminal | st (shipped) + tmux | `st`, `tmux` (0.8 MB); tabs: `xfce4-terminal` (0.3 MB) | `lxterminal` |
| Video | mpv | `mpv` (2.6 MB, libs ~35 MB), `ffmpeg` | `vlc` (28 MB) |
| Music | mpd/mpv-based Rust app | `mpd` (1.4 MB); `audacious` (1.2 MB) | `deadbeef` 9.8 MB |
| Images | ristretto/imv | `ristretto` (0.3 MB), `imv` (0.1 MB), `gthumb` (5.8 MB) | `gimp` 65 MB (optional) |
| PDF | evince | `evince` (0.9 MB + libs) | `zathura-pdf-mupdf` (0.6 MB), `foliate` 11.5 MB |
| Text editor | mousepad | `mousepad` (0.7 MB) | `geany` 21 MB |
| Office | AbiWord, Gnumeric | `abiword` (17.7 MB), `gnumeric` (9.8 MB) | `libreoffice-writer` (11 MB + common 311 MB) |
| Calendar/contacts | calcurse then khal | `calcurse` (0.3 MB), `khal`, `vdirsyncer`, `khard` (0.8 MB) | `gnome-calendar` + `evolution-data-server` (10 MB) |
| Email | claws-mail | `claws-mail` (3.5 MB) | `geary` 8 MB, `thunderbird` 226 MB |
| Calculator | Rust app | stop-gap `galculator` (0.8 MB) | `gnome-calculator` 1.6 MB |
| Clock/alarms | Rust (core) | none | |
| Notes | Rust or mousepad | `mousepad` | |
| Screenshot/record | shell + ffmpeg | `ffmpeg` (0.6 MB + libs), `scrot` (0.1 MB) | `maim` |
| Network tools | | `iw`, `iproute2`, `tcpdump` (0.9 MB), `nmap` (12.7 MB), `wireguard-go` (3.3 MB), `openvpn` (0.8 MB) | `networkmanager` not needed |
| Bluetooth | BlueZ | `bluez`, `bluez-obexd` (0.3 MB), `bluez-alsa` | `blueman` 1.6 MB |
| Sharing | Samba client/server, SFTP, VNC | `samba-client` (3.3 MB), `cifs-utils`, `nfs-utils` (1.7 MB), `openssh-sftp-server`, `x11vnc` (1.6 MB), `novnc` (1.9 MB) | `syncthing` 24 MB |
| Backup/crypto | | `borgbackup` (2.2 MB), `cryptsetup` (0.4 MB), `gocryptfs` (7.5 MB) | `restic` 42 MB |
| System | | `earlyoom`, `xfce4-taskmanager` (0.2 MB), `smartmontools` (1.6 MB), `usbutils`, `lm-sensors`, `fastfetch` | |
| Fonts/IME | | `font-noto-bengali`, `font-noto-emoji`, `hunspell`, `fcitx5-m17n` (+`m17n-db` 7 MB, Bangla InScript tables) as fallback IME | `ibus` 3 MB |
| Kali | chroot | `debootstrap` (0.2 MB, optional), Kali userland 0.1 to 8 GB in `/opt/kali` | |
| Accessibility | | `espeak-ng`, `onboard` is not needed (our keyboard) | |

Total new Alpine payload excluding Firefox/LibreOffice: roughly 250 to 400 MB with dependencies.

## 8. Phases

Phase 1: daily-driver basics (2 to 4 weeks of agent work)
1. `app-switcher-navigation` (Back/Home/Recents, task cards)
2. `clipboard-selection` (clipboard, long-press edit menu)
3. `notification-centre`
4. `lock-screen`
5. `display-sound-settings` (volume, wallpapers, text size, themes, night light)
6. `keyboard-languages-bangla` (layouts, Bangla, emoji)
7. `app-catalogue-manager` (install/remove/defaults)
8. `vpn-and-dns`

Phase 2: media and connectivity
9. `media-apps-capture` (video, music, images, PDF, screenshots, recording)
10. `clock-alarms-productivity` (alarms, calculator, notes, calendar basics)
11. `bluetooth-ui` (after the device test of the BlueZ chain)
12. `network-file-sharing` (SMB/NFS, SFTP, storage manager, optional share)
13. `launcher-home-screen`
Plus (no change yet): SMS app once AT-SMS is proven, USB serial/webcam modules batch, suspend experiments, camera per `camera-bringup`.

Phase 3: security-testing mode
14. `kali-chroot-mode`; then USB Wi-Fi adapter module build, `walpad-usbmode` (HID/tether/storage), firewall UI.

Phase 4: polish and OTA: predictive text, accessibility, split view, automation rules, crash reporter, OTA with signed updates + rollback, backup/restore, multi-user (probably never), voice typing, calls (only if audio routing appears).

Dependencies: notifications before alarms/SMS/Bluetooth receive; lock screen before screen pinning and the vault; clipboard and keyboard languages share the XIM commit work (do them in order 2 then 6 to avoid conflicts in `ime.rs`/`keyboard.rs`); catalogue manager before media apps (so they install from the catalogue).

## 9. OpenSpec changes created

See `openspec/changes/`: `app-switcher-navigation`, `clipboard-selection`, `notification-centre`, `lock-screen`,
`display-sound-settings`, `keyboard-languages-bangla`, `app-catalogue-manager`, `vpn-and-dns`, `media-apps-capture`,
`clock-alarms-productivity`, `bluetooth-ui`, `network-file-sharing`, `launcher-home-screen`, `kali-chroot-mode`.
Each has proposal.md, tasks.md and one spec; hardware behaviour not yet seen is written as open questions.

## 10. Open-source projects worth reusing

| Project | Use | Licence (check before copying) |
|---|---|---|
| Phosh / squeekboard / Chatty / Calls | UI ideas, keyboard layout JSON/YAML format, SMS/call UX | GPL-3 / LGPL (ideas, not code) |
| Lomiri / Ubuntu Touch, Plasma Mobile, Sailfish | Gesture models, indicators, telephony stack (ofono) | GPL/LGPL |
| postmarketOS | Package and device-port patterns (we are Alpine too), `pmbootstrap` for building kernel modules | various |
| Kali NetHunter (+ kali-nethunter docs) | Chroot manager design, kernel feature list, minimal rootfs | GPL |
| OpenBangla Keyboard / `riti` | Bangla phonetic and fixed layout engine in Rust | MPL-2.0 / GPL-3 (verify per crate) |
| `zbus` | Pure-Rust D-Bus (notifications server, BlueZ client) | MIT |
| `wireguard-go`, `openvpn`, `strongswan` | VPN | MIT / GPL |
| `mpv`, `ffmpeg`, `mpd` | Media | GPL/LGPL |
| `evince`, `zathura`, `mupdf` | PDF | GPL / AGPL(mupdf: check) |
| `umtp-responder` | MTP over FunctionFS | GPL |
| `x11vnc`, `novnc` | Remote screen/casting | GPL / MPL |
| `khal`, `vdirsyncer`, `calcurse` | Calendar/contacts (CalDAV) | MIT/BSD/GPL |
| `earlyoom` | OOM management | MIT |
| `borg`, `restic`, `cryptsetup`, `gocryptfs` | Backup and encryption | BSD/GPL/MIT |
| `dunst` | Fallback notification daemon | BSD |
| KDE Connect / `kdeconnect`, Syncthing | Phone integration, sync | GPL / MPL |
| Open-Meteo | Weather API without a key | open data, non-commercial/API terms |

## 11. Sources

- Kali metapackages: https://www.kali.org/docs/general-use/metapackages/
- NetHunter components (chroot sizes, HID, OTG wireless, kernel features): https://www.kali.org/docs/nethunter/nethunter-components/
- NetHunter rootless and chroot manager: https://www.kali.org/docs/nethunter/nethunter-rootless/ , https://www.kali.org/docs/nethunter/nethunter-chroot-manager/
- Installing NetHunter / building / porting: https://www.kali.org/docs/nethunter/installing-nethunter , https://www.kali.org/docs/nethunter/building-nethunter/ , https://www.kali.org/docs/nethunter/porting-nethunter/
- OpenBangla riti: https://github.com/OpenBangla/riti
- Alpine package index (sizes measured from the v3.22 aarch64 APKINDEX): https://dl-cdn.alpinelinux.org/alpine/v3.22/community/aarch64/ , https://pkgs.alpinelinux.org
- Phosh: https://phosh.mobi , squeekboard: https://gitlab.gnome.org/World/Phosh/squeekboard
- Ubuntu Touch docs: https://docs.ubports.com
- postmarketOS wiki: https://wiki.postmarketos.org
- Sailfish OS: https://sailfishos.org
- Plasma Mobile: https://plasma-mobile.org
- ChromeOS features: https://chromeos.google/
- freedesktop notification spec: https://specifications.freedesktop.org/notification-spec/latest/
- XDND / selections (ICCCM): https://www.x.org/releases/X11R7.6/doc/xorg-docs/specs/ICCCM/icccm.html
- Linux USB gadget configfs: https://www.kernel.org/doc/html/v4.19/usb/gadget_configfs.html
- umtp-responder: https://github.com/viveris/uMTP-Responder
- wireguard-go: https://git.zx2c4.com/wireguard-go
- zbus: https://github.com/dbus2/zbus
- Open-Meteo: https://open-meteo.com
- Project facts: `data/kernel/walpad-4.19.127.config`, `.agents/shared/memory/{hardware,known-issues,decisions}.md`,
  `docs/rust/{UI,CONTRACTS}.md`, `docs/30-bluetooth.md`, `openspec/changes/camera-bringup`.
