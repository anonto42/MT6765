# Headphone jack and microphone (owner request 2026-11-12, change `headphone-mic-jack`)

Owner words (typos kept): "i want the hadephone jack have to work and also the microphons ahve to
work". Research done read-only over SSH (`ssh -F build/ssh/config walpad`, every command
`timeout`-wrapped) and by reading the real driver source
(`tools/kernel-mt6765-4.19/sound/soc/mediatek/codec/mt6357/mtk-soc-codec-6357.c`,
`tools/kernel-mt6765-4.19/sound/soc/codecs/mt6357-accdet.c`, and the kernel config
`data/kernel/walpad-4.19.127.config`). No live device state was changed.

## Confirmed real (on the device, 2026-11-12)

- `amixer -c 0 controls` (119 controls total) has every name this change uses, byte for byte, and
  `amixer -c 0 contents` for the ones checked shows sane enum item lists matching the driver
  source. None of this appears under `amixer -c 0 scontrols`: these are `SOC_ENUM_EXT` controls
  without a standard Switch/Volume type, so the "simple controls" view the task description warned
  about really does hide them; `controls`/`contents` do not.
  - Output: `Headset_Speaker_Amp_Switch` (On/Off), `Headset_PGAL_GAIN`, `Headset_PGAR_GAIN`
    (0-19 like `Lineout_PGAL/R_GAIN`, same enum family as `Speaker_Amp_Switch`,
    `Ext_Speaker_Amp_Switch`: the codec drives one DAC into several independently-switched analog
    outputs, so headphone output is the *same* playback PCM (hw:0,0) with different switches, not
    a separate device).
  - Capture: `Audio_ADC_1_Switch` / `Audio_ADC_2_Switch` (On/Off), `Audio_ADC_1_Sel` /
    `Audio_ADC_2_Sel` (items `idle`/`AIN`/`Preamp`, already `Preamp` on this device),
    `Audio_Preamp1_Switch` / `Audio_Preamp2_Switch` (items `OPEN`/`IN_ADC1`/`IN_ADC2`/`IN_ADC3`),
    `Audio_MIC1_Mode_Select` / `Audio_MIC2_Mode_Select` (items `ACCMODE`/`DCCMODE`/`DMIC`/
    `DCCECMDIFFMODE`/`DCCECMSINGLEMODE`, default `ACCMODE`), `Audio_MicSource1_Setting` (items
    `ADC1`/`ADC2`/`ADC3`/`ADC4`). The vendor driver's own comment on `Audio_MicSource1_Set` says
    it plainly: *"ADC1 Mic source selection, 'ADC1' is main_mic, 'ADC2' is headset_mic"*.
  - `Headphone Plugged In` exists (ENUMERATED Off/On) but is **write-only software state**: the
    `_Set` handler just stores the value and, when set to On, reads `get_accdet_auxadc()` once for
    impedance math. Nothing in this kernel sets it automatically; it is not a jack sensor.
- `/proc/asound/pcm`: capture is a separate ALSA device, `00-01 MultiMedia1_Capture`
  (`mt-soc-codec-rx-dai-1`), i.e. **hw:0,1**, not hw:0,0. `00-29 Headphone_PLayback` is
  `snd-soc-dummy-dai-29` — a dummy DAI with no real backend link on this build, so it is *not*
  the path headphone output actually uses (confirmed by its PCM device number never changing the
  mixer state; the real headphone path is the switch family above).
