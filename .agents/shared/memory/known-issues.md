# Known Issues and Traps

- `pkill -f <pattern>` over SSH also matches the SSH shell's own command line and kills the
  session: use `pidof`, or anchored `pgrep -f "^/path"`.
- busybox `blkid` ignores `-o value -s TYPE`: parse `TYPE="..."` from the full line.
- busybox has no `xxd`; the tablet has no `scp`/sftp: copy with `ssh host 'cat > f' < f`
  (`scripts/deploy.sh` does this and verifies hashes and the live files).
- `speaker-test`/raw `hw:0,0` at 48 kHz or default buffers hangs the tablet (hardware watchdog
  reboots it after minutes, sometimes not at all: then hold power 15 s).
- Sound (owner request 2026-11-10): Firefox/apps play through a system-wide PulseAudio (`walpad-sound`,
  `/etc/pulse/*`, socket /run/pulse/native) whose only sink is `hw:0,0` at 44.1 kHz (system.pa).
  Never add `pulseaudio-alsa`/`alsa-plugins-pulse` (they redirect ALSA `default` into pulse: a loop). Volume is
  software (`walpad-volume`, owned by walpad-core); the codec PGA gain is only the no-pulse fallback. Idle pulse
  suspends the card after 10 s, so `aplay`/`walpad-volume test` still work then.
- Kernel threads have the boot ramdisk as root (chroot boot): firmware for request_firmware must be
  bind-mounted under `/proc/2/root` (done in walpad-init).
- `/dev` is a bind of the ramdisk's: `/dev/pts` must be mounted in the chroot (walpad-init).
- Zombies of an earlier core are reparented to the ramdisk PID 1, which never reaps: process
  checks must skip state Z.
- openbox ignores the position of managed dock-type windows: strips (dock, keyboard, mouse pad)
  are override-redirect plus a managed 1x1 strut window.
- matchbox-keyboard is maximised by openbox and loops at ~85% CPU: replaced by the shell keyboard.
- Vendor daemons must run under their own process name (symlink to the bionic linker named like
  the daemon): the ccci driver rejects MD start from a process called `linker`.
- Modem/GPS daemons are 32-bit: 32-bit linker, `/vendor/lib`, VNDK APEX libs (mounted from
  `/system/system/apex/com.android.vndk.current.apex`), `libwpdprops32.so`.
- mtkclient: after a dump the preloader session can stay open; a flash may then fail at once with
  "Wrong magic" (nothing written); replug and retry.
- Agent worktrees may start from an old base commit (058bbe4): tell agents to check
  `git log -1` and `git reset --hard main` in their own worktree. Agents hit the 5-hour usage
  limit and must be resumed with SendMessage.
- Never register binfmt_misc entries from a privileged container: the registry is host-global
  (a bad entry broke every exec on the PC until reboot).
- `/tmp/wpd-props` is the Wi-Fi stack's property file and the (frozen) 64-bit shim reads only its
  first 8 KB. Never append build.prop lines to it: the chip id behind them is invisible, wmt_launcher
  never serves, no Wi-Fi/Bluetooth. Modem/GPS use `/tmp/wpd-props32`, the camera `/tmp/wpd-props64`.
  Opening `/dev/stpbt` (Bluetooth) before wmt_launcher serves also wedges CONNSYS: Bluetooth starts
  only once `wlan0` exists (walpad-init).
- Never extract a tar built on the PC onto the tablet as root without `--no-same-owner`/ownership
  forced: PC uid 1000 owned `/` and `/root`, dropbear then refused the key (no SSH). Recovery that
  worked: read-only mtk dump, fix inodes on a copy with debugfs, `jr` replay, write back only the
  changed blocks, read back and compare (owner approved each write).
- The USB serial console `/dev/ttyACM0` (after boot) is a root shell of the ramdisk; the system is
  under `/newroot`. `build/tools/ser.py 'cmd'` (run with tools/mtk/.venv/bin/python) sends one
  command. Use it when Wi-Fi is down.
- `find / -user 1000 -exec chown` also hits the NV partition roots (nvdata 0:1000, nvcfg/protect_f/
  protect_s 1000:1000): exclude `/mnt/vendor` (restored from the backups once).
- `LK Load MD: Disabled` (`cat /sys/kernel/ccci/lk_md`): the bootloader did not load the modem this
  boot (seen after a restart via the flash tool); no software can start it, reboot normally.
