## ADDED Requirements

### Requirement: Code editor app entry
The system SHALL ship a `code` app id (`lite-xl`) in both `config/rootfs/etc/walpad/apps.json`
and `config/rootfs-nopython/files/etc/walpad/apps.json`, launched by id only through the existing
app allowlist, with its own forced `WM_CLASS` so window matching is deterministic.

#### Scenario: launch from the Apps panel
- **WHEN** the owner launches the `code` app
- **THEN** the core runs `lite-xl` with `SDL_VIDEO_X11_WMCLASS=lite-xl` in its environment and the
  contract environment, never a shell string

#### Scenario: open a file
- **WHEN** `launch` is called with `args.path` for the `code` app
- **THEN** the core appends exactly one resolved path argument to `exec` (no `--` separator,
  since lite-xl treats every argv entry as a file/directory to open)

### Requirement: Package footprint evidence
The chosen editor package SHALL be a prebuilt Alpine 3.24 aarch64 package reachable through the
same `apk`/container mechanism `scripts/build_rootfs.sh` already uses, not a from-source build.

#### Scenario: availability check
- **WHEN** `apk add --root ... --arch aarch64 -X .../main -X .../community lite-xl` runs against
  an `alpine:3.24` container
- **THEN** it succeeds and installs `lite-xl` plus at most a small number of packages not already
  in the shipped package set
