# Scope: root filesystem and device scripts

`scripts/build_rootfs.sh` (Alpine 3.24, package lists BASE_PKGS/GUI_PKGS, `WALPAD_NO_PYTHON=1`),
overlay `config/rootfs/` (+ `config/rootfs-nopython/` for walpad-init, session, runtime.conf,
apps.json). Boot order in `/sbin/walpad-init` (two copies: keep both in sync): busybox links,
hwclock, devpts, super partitions (dmlinear), firmware binds, dropbear, walpad-wifi,
walpad-storage, walpad-audio, core, session. Shell tests: `tests/shell/run.sh` (fake roots).