- zsh does not word-split unquoted variables: use arrays or sh -c.
- Mobile data link (static RE 2026-11-11, `docs/42-mobile-data-link.md`): the interface for `+EIF: <id>, ifup` is `ccmni<id%100>` (302 = ccmni2, not ccmni0) and the RIL confirms it with `AT+EIF=<id>, "ifst", "up", <type>` (quotes and spaces exactly) on `/dev/radio/pttynwcmd`. Device-verified result pending: if downlink still fails see the experiment list there.
- No sound (YouTube silent, 2026-11-11): `period_time`/`buffer_time` inside a plug `slave {}` make alsa-lib
  fail with "Unknown field period_time", so PulseAudio's sink never loaded (log: "Failed to parse module
  arguments", `mixer_reset` is no alsa-sink argument either) and Firefox had no output. dmix fails too (no
  SysV IPC). Fix: sink on `device=hw:0,0` with rate/format/fragments in system.pa; plain plug in asound.conf.
- Panel colours (2026-11-11): /dev/fb0 reads a pixel's bytes as R,G,B (not B,G,R): solid red written as BGRA
  showed blue, blue showed red, YouTube's red logo looked purple. fbmirror swaps the R/B bytes (PX macro).
  Anything else drawing straight to fb0 (padui, splash) needs the same order. Test: write solid colours to
  /dev/fb0 with fbmirror stopped and pan "0,0".
- USB driver-CD dongles (2026-11-11): once `usb-storage` binds the fake CD, SCSI scan, `eject`, `dd` and
  `usb_modeswitch` all hang; only a udev-at-plug-in switch works reliably. A passthrough of it at VM boot hangs
  Windows Setup. Details: `memory/usb-dongle-identify.md`.
- QEMU user-net DNS (slirp) uses only the FIRST `/etc/resolv.conf` nameserver. Here that is the host's own LAN IP with
  nothing listening, so the guest has a link but no DNS: set the guest DNS to 8.8.8.8 (and `ipv6=off`).
- QEMU monitor over `nc -U`: `nc` never exits (wrap in `timeout`); `sendkey` uses the GUEST layout (UK: `|` is
  `shift-less`, `*` is `shift-8`). Never `pkill -f <pattern>` from a shell whose own command line contains it.
- `rtl8xxxu` has no `new_id` (no runtime ID adding): a clone ID needs a rebuilt module. One constant value for all
  register reads means the chip is not Realtek.
- ZT9101 driver in monitor mode (2026-11-12): receive needs the RCR filter command (`UMSG_OPS_MP_SET_PRX`), not just
  `iw set type monitor`; radiotap must not claim an FCS the chip does not append; `iw dev info` shows no channel
  (no get_channel op, cosmetic). Details and patch: `memory/usb-dongle-identify.md`.
- Wi-Fi monitor mode (2026-11-12): wlan_drv_gen4m has it in source (CFG_SUPPORT_SNIFFER_RADIOTAP,
  gated off in the stock build); scripts/build_wifi_module.sh builds a working replacement (confirmed:
  matching vermagic/CRCs, monitor-mode functions present, loads cleanly). Two ways to actually run it
  were tried and both are dead ends:
  - Boot-time preload (insmod before wmt_drv/wmt_chrdev_wifi exist): safe, but wlan_drv_gen4m needs
    those two loaded first; walpad-wifi's own insmod_once calls for them run back-to-back with no
    usable gap to win the race from a separate script without touching CONNSYS (forbidden).
  - Live hot-swap on an already-running chip (even with the correct `echo 0`/`echo 1` to /dev/wmtWifi
    power-off/on, mtk_wcn_wmt_func_off/on): watchdog-reset the tablet (self-recovered, no lasting
    damage both times, but not safe to repeat).
  - The only way left is replacing /vendor/lib/modules/wlan_drv_gen4m.ko itself, so the stock boot
    sequence (already correct) loads it unmodified. /vendor is dm-verity protected: a plain
    `mount -o remount,rw` is refused ("Permission denied") even as root. Doing this for real needs
    disabling Android verified boot (vbmeta), which is actual partition flashing (mtk.py/fastboot) and
    carries real brick risk (not self-recovering like the above) - owner decided not to pursue this.
  Conclusion: monitor mode is not usable on this tablet's current boot chain. Don't re-attempt the
  boot-race or live-swap approaches; they're confirmed dead ends. Revisit only if the owner explicitly
  wants to disable verified boot.
