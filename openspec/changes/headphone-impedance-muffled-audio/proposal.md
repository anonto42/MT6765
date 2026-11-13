# Headphone impedance detection stuck at 0 (muffled audio investigation)

## Status: kernel patch written and compile-verified; NOT flashed; real audible fix still unconfirmed

## Why

Owner request 2026-11-13 (same night as the "Headphone Plugged In" silence fix): headphones
produce real but muffled/quiet audio. Silence itself was already fixed separately (that ALSA
control wasn't being set). This change investigates the muffled-quality complaint via the MT6357
codec's per-headphone impedance calibration (`Audio HP ImpeDance Setting`, numid 20), which always
reads `0` live on the device, and the working hypothesis going in was that `set_hp_impedance_ctl`
(the hook that gates real calibration) is `NULL` due to a missing devicetree node, Kconfig option,
or sub-driver.

## What was actually found

The NULL-pointer hypothesis does not hold up under source audit. In
`tools/kernel-mt6765-4.19/sound/soc/mediatek/mt6765/mt6765-sound.c:2653`, `init_afe_ops()` calls
`set_codec_ops(&mtk_codec_platform_ops)` unconditionally from `InitAfeControl()` at AFE platform
probe -- no `#ifdef`, no devicetree-conditional, no missing sub-driver. `data/kernel`'s
`r1_defconfig` matches this exact board (`CONFIG_MACH_MT6765=y`, `CONFIG_MTK_PLATFORM="mt6765"`,
`CONFIG_MTK_SND_SOC_CODEC="mt6357"`), and `BYPASS_HPIMP` (the build-time escape hatch that would
skip detection by design) is commented out. So `set_hp_impedance_ctl` is very likely **not** NULL
on the real device.

The real mechanism, confirmed by source + a live, read-only SSH check (see **Verification**):
`OpenHeadPhoneImpedanceSetting()`'s first line is `if (GetDLStatus()) return false;` -- a
deliberate hardware-safety guard against reconfiguring the live analog headphone front-end.
`GetDLStatus()` is true whenever `mAudio_Ana_DevicePower[HEADSETL]` or `[HEADSETR]` is true, and
those flags are set by `Audio_Amp_L_Switch`/`Audio_Amp_R_Switch` -- the exact controls
`config/rootfs/usr/local/sbin/walpad-audio` turns **On unconditionally at boot** ("boot default:
speaker amps on (unchanged behaviour)") and never turns back off. So on this board, every read of
numid 20 after boot trips that guard before `detect_impedance()` ever runs, and the static
`hp_impedance` variable stays at its compile-time default (`0`) forever. This fully reproduces
"always reads 0" without the registration pointer needing to be NULL at all -- category **(d)
something else entirely**, not a missing DT node/Kconfig/sub-driver.

Separately, and importantly: `hp_impedance` has **zero downstream consumers** anywhere in this
driver (written only by `detect_impedance()`, read only by the ucontrol get callback and debug
prints). A correct measurement, by itself, would not change playback quality. A real per-headphone
gain-compensation knob does exist and is wired to real analog hardware:
`Apply_N12DB_Gain` (numid 22) gates a `-12dB` de-gain on the headset IVBUF path, applied from
`Headset_Speaker_Amp_Change()` -- the function behind `Headset_Speaker_Amp_Switch`, which
`walpad-audio`'s headphone route already uses. It defaults to Off and nothing in WalPad ever sets
it (confirmed live: `Apply_N12DB_Gain` reads `0` on the device right now). This is a real,
immediately-testable-by-ear candidate for the muffled complaint, **not changed in this patch**
(see **Out of scope**).

## What changed (kernel source, compile-verified, not flashed)

- `tools/kernel-mt6765-4.19/sound/soc/mediatek/codec/mt6357/mtk-soc-codec-6357.c`:
  `hp_impedance` now starts at a sentinel `HP_IMPEDANCE_UNMEASURED` (-1) instead of `0`, and
  `hp_impedance_get()` reports `HP_IMPEDANCE_DEFAULT_OHM` (32 ohm -- the same constant this
  driver's own `BYPASS_HPIMP` path already hardcodes for the same "typical wired earphone" case)
  whenever no real detection has ever completed, instead of a bare `0` that reads like a literal
  "shorted" measurement. A real completed measurement is still kept and reported as-is; the
  `GetDLStatus()`/`OpenHeadPhoneImpedanceSetting()` hardware-safety guard itself is untouched.
- `scripts/build_codec_check.sh` (new): compile-checks one in-tree object (this file) against
  WalPad's real running 4.19.127 config/headers, reusing `scripts/build_bt_module.sh`'s proven
  `kheaders.ko` overlay recipe. There is no full-Image build path in this repo yet (the vendor
  kernel is policy-frozen, see `openspec/config.yaml`: "the stock vendor kernel 4.19.127 cannot
  change"), so this is the closest real verification available without one.

## Why the hardcoded default, not "wire up real calibration"

Making live detection actually succeed needs either (a) loading the currently-not-loaded
`mt6357-accdet.ko` and adding a new jack-insert-triggered callback into this codec driver -- a
real cross-driver kernel design change whose IRQ/locking/pop behaviour cannot be verified without
hardware testing in one offline session -- or (b) resequencing `walpad-audio` in userspace to
briefly drop `Audio_Amp_L_Switch`/`Audio_Amp_R_Switch` around a detection read (no kernel change
needed for that part). Either way, `hp_impedance` still has no consumer to act on the result. Given
all that, and given this project's standing policy that the vendor kernel does not change,
hardcoding a sane default is the right-sized fix here, exactly as the fallback this investigation
was scoped to allow.

## Out of scope (explicitly, for the owner)

- **Not flashed, not built as a full Image.** This repo has no full-kernel-Image build path; the
  vendor kernel is policy-frozen (`openspec/config.yaml`). This patch exists as a compile-verified,
  honest record of the real root cause and a ready-to-use fix if the owner ever decides to build
  and flash a modified kernel. **Flashing a new boot image needs the owner's explicit, real-time
  approval** and is not something to do without that conversation.
- **`Apply_N12DB_Gain` / `Headset_Speaker_Amp_Switch` experiment.** Zero-risk, needs no kernel
  change or reflash, and is the most direct lead on the actual muffled-audio complaint found
  tonight. Try on the device, by hand, compare by ear:
  ```
  ssh -F build/ssh/config walpad "amixer -c 0 cset name='Apply_N12DB_Gain' On; \
    amixer -c 0 cset name='Headset_Speaker_Amp_Switch' Off; \
    amixer -c 0 cset name='Headset_Speaker_Amp_Switch' On"
  ```
  then play something and listen; `cset name='Apply_N12DB_Gain' Off` + bounce the switch again to
  compare back. Not done here because I cannot hear the result and the task explicitly asked me
  not to guess at or claim an audible fix; getting the direction wrong could make it worse, not
  better.
- Loading `mt6357-accdet.ko` for real jack-insert detection: already explicitly deferred to its own
  approved change in `openspec/changes/headphone-mic-jack/proposal.md`; not reopened here.
- Resequencing `walpad-audio` to attempt a real one-time detection window: not implemented;
  would need the owner's input on when a read would even make sense (no accdet IRQ exists to
  trigger it off a real jack-insert event -- see the accdet point above), and still would not, by
  itself, change playback without also deciding an `Apply_N12DB_Gain` policy.

## Device risk

None. No partition read or written beyond read-only `amixer`/`pactl`/`dmesg`/`uname` queries over
SSH. No module loaded or unloaded on the device. No kernel flashed. The only repo changes are
kernel *source* (not deployed anywhere) and a new build-verification script.

## Secrets impact

None. `build/ssh/config` and `build/ssh/known_hosts` (both gitignored, copied locally from the
main checkout to reach the device from this worktree) contain no credentials beyond a LAN IP and a
host-key fingerprint; not committed.

## Verification

- Source audit: `init_afe_ops()`/`set_codec_ops()` wiring, `r1_defconfig` match, `BYPASS_HPIMP`
  commented out, `GetDLStatus()`/`Audio_AmpL_Set`/`Audio_AmpR_Set` trace, `walpad-audio`'s
  unconditional boot-time `Audio_Amp_L_Switch On`/`Audio_Amp_R_Switch On`, zero downstream
  consumers of `hp_impedance`, `Apply_N12DB_Gain` wiring into `Headset_Speaker_Amp_Change()`.
- Live, read-only, over SSH (`build/ssh/config` host `walpad`): `amixer -c 0 cget numid=20` reads
  `0` on repeated cold reads with `pactl list sinks` showing the sink `IDLE` and `Headphone Plugged
  In` (numid 21) `Off` (headphones not inserted during the check); `amixer -c 0 cget
  name='Apply_N12DB_Gain'` reads `0` (Off).
- Build: `scripts/build_codec_check.sh sound/soc/mediatek/codec/mt6357/mtk-soc-codec-6357.o`
  compiles clean (zero warnings/errors under `-Wall -Wextra -Werror`) against WalPad's real
  running kernel config/headers; `nm` on the resulting object shows `hp_impedance`,
  `hp_impedance_get`, `set_hp_impedance_ctl` present as expected. One unrelated, pre-existing
  warning-promoted-to-error (an `int`-vs-`void`-returning `.remove` mismatch on
  `struct snd_soc_component_driver`, identical in the sibling `mt6358` driver -- a tree-wide vendor
  pattern, not introduced by this patch) only surfaces under the stand-in Debian clang-11.0.1-2
  package's stricter warning split and is suppressed in the build script's flags, not "fixed" in
  the source.
