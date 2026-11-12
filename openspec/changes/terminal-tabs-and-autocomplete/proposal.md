# Terminal: default shell with autosuggestions/completion (tabs already shipped)

## Why

Owner request 2026-11-12 (verbatim, typos kept), two asks:
1. "i want that on the top side of the terminal have to open multerpal sessions like other
   terminals do and i will be abale to navigate btuwing them"
2. "you must style the terminal like auto sujessation auto complites etc like thiks"

## What changes

- **Ask 1 (terminal tabs): nothing new is built.** Reading `crates/walpad-shell/src/wmbar.rs` and
  `docs/rust/CONTRACTS.md` ("Multitasking and windows") shows this is already shipped under the
  `multitasking-windows` change (owner request 2026-11-11): `apps.json` gives `terminal`
  `max_instances: 6`, and the window bar at the top of the screen already draws one tab per open
  terminal session with a "+" to start another (`new_instance`) and an "x" per tab to close just
  that one (`close_window`), tracked safely by pid even though `terminal` and `kali` share
  WM_CLASS `st`. Full reasoning, including why adding `tabbed` (suckless) on top was considered
  and rejected (not packaged for Alpine at all, and would conflict with the window bar's own
  per-window tabs), is in `docs/46-terminal-tabs-and-autocomplete.md`.
- **Ask 2 (autocomplete/autosuggestion)**: the `terminal` app's `st` now runs `fish` (`-e fish`)
  instead of the user's login shell. `fish` gives autosuggestions and tab completion with no
  config needed, verified available for Alpine 3.24 aarch64 (`fish-4.6.0-r1`) through the same
  docker/apk mechanism `scripts/build_rootfs.sh` already uses. `zsh` + `zsh-autosuggestions` +
  `zsh-syntax-highlighting` are also available aarch64 packages but need more wiring (plugin
  sourcing from a shipped `zshrc`) for the same result, so `fish` was picked. Root's system login
  shell (`/etc/passwd`, used by SSH) is untouched; this is scoped to the `terminal` app's `exec`
  only. The `kali` app (a separate Debian/NetHunter chroot, not built by this script) is
  deliberately left on its own shell.

## Out of scope

- A literal single-window "tabbed" visual style (merging the window bar into the terminal's own
  frame) — a possible follow-up to `wmbar.rs`'s rendering only, not a new package; not done here.
- Changing Kali's shell inside its chroot.
- Changing root's system-wide login shell (SSH, scripts) away from `ash`.

## Device risk

None: no partition write, no flash, no kernel module, no reboot. Package-list and config-only
change; takes effect on the next `scripts/build_rootfs.sh` rebuild + flash (owner-approved, not
done by this change).

## Secrets impact

None.

## Verification on the tablet (not done by the agent that wrote this; needs a rebuilt+flashed image)

1. `ssh walpad`, `walpad-ctl launch terminal`: the window bar shows a single tab ("Terminal 1")
   with a "+"; `walpad-ctl new-instance terminal` a few times: more tabs appear, each switchable by
   tap, each closable by its own "x" without touching the others.
2. Open the terminal: the prompt is `fish`'s; typing a previously-run command shows a greyed-out
   suggestion; Right arrow/End accepts it; Tab on a partial command/path completes it. The on-screen
   keyboard and touch gestures (selection toolbar Copy/Paste) behave the same as before (shell-
   agnostic at the X11 level).
3. `ssh walpad` itself still lands in `ash`/`sh` (unchanged), and `walpad-kali run echo ok` still
   works unchanged.

## Open questions

- Whether `wmbar`'s tab strip is already visible on the owner's current running image, or whether
  it needs the already-pending rebuild/flash from `multitasking-windows` — this change does not
  change that answer, it only confirms the code exists.
