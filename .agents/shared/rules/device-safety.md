# Device Safety

- Never flash or erase a partition without the owner's explicit approval in this conversation.
  Never run `tools/mtk/mtk.py`, `scripts/flash_os.sh`, `flash_partition.sh`, `fastboot_py.py`
  or write a block device on your own; prepare the command, the owner approves or runs it.
- After any write, read back and compare hashes (a write after a stale session can report
  success without landing). `flash_os.sh` does this: watch `build/flash/os.result`.
- mtkclient: preloader mode, no preloader file; start ONE mtk.py command, then the owner unplugs,
  holds power 15 s until off, releases, replugs with no buttons. Never two mtkclient processes.
  Read-only dumps (`mtk.py ro <offset> <len>`) also need the owner's OK.
- Flash both partitions: `scripts/flash_os.sh <rootfs.ext4> build/android-restore/boot_a-os.img`.
  A reflash wipes userdata (saved Wi-Fi, NV-independent). Prefer SSH deploys instead.
- Keep `build/rootfs/rootfs.ext4` (rollback image), `build/android-restore/super-*.img`,
  `firmware/MT6765/backups/` (incl. modem NV backups). Never delete them.
- Partition-writing vendor daemons (ccci_fsd, nvram_daemon) only with NV backups in place.
- Serial console wedges after large transfers; use mtkclient reads for anything big.
- Fastboot (`0e8d:201c`, unlocked): no `fastboot` binary, use `scripts/fastboot_py.py`.
