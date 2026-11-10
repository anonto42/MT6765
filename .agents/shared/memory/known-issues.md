# Known Issues and Traps

- `pkill -f <pattern>` over SSH also matches the SSH shell's own command line and kills the
  session: use `pidof`, or anchored `pgrep -f "^/path"`.
- busybox `blkid` ignores `-o value -s TYPE`: parse `TYPE="..."` from the full line.
- busybox has no `xxd`; the tablet has no `scp`/sftp: copy with `ssh host 'cat > f' < f`
  (`scripts/deploy.sh` does this and verifies hashes and the live files).
- `speaker-test`/raw `hw:0,0` at 48 kHz or default buffers hangs the tablet (hardware watchdog
  reboots it after minutes, sometimes not at all: then hold power 15 s).
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
