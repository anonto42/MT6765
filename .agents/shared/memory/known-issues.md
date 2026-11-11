# Known Issues and Traps

- `pkill -f <pattern>` over SSH also matches the SSH shell's own command line and kills the
  session: use `pidof`, or anchored `pgrep -f "^/path"`.
- busybox `blkid` ignores `-o value -s TYPE`: parse `TYPE="..."` from the full line.
- busybox has no `xxd`; the tablet has no `scp`/sftp: copy with `ssh host 'cat > f' < f`
  (`scripts/deploy.sh` does this and verifies hashes and the live files).
- `speaker-test`/raw `hw:0,0` at 48 kHz or default buffers hangs the tablet (hardware watchdog
  reboots it after minutes, sometimes not at all: then hold power 15 s).
- Sound (owner request 2026-11-10): Firefox/apps play through a system-wide PulseAudio (`walpad-sound`,
  `/etc/pulse/*`, socket /run/pulse/native) whose only sink is `walpad_out` (asound.conf, the 44.1 kHz plug).
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
