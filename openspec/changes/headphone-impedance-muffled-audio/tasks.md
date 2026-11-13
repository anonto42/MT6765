## 1. Research (read-only, device + kernel source)

- [x] 1.1 Trace `set_hp_impedance_ctl` registration (`set_codec_ops`/`init_afe_ops` in
      `mt6765-sound.c`); confirm it matches `r1_defconfig` (`CONFIG_MACH_MT6765`,
      `CONFIG_MTK_PLATFORM`, `CONFIG_MTK_SND_SOC_CODEC`) and `BYPASS_HPIMP` is off
- [x] 1.2 Find why `hp_impedance_get()` still always returns 0 despite the wiring being intact:
      `GetDLStatus()`/`OpenHeadPhoneImpedanceSetting()` guard, `Audio_AmpL_Set`/`Audio_AmpR_Set`,
      `walpad-audio`'s unconditional boot-time `Audio_Amp_L_Switch On`/`Audio_Amp_R_Switch On`
- [x] 1.3 Check for any downstream consumer of `hp_impedance` (found: none) and for any real
      impedance-adjacent gain-compensation hook that does exist (found: `Apply_N12DB_Gain` ->
      `Headset_Speaker_Amp_Change()`'s `-12dB` IVBUF de-gain)
- [x] 1.4 Live, read-only confirmation over SSH: `amixer -c 0 cget numid=20` (reads 0),
      `pactl list sinks` (IDLE), `amixer -c 0 cget numid=21` (Headphone Plugged In, Off),
      `amixer -c 0 cget name='Apply_N12DB_Gain'` (Off)

## 2. Kernel source patch

- [x] 2.1 `mtk-soc-codec-6357.c`: `HP_IMPEDANCE_UNMEASURED`/`HP_IMPEDANCE_DEFAULT_OHM`,
      `hp_impedance_get()` reports the default instead of a bare 0 when nothing was ever measured
- [x] 2.2 `scripts/build_codec_check.sh`: new build-verification script (kheaders.ko overlay
      recipe adapted from `build_bt_module.sh`)
- [x] 2.3 Compile-check `sound/soc/mediatek/codec/mt6357/mtk-soc-codec-6357.o` against WalPad's
      real running config/headers: clean, zero warnings/errors; `nm` shows expected symbols

## 3. Owner (not done here; in order of cost/risk)

- [ ] 3.1 Zero-risk, no reflash: try the `Apply_N12DB_Gain` + `Headset_Speaker_Amp_Switch` bounce
      from the proposal's **Out of scope** section on real headphones, by ear, both directions
- [ ] 3.2 Decide whether `Apply_N12DB_Gain` should default On for the headphone route based on
      3.1 (a separate, small change to either the kernel default or `walpad-audio`, once the
      direction is confirmed by ear -- not guessed here)
- [ ] 3.3 Decide whether a full kernel Image rebuild + flash (to carry this patch, or any future
      real jack-insert-triggered calibration) is ever worth doing given the project's standing
      "vendor kernel does not change" policy; if yes, **explicit real-time approval required**
      before any `scripts/flash_os.sh`/`flash_partition.sh`/mtk.py is run
