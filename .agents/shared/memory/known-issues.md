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
- ZT9101 USB Wi-Fi dongle on WalPad itself (2026-11-12, `openspec/changes/zt9101-usb-wifi-driver/`):
  station mode, monitor mode AND injection all confirmed working via a custom-built
  `zt9101_ztopmac_usb.ko` (`scripts/build_zt9101_module.sh`) -- unlike the onboard CONNSYS chip above,
  this one is NOT a dead end. Traps hit along the way, reusable for any future out-of-tree USB driver
  build on this kernel:
  - Building an out-of-tree module whose source lives outside the kernel tree (`M=<external path>`)
    still needs `Module.symvers` written at the kernel BUILD tree's root ($S in `build_bt_module.sh`'s
    scheme), never in the module's own directory -- modpost looks there regardless of where `M` points;
    writing it to the module dir silently produces a CRC-less, unverified `.ko`.
  - A driver with its own platform-select Makefile (`HOST_PLAT`) that doesn't list this device needs
    `HOST_PLAT` set to anything unmatched to skip those branches and pass ARCH/CROSS_COMPILE/KSRC on
    the command line instead -- but check what else those branches set besides the toolchain
    (`-DCONFIG_LITTLE_ENDIAN` here) and pass it back via `USER_EXTRA_CFLAGS`/`EXTRA_CFLAGS`, or code
    guarded on it silently breaks (here: a union's only member, used by TKIP/AES IV macros).
  - `LLVM=1` must be passed explicitly to an external module's OWN inner `$(MAKE) -C $(KSRC)
    M=$(WDRV_DIR)` invocation too, not just the outer one -- it is not automatically forwarded by every
    out-of-tree Makefile the way ARCH/CROSS_COMPILE are.
  - A genksyms-computed CRC can legitimately disagree with a real vendor module's CRC for a symbol
    exported from a MTK-vendor-patched kernel file (`kernel/sched/core.c`, `mm/memory.c` here) when the
    only kernel source tree available (`tools/kernel-mt6765-4.19`) is a different point-release than
    what actually built the running kernel -- the header overlay fixes `include/`, not `kernel/sched/`
    or `mm/`. Vendor's CRC (read from an already-loaded module) is ground truth in that case; trust it
    explicitly and narrowly (see `scripts/bt/merge_symvers.py`'s optional expected-mismatch arg), don't
    loosen the check generally.
  - This chip ships in a fake-CD-ROM mode (`2310:1a2b`) until mode-switched to Wi-Fi mode (`2310:9086`);
    on WalPad, `usb-storage` auto-binds the fake CD on enumeration same as on a PC (same trap as "USB
    driver-CD dongles" above) -- unbind it (`echo IFACE > /sys/bus/usb/drivers/usb-storage/unbind`)
    BEFORE running `usb_modeswitch`, or the switch can hang. The host PC's own known-good
    `/etc/usb_modeswitch.d/2310:1a2b` config (`StandardEject=1`) works unchanged on WalPad.
  - The dongle showed real USB enumeration instability directly on the bare OTG port (repeated
    connect/reset/disconnect within ~200ms, several times) before ever settling -- looked like a
    power-delivery limit, not a software bug. It did eventually enumerate and stay stable on retries
    without a powered hub in the end, but that is the first thing to suspect if it recurs.
  - Running the dongle in STATION mode (own DHCP lease) at the same time as the onboard Wi-Fi's own
    station-mode connection broke the tablet's SSH/network reachability for several minutes
    (`Destination Host Unreachable` from the LAN gateway) -- the tablet itself never crashed (uptime
    never reset, screen stayed responsive) and it self-recovered (a later DHCP lease landed on a
    different address), but avoid two simultaneous station-mode radios on this tablet until the actual
    routing conflict is root-caused. Monitor mode does not carry this specific risk (no DHCP).
  - `ifname=`/`if2name=` in this driver's config file MUST be set to something other than
    `wlan0`/`p2p0` -- those names are already owned by the onboard CONNSYS chip, which stays up the
    whole time; this driver defaults to `wlan0` in its sample config and would collide.
  - For any aircrack-ng-suite work on this device, install `aircrack-ng` inside the already-provisioned
    `walpad-kali` chroot (`openspec/changes/kali-chroot-mode/`, `/usr/local/sbin/walpad-kali mount` then
    `run`), not via `apk add` into the base Alpine rootfs -- the chroot shares the host kernel so it
    sees the dongle's netdev directly, and this keeps the security-testing tool stack out of the base
    image.
- Headphone impedance always reads 0 (2026-11-13, `openspec/changes/headphone-impedance-muffled-audio/`):
  do NOT assume `set_hp_impedance_ctl` is NULL just because `Audio HP ImpeDance Setting` (numid 20)
  reads 0 -- in `mtk-soc-codec-6357.c`, registration (`init_afe_ops`/`set_codec_ops` in
  `mt6765-sound.c`) is unconditional and matches this board's `r1_defconfig` exactly. The real cause:
  `walpad-audio` turns `Audio_Amp_L_Switch`/`Audio_Amp_R_Switch` On at boot and never back off, which
  keeps `mAudio_Ana_DevicePower[HEADSETL/R]` (so `GetDLStatus()`) permanently true, which trips
  `OpenHeadPhoneImpedanceSetting()`'s very first guard (`if (GetDLStatus()) return false;`) on every
  read, so `detect_impedance()` never runs. `hp_impedance` also has zero downstream consumers in this
  driver -- fixing the readback alone changes nothing audible. `Apply_N12DB_Gain` (numid 22, a real
  `-12dB` de-gain on the headset IVBUF path inside `Headset_Speaker_Amp_Change()`) is the one real,
  wired, currently-Off-by-default gain-compensation knob that actually affects headphone playback;
  worth trying by ear (bounce `Headset_Speaker_Amp_Switch` after setting it, see the OpenSpec
  proposal) before any kernel work on this path.
- Compile-checking one in-tree (obj-y) kernel source file with `make path/to/file.o` does NOT pick up
  the parent directory's `subdir-ccflags-y` (e.g. `sound/soc/mediatek/Makefile`'s `-I.../common_int`,
  `-I.../codec/$(MTK_CODEC)`) the way a full `make modules`/`make vmlinux` build would -- pass the
  same `-I` paths explicitly via `KCFLAGS` (see `scripts/build_codec_check.sh`). Also: Debian's
  packaged clang-11.0.1-2 splits function-pointer-signature mismatches into
  `-Wincompatible-function-pointer-types`, separate from `-Wincompatible-pointer-types` (which this
  kernel tree already disables treewide) -- the real vendor clang-11.0.1 r383902 toolchain does not
  make this split, so a pre-existing, harmless `int`-vs-`void` `.remove` mismatch (present identically
  in both `mt6357` and `mt6358` codec drivers) only errors under the stand-in toolchain; suppress it
  in the build script's flags, don't "fix" source that builds fine on the real toolchain.
