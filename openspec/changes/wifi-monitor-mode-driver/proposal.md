# Wi-Fi monitor mode driver

## Why

Owner request 2026-11-11. `iw dev wlan0 interface add mon0 type monitor` fails `Not supported
(-95)`: the stock `wlan_drv_gen4m.ko` is built without `CFG_SUPPORT_SNIFFER_RADIOTAP`, so
`NL80211_IFTYPE_MONITOR` is never added to `wiphy->interface_modes` and the monitor-only code
paths (`mtk_init_monitor_role`, `mtk_uninit_monitor_role`, `mtk_cfg80211_set_monitor_channel` in
`os/linux/gl_cfg80211.c`) are compiled out.

## What changes

- `scripts/build_wifi_module.sh`, modelled on `scripts/build_bt_module.sh`'s proven
  docker/header-overlay/CRC-cross-check pipeline, builds `wlan_drv_gen4m.ko` for the running
  4.19.127 kernel from `tools/kernel-mt6765-4.19` with `CONFIG_MTK_WIFI_SNIFFER_RADIOTAP=y` added
  to the gen4m-specific make variables (`MTK_COMBO_CHIP=CONNAC WLAN_CHIP_ID=6765
  CONFIG_MTK_COMBO_WIFI_HIF=axi ...`) that `drivers/misc/mediatek/connectivity/Makefile` normally
  `export`s before recursing into gen4m (bypassed here since the module is built directly via
  `M=.../wlan/core/gen4m`, same as the BT build bypasses `drivers/bluetooth`'s parent).
- Output (build-only, gitignored, not committed): `build/wifi/wlan_drv_gen4m.ko` +
  `.sha256`/`modinfo.txt`/`crc-report.txt`/`config.diff`. The stock module stays untouched at
  `build/wifi/vendor-modules/wlan_drv_gen4m.ko` (extracted from `build/android-restore/super-raw.img`,
  same recipe as the BT build) for the owner's own rollback/diff.
- Two extensions to the BT script's CRC method, needed because gen4m cross-links against symbols
  exported by other vendor `.ko`s (wmt_drv, wmt_chrdev_wifi), not only the core kernel:
  authoritative CRCs are also read from each vendor `.ko`'s own `__crc_*` symtab entries (not just
  its `__versions` import table), and `Module.symvers`'s module column is corrected from
  `vmlinux` to the real owning `.ko` for those symbols so the built module's `depends:` matches
  stock's (`wmt_drv,wmt_chrdev_wifi`) instead of coming out empty.
- A small, explicitly justified source-drift patch (config.h, gl_init.c) removes two call sites
  (`CFG_SUPPORT_ICS`, the second `wifi_fwlog_get_fw_ver_register` call) that this exact kernel
  submodule checkout compiles but the real shipped module's own `__versions`/`depends:` prove are
  not in the real build -- confirmed before patching, not guessed. Does not touch any
  `CFG_SUPPORT_SNIFFER_RADIOTAP` code.

## Out of scope

Loading the module on the device, `walpad-wifi`/`stack_up` changes, any device state change.
Build-only; the owner tests and loads it personally.

## Device risk

None from this change by itself (no file here is read by the running system). If the owner loads
it: wrong CRCs would simply refuse to `insmod` (checked and clean here); a working monitor mode
interface briefly takes the radio off its normal station role while `mon0` exists, no partition
writes involved either way.

## Verification

Build-side only (`scripts/build_wifi_module.sh`): vermagic equals `wmt_drv.ko`'s
(`4.19.127 SMP preempt mod_unload modversions aarch64`); every CRC in the built module's
`__versions` table traced back to the `Module.symvers` used for that build; zero unresolved
symbols, zero vendor/computed CRC mismatches left unexplained. Monitor-mode symbols
(`mtk_init_monitor_role`, `mtk_uninit_monitor_role`, `mtk_cfg80211_set_monitor_channel`) present in
the built `.ko`, absent from the stock one -- confirmed twice: once inside the build, once again
independently by parsing the copied-out `.ko`'s ELF `.symtab` directly on the host. Loading on the
tablet (`insmod`, `iw dev wlan0 interface add mon0 type monitor`) is the owner's own next step.

## Open questions

- Whether the on-device firmware/MCU side actually honors the requested monitor role once the
  host driver advertises it (CFG files like `CFG_SUPPORT_PDMA_SCATTER`, also enabled by this same
  flag, assume firmware support) is unverified without the device.
