# Terminal tabs and shell autocomplete (owner request 2026-11-12)

Owner words (verbatim, typos kept):
1. "i want that on the top side of the terminal have to open multerpal sessions like other
   terminals do and i will be abale to navigate btuwing them"
2. "you must style the terminal like auto sujessation auto complites etc like thiks"

Both were researched against the real Alpine aarch64 package repo through the project's own
`scripts/build_rootfs.sh` apk mechanism (docker, `--arch aarch64`, `-X .../main -X .../community`),
not assumed. Full commands are in the OpenSpec change's proposal (`openspec/changes/
terminal-tabs-and-autocomplete/`).

## 1. Terminal tabs: already shipped, no new code

Before building anything, `crates/walpad-shell/src/wmbar.rs` and `docs/rust/CONTRACTS.md`
("Multitasking and windows") were read. They describe a feature shipped under OpenSpec change
`multitasking-windows` (owner request **2026-11-11**, itself driven by the owner's own words then:
"i don't have multitasking on the terminal ... like a pc"):

- `apps.json` already gives the `terminal` app `max_instances: 6` (and `kali` 3).
- A 48 px strip (`walpad-wmbar`, `BAR_CLASS`) is shown at the **top of the visible area** whenever
  a window of a multi-instance app (the terminal) is in front, with **one tab per open session**
  (`BarModel.tabs`, labelled "Terminal 1", "Terminal 2", ...), a **"+" button** that sends
  `new_instance` to start one more `st` process, and an **"x" on each tab** that sends
  `close_window` for just that session. Tapping a tab sends `focus_window`
  (`crates/walpad-core/src/daemon/windows.rs::focus_window`). In `single` layout mode (the
  default) every window of the app is maximised to the same rect, so switching tabs behaves
  exactly like switching tabs in a desktop terminal: one session visible at a time, the others
  kept alive underneath.
- This is tracked end-to-end by pid: `Daemon::app_of_window` (`daemon/windows.rs:43`) matches a
  window to an app first by the pid of the child that was spawned for it
  (`self.apps.instance_of_pid`), falling back to WM_CLASS only for windows the core did not spawn
  itself — which is exactly why `terminal` and `kali` can share WM_CLASS `st` without the dock's
  close button or the window bar ever confusing the two (the pid-based fix mentioned in the
  project history, 2026-11-12).

This already matches the owner's literal request ("open multiple sessions... navigate between
them", tabs "on the top side of the terminal") almost word for word, and it is more deeply
integrated with the project's own window manager (pid tracking, Recents, the dock's close-by-
window-id fix, the layout engine) than anything a new mechanism could be bolted on as.

**Decision: no new terminal-tabs mechanism was built.** Adding `tabbed` (see below) on top would
create a second, competing tab strip and would conflict with the one that already exists (see
"Why not tabbed" below). If the owner tries the terminal and still does not see tabs, the most
likely explanation is that the running device image predates the `multitasking-windows` change
(the image on the tablet has not been rebuilt/reflashed since 2026-11-11) — that is a flash/
verification step for the owner, not a missing feature, and is called out explicitly in the
handback.

### Why not `tabbed` (suckless)

Checked for real, not assumed:
- `apk search` (and a direct `apk add --arch aarch64 ... tabbed` against
  `https://dl-cdn.alpinelinux.org/alpine/v3.24/{main,community}`, and `edge/testing`) finds **no
  `tabbed` package at all** for Alpine, any architecture: `apk search tabbed` only turns up
  unrelated packages (`perl-tickit-widget-tabbed`, `sxmo-utils`). It would have to be built from
  source, the same way `tools/fbmirror`/`tools/padui`/`tools/dmlinear` are already built as static
  C tools and installed into the image.
- Alpine's `st` package (`community/st/APKBUILD`, pinned to the vanilla
  `https://dl.suckless.org/st/st-0.9.3.tar.gz` tarball, no patches beyond a `tic` Makefile line
  removal) **does** support XEmbed: `st -h` prints `-w windowid` in its usage, confirmed by running
  it against a throwaway Xvfb inside the same Alpine 3.24 container. So `tabbed -c st -w` would
  technically work mechanically.
- But even if `tabbed` were built from source, making it the "terminal" app's top-level window
  would turn its WM_CLASS into `tabbed`, not `st`; the embedded `st` instances it spawns are
  XEmbed children, not separate top-level windows, so they stop appearing in
  `b.app_windows()`/`_NET_CLIENT_LIST` at all. That would make `wmbar`'s own per-window tab
  tracking, `new_instance`/`max_instances`, and the dock's pid-based close-by-window-id fix all
  moot for the terminal, and `tabbed`'s own tab strip would duplicate the window bar's (two
  tab strips, one above the other, doing nearly the same thing two different ways).
- Given the window bar already exists, is already pid-safe against the Kali chroot sharing
  WM_CLASS `st`, and `tabbed` is not even packaged, building and wiring it would be strictly more
  risk for a feature that already ships. This is also why "think through the max_instances/
  window-tracking interaction, flag if too complex" in the task brief turned out to be the right
  call: the interaction is not merely complex, it actively removes working behaviour.