- Only `Audio_ADC_1`/`_2`, `Audio_Preamp1`/`_2`, `Audio_MicSource1` and `Audio_MIC1`/`_2_Mode` are
  wired up; `Audio_ADC_3`/`_4`, `Audio_MicSource2`/`_3`/`_4` and `Audio_PGA3`/`_4` getters/setters
  are stubs that just say `/* 6752 removed */` in the driver source — a leftover from a sibling
  SoC variant (PGA1/PGA2 were not individually re-checked). **`Audio_MicSource1_Setting` is the only
  functional capture mux**, so both the built-in and the headset mic are captured through the same
  logical "MicSource1" path; switching mic means pointing that one mux at ADC1 (built-in, per the
  driver's own comment) or ADC2 (headset), not running two capture paths at once.
- Kernel config: `CONFIG_SND_SOC_MT6357_ACCDET=m` (the real jack-detect driver,
  `sound/soc/codecs/mt6357-accdet.c`, DT-matched by `compatible = "mediatek,mt6357-accdet"`,
  which is the `pwrap@.../pmic_accdet` node the owner's request pointed at) is built as a
  **loadable module**, and it is **present, prebuilt, on the device**:
  `/vendor/lib/modules/mt6357-accdet.ko`. `lsmod` on the device shows only the Wi-Fi modules
  (`wlan_drv_gen4m`, `wmt_chrdev_wifi`, `wmt_drv`) — **accdet is not loaded**. There is no
  `/sys/class/switch`, no `/sys/class/extcon` node and no accdet entry in
  `/proc/bus/input/devices`, consistent with the module never having run.
  - The legacy, separate `drivers/misc/mediatek/accdet/accdet_drv.c` (a different, older driver,
    `compatible = "mediatek,mt8173-accdet"` etc.) is not built at all (`CONFIG_MTK_ACCDET` is
    absent from the config): it is not the one that matters here.
- Device tree (`/sys/firmware/devicetree/base/pwrap@1000d000/main_pmic/pmic_accdet/`) has real
  accdet configuration properties (`headset-mode-setting`, `headset-key-mode`,
  `headset-eint-num`, `accdet-mic-mode`, `accdet-mic-vol`, ...): the board was designed with a
  4-pole headset jack and ACCDET wiring; it is just not initialized by anything running today.

## Scope decision: no accdet insmod in this change

Loading `mt6357-accdet.ko` live was explicitly out of scope for this agent (read-only research
only; the Wi-Fi driver work the same night already caused two watchdog resets from live hardware
experiments). Loading it is the obvious next step for real jack-insert detection, but it is
untested here and must be its own approved, on-device step: insert it with a short `timeout`, have
a way to recover (console/USB serial) if it wedges anything, and confirm whether it actually
creates a switch/extcon node or calls back into `hp_plugged_in_set`/the codec before trusting it.
**Not done. Needs an on-device test.**

Because of that, this change ships **manual override only** (scope reduction the task explicitly
allowed): `jack_force {route: null|"headphones"|"headset_with_mic"}` (IPC), `walpad-ctl jack
off|headphones|headset`. `walpad_hw::jack::read_jack` is wired up and polled (`crate::jack`,
2 s) so that if/when accdet is loaded and creates `/sys/class/switch/h2w/state` or
`/sys/class/extcon/extcon0/state`, live detection starts working with no further code change — but
until then it reads `None` honestly, every time, and `JackStatus.detected` stays `None`.
`JackStatus.forced` carries the manual override and is clearly a separate field from `detected` in
the IPC status (see `docs/rust/CONTRACTS.md`), so the shell/owner can tell "the owner set this" from
"the hardware reported this" — it can never report a detection that is not real.

A plain switch (the kind accdet would create) can only report presence, never whether the inserted
accessory has a mic; `walpad_hw::jack::read_jack` therefore can only ever promote `Some(true)` to
`JackKind::Headphones`. Telling a mic-equipped headset apart needs accdet's own 4-pole
classification (impedance / key thresholds), which is a `None` until that module runs — so even
once hot-plug detection exists, "headset with mic" will likely still need the owner's confirmation
the first time (or careful accdet register reading this change did not attempt).

## Does this tablet have a built-in microphone?

Only AC-coupled electret / DMIC mode strings on `Audio_MIC1_Mode_Select` were enumerated, not
whether a microphone element is actually soldered behind MIC1. The codec driver and device tree
only prove MIC1 (ADC1/Preamp1, "main_mic" by the driver's own naming) is a wired, usable input
*path*; nothing read so far proves or disproves a physical built-in mic capsule exists on this
board revision versus MIC1 being reserved for a yet-unused purpose. **Not verified either way.**
Recording through the built-in path and listening back is the only way to know, and that is a live
test this agent did not run. Until someone does, treat "built-in mic" status as "the driver thinks
there is one and happily enables that path" rather than "confirmed working."

## What this change does

1. `config/rootfs/usr/local/sbin/walpad-audio` gets a `jack <headphone|speaker> <builtin|headset>`
   subcommand (the no-arg boot behaviour is unchanged). Every `amixer cset` is checked against
   `amixer controls` first (never fired blind) and `timeout 3`-wrapped, same discipline as
   `walpad-callaudio`.
2. `/etc/asound.conf` gets `pcm.walpad_in` (hw:0,1, same safe 44.1 kHz S16 stereo plug as
   `walpad_out`); `/etc/pulse/system.pa` gets a `module-alsa-source` on hw:0,1
   (`source_name=walpad_mic`, `set-default-source`). **This has not been opened on the device.**
   Opening the capture PCM at the wrong parameters is exactly the class of bug that hung the
   tablet for playback (`memory/known-issues.md`); the fragment/rate settings mirror the
   already-safe sink, but actually starting PulseAudio with this source present, and recording
   through it, is an on-device test this agent did not run.
3. `walpad-core`: `walpad_hw::jack` (hardware read, honestly `None`), `logic::jack` (pure state
   machine: `effective`/`route_for`/`mic_source_for`, unit tested), `jack::Jack` (the module that
   persists the override in `/var/lib/walpad/jack-force` and drives `walpad-audio jack ...` as a
   timed child, polled from the daemon loop — same pattern as `volume.rs`/`hotspot.rs`). Wired into
   `Status.jack` and command `jack_force`.
4. `walpad-shell`: the Quick Settings volume caption reads "Headphones" / "Headset mic" instead of
   "Volume" while the override is in effect (no new icon, no new Quick Settings row/geometry — a
   separate agent owns UI polish right now).
5. `walpad-ctl jack off|headphones|headset` for the owner to flip the override from a shell.

## What the owner should test first (in this order, each with a timeout / a way to recover)

1. `walpad-ctl jack headphones`, confirm with `amixer -c 0 cget name='Headset_Speaker_Amp_Switch'`
   that it reads On and `Speaker_Amp_Switch` reads Off, then play something and listen on wired
   headphones. `walpad-ctl jack off` to go back to the speaker.
2. Only after (1) sounds right: `walpad-ctl jack headset`, then try recording (short, `timeout`-
   bounded) through `walpad_in` / the `walpad_mic` PulseAudio source, to see whether the headset
   mic or the built-in path actually picks up sound.
3. Loading `mt6357-accdet.ko` for real jack-insert detection is a separate, future, explicitly
   approved change (see "Scope decision" above) — not part of this one.
