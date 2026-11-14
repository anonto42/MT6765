# Headphone jack and microphone

## Why

Owner request 2026-11-12 (typos kept): "i want the hadephone jack have to work and also the
microphons ahve to work". Neither works today: playback always goes to the speaker, and there is
no capture path (no PulseAudio source, no ALSA capture device wired up).

## What changes

- `config/rootfs/usr/local/sbin/walpad-audio` gets a `jack <headphone|speaker> <builtin|headset>`
  subcommand that flips the MT6357 codec's confirmed-real mixer switches (research:
  `docs/45-headphone-mic.md`); the no-arg boot behaviour (speaker on) is unchanged.
- `/etc/asound.conf` + `/etc/pulse/system.pa` gain a capture PCM/source on hw:0,1 (the codec's
  separate `MultiMedia1_Capture` device), at the same safe 44.1 kHz S16 stereo as the existing sink.
- `walpad-core`: `walpad_hw::jack` (hardware read of a switch/extcon node; always `None` on this
  kernel build — honest, not guessed), `logic::jack` (pure route/mic-source state machine, unit
  tested), `jack::Jack` (drives the script as a timed child, polled from the daemon loop, persists
  the override). New IPC: `Status.jack: JackStatus {detected, forced, route}`, command
  `jack_force {route: null|"headphones"|"headset_with_mic"}`.
- `walpad-shell`: the Quick Settings volume caption shows "Headphones"/"Headset mic" while the
  route is in effect (text only, no new row/icon/geometry).
- `walpad-ctl jack off|headphones|headset`.

## Out of scope

- Loading `mt6357-accdet.ko` (the real jack-insert-detection kernel module; present, prebuilt, at
  `/vendor/lib/modules/mt6357-accdet.ko`, confirmed not loaded) — that is a live hardware
  experiment, explicitly deferred to its own approved change (`docs/45-headphone-mic.md`). Until
  then, route selection is a manual Settings/`walpad-ctl` override, never a sensed plug-in; the
  status field is labelled honestly (`detected` stays `None`).
- Opening the new capture PCM/PulseAudio source on the device, and recording a test clip: not run
  (same class of risk as the playback hang in `memory/known-issues.md`), needs an on-device test.
- A dedicated headphone icon or a new Quick Settings row/geometry (a separate agent owns UI polish
  this cycle).
- Voice-call audio routing through the headset (out of scope; see `walpad-callaudio`, a separate,
  already-experimental script).

## Device risk

No flashing, no partition writes. Research was read-only over SSH (every command `timeout`-
wrapped); no live device state was changed while preparing this change. The new capture PCM could,
in principle, hang the tablet the way wrong playback parameters once did
(`memory/known-issues.md`) — mitigated by mirroring the sink's already-safe parameters, but this is
explicitly **not yet verified live** and is called out as such in the status docs and the handback.

## Secrets impact

None. No new secret, credential or passphrase is introduced.

## Verification

Host: unit tests for `logic::jack` (route/mic-source transitions) and `jack::Jack` (persisted
override, apply-once), `walpad-ctl` command-building tests, a shell test for `walpad-audio jack`
(fake `amixer`, confirms only existing controls are set, never a PCM tool). `scripts/check.sh`
green.

Device (not done by this agent, for the owner next): `walpad-ctl jack headphones` + wired
headphones + `amixer -c 0 cget` to confirm the switches really flipped, then listen; only after
that, try the mic path; see the ordered list at the end of `docs/45-headphone-mic.md`.
