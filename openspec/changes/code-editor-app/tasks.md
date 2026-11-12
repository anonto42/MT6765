## 1. Package availability (real evidence, not a guess)

- [x] 1.1 `apk add --root ... --arch aarch64 -X .../v3.24/main -X .../v3.24/community lite-xl`
  against a throwaway root in the same `alpine:3.24` container `scripts/build_rootfs.sh` uses:
  confirms `lite-xl-2.1.8-r3` is a real prebuilt package (not source-only)
- [x] 1.2 Same for `geany` (`geany-2.1-r0`); `apk info -R` dependency closure for both
- [x] 1.3 Diff installed-package lists: lite-xl adds 2 packages over the shipped set
  (`lite-xl` + `lua5.4-libs`); geany adds 1 (`geany`, since GTK3 already comes in via
  firefox-esr/pcmanfm/gpicview) but its *runtime* dependency closure is the full GTK3 stack
- [x] 1.4 `readelf -d` on the installed `lite-xl` binary: 4 dynamic libs, no libSDL2 (statically
  linked) — write up in `docs/46-ide.md`

## 2. Decision and docs

- [x] 2.1 `docs/46-ide.md`: decision (Lite XL), the owner's stated condition, and the exact `apk`
  evidence for both candidates

## 3. Image

- [x] 3.1 `scripts/build_rootfs.sh`: add `lite-xl` to `GUI_PKGS` (ships in both the Python and
  no-Python images; not Python-runtime-specific, so no no-Python filter change needed)
- [x] 3.2 `config/rootfs/etc/walpad/apps.json` and
  `config/rootfs-nopython/files/etc/walpad/apps.json`: new app id `code`

## 4. UI

- [x] 4.1 `crates/walpad-ui/src/icons.rs`: new `Icon::Code` (`</>`, 24 px grid, 2 px stroke),
  `from_name("code"|"editor")`, `Icon::ALL` count, unit tests

## 5. Check

- [ ] 5.1 `scripts/rust.sh fmt`, `scripts/check.sh` prints ALL CHECKS PASSED
- [ ] 5.2 `tests/shell/run.sh` passes

## 6. Device (read-only only; no install, no flash — needs the owner's explicit approval first)

- [ ] 6.1 Not done here: this change was made from an isolated agent worktree with no
  `build/ssh/config` and no network path to the tablet. A read-only SSH sanity check
  (`apk update`/`apk search`, no install) is left for the owner or a device-connected agent
  (exact commands in `docs/46-ide.md`)
- [ ] 6.2 Not done (out of scope for this change): real on-device launch, `WM_CLASS` via `xprop`,
  and a touch test of the editor view — needs a rebuilt image and a flash, which needs the
  owner's explicit approval