If a literal "tabbed" look (one single OS-level window, tabs drawn inside it rather than a
separate strip above it) is wanted later, that is a UI-only change to `wmbar.rs`'s rendering
(e.g. visually merging the strip with the terminal's top edge) rather than a new package — flagged
as a possible follow-up, not done here (out of scope of this change; no code changed, so nothing
to test beyond reading the existing `wmbar_*` goldens still pass unmodified).

## 2. Shell: `fish` (autosuggestions + completion, zero config)

Checked for real against the same Alpine 3.24 aarch64 repo:

| Package | aarch64 available | Version |
|---|---|---|
| `fish` | yes | 4.6.0-r1 |
| `zsh` | yes | 5.9-r7 |
| `zsh-autosuggestions` | yes | 0.7.1-r0 |
| `zsh-syntax-highlighting` | yes | 0.8.0-r1 |
| `zsh-completions` | yes | 0.35.0-r0 |
| `tmux` | yes | 3.7c-r0 (fallback path, not used; see part 1) |

Both paths are available. `fish` is the better fit here, for the same "low effort, high value"
reason suspected going in:
- Autosuggestions (greyed-out completion of a previous command as you type, Right/End to accept)
  and context-aware tab completion are **on by default**, no plugin installation, no `.zshrc`
  wiring, no `source` lines to get right inside the image build.
- `zsh` needs the two extra packages installed *and* sourced from an `/etc/zsh/zshrc` (or
  `~/.zshrc`) the image would have to ship and maintain; one more moving part for the same
  outcome, and more to go wrong non-interactively inside `st` (`-e zsh` runs it as a login-ish
  shell, and `zsh-autosuggestions`/`zsh-syntax-highlighting` are typically wired through
  `~/.zshrc`, which depends on `$HOME` existing and being writable at first boot).
- Both are equally available in the repo, so this is a real "least moving parts for the same
  result" choice, not a forced one.

No interaction with the system keyboard/XIM was found or expected: per `docs/rust/UI.md` ("System
keyboard") and `docs/rust/CONTRACTS.md` ("Input method"), the shell's XIM server and the on-screen
keyboard operate at the X11 level (text input focus, `XSetICFocus`) and know nothing about what
process is running inside `st`; `fish` reads raw keystrokes from the pty like any other shell, so
there is nothing shell-specific to wire up for touch/keyboard. This cannot be fully confirmed
without running the actual touch keyboard against `fish` on the device (listed as unverified
below), but there is no code path where the shell choice could matter to XIM.

### What changed
- `scripts/build_rootfs.sh`: `fish` added to `GUI_PKGS` (next to `st`/`xterm`), deliberately **not**
  added to the `NO_PY` trim list, so both the Python-default and `WALPAD_NO_PYTHON=1` images get it.
- `config/rootfs/etc/walpad/apps.json` and `config/rootfs-nopython/files/etc/walpad/apps.json`:
  the `terminal` entry's `exec` gains `"-e", "fish"` (`st -e fish` instead of the user's login
  shell). This is scoped to the `exec` array of one app id only — root's actual login shell
  (`/etc/passwd`) is **not** changed, so SSH (`dropbear`), `walpad-kali run`, and every shell
  script that execs over SSH keep using `ash`/`sh` exactly as before. Changing the system-wide
  login shell to `fish` was considered and rejected: it would also apply to every SSH session and
  to scripts that assume POSIX `sh`/`bash` syntax (`scripts/deploy.sh`, `tests/shell/run.sh`'s own
  assumptions when it execs over the device), for no benefit (those are not interactive sessions
  that need autosuggestions).
- `kali` app's `exec` is **unchanged** (`st -e /usr/local/sbin/walpad-kali shell`). The Kali
  profile is a separate NetHunter (Debian) chroot (`config/rootfs/usr/local/sbin/walpad-kali`),
  not built by `scripts/build_rootfs.sh`'s Alpine package list at all; `fish` is not installed
  inside that chroot, and adding it there would need `apt`, a chroot-local config, and the chroot
  tarball to already be unpacked (`walpad-kali install`), which is optional and may not have run.
  Out of scope: the owner's request was about "the terminal", and Kali's shell is a deliberate,
  separate security/learning environment the owner did not ask to change.

## Unverified (needs the owner's real device, not done here)

- **Terminal tabs**: whether the already-shipped `wmbar` strip is actually visible and usable on
  the real touch panel requires the device to be running an image built after
  `multitasking-windows` (2026-11-11) — this change did not flash anything. If the owner's tablet
  predates that, a rebuild + flash (owner-approved) is the actual next step, not new code.
- **`fish` + touch/on-screen keyboard**: typing, autosuggestion acceptance (Right arrow / End) and
  tab completion with the system's on-screen keyboard and XTEST key injection have not been tried
  on the tablet. Nothing in the XIM/keyboard code path is shell-aware, so no interaction is
  expected, but this is a real-device check, not something provable from the host.
- Neither of the above was reachable without a full `scripts/build_rootfs.sh` rebuild and a flash,
  which this change deliberately does not do (owner approval required, per `rules/device-safety.md`).
