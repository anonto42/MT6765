## 1. Research (read-only, device + kernel source)

- [x] 1.1 Enumerate real amixer control names via `amixer -c 0 controls`/`contents`, cross-checked
      against `tools/kernel-mt6765-4.19/sound/soc/mediatek/codec/mt6357/mtk-soc-codec-6357.c`
- [x] 1.2 Confirm accdet module state: built as `=m`, prebuilt `.ko` present on `/vendor`, not in
      `lsmod`, no switch/extcon/input node
- [x] 1.3 Write up confirmed-vs-theoretical in `docs/45-headphone-mic.md`

## 2. Host

- [x] 2.1 `walpad_hw::jack` (honest `None`, tested)
- [x] 2.2 `logic::jack` pure state machine + transition unit tests
- [x] 2.3 `jack::Jack` driver module (persisted override, timed child, polled)
- [x] 2.4 IPC: `Status.jack`, `jack_force` command, `walpad-ctl jack`
- [x] 2.5 `walpad-audio jack <route> <mic>` + shell test (fake amixer)
- [x] 2.6 asound.conf / system.pa capture wiring
- [x] 2.7 Quick Settings volume caption reflects the route
- [x] 2.8 `scripts/check.sh` passes

## 3. Device (not part of this change; for the owner, in order)

- [ ] 3.1 `walpad-ctl jack headphones`, confirm with `amixer cget`, listen on wired headphones
- [ ] 3.2 `walpad-ctl jack headset`, test recording (short, timeout-bounded)
- [ ] 3.3 Separately: load `mt6357-accdet.ko` for real jack-insert detection (its own approved
      change, not this one)
