# Code editor app

## Why

Owner request 2026-11-12 (verbatim, typos kept): "now i want a ide on my os that can be geany or
lite xl chouse the best one for me". The owner's own lean, stated beforehand: prefer Lite XL for
its much lower CPU/RAM footprint (the tablet is already CPU-constrained: software-rendered X,
high load averages even just playing video) if it is realistically available/buildable for
Alpine aarch64; fall back to Geany otherwise. This change verifies real package availability
first (not a web guess) and documents the decision in `docs/46-ide.md`.

## What changes

- **Verified with the exact Alpine build mechanism `scripts/build_rootfs.sh` already uses**
  (`apk add --root ... --arch aarch64 -X .../main -X .../community` inside an `alpine:3.24`
  container): `lite-xl` IS a real, prebuilt Alpine 3.24 community package for aarch64
  (`lite-xl-2.1.8-r3`), not source-only. Installing it on top of the exact package set this
  project already ships (`WALPAD_NO_PYTHON=1 WALPAD_PRINT_PKGS=1 scripts/build_rootfs.sh`) adds
  only 2 new packages: `lite-xl` itself and `lua5.4-libs` (its only packaged runtime dependency
  not already present via `fontconfig`/`firefox-esr`). Its binary has 4 dynamic deps
  (`libc`, `libfreetype`, `liblua-5.4`, `libpcre2-8`) and no GTK/dbus/at-spi2/accessibility stack
  at runtime (Alpine's build statically links SDL2; the repo carries no separate `sdl2` package
  at all). `geany-2.1-r0` is also packaged and, because the GTK3 stack is already pulled in by
  `firefox-esr`/`pcmanfm`/`gpicview`, also adds only 1 new package at *install* time — but it
  still *runs* the full GTK3/glib/cairo/pango/atk/dbus/at-spi2/bubblewrap/seccomp stack (over 80
  packages, ~88 MiB fresh) at CPU/RAM cost every time it is open, unlike lite-xl.
- Decision: **Lite XL**, per the owner's own stated preference, now confirmed realistic
  (prebuilt package, trivial `apk add`, no from-source build). `scripts/build_rootfs.sh`
  `GUI_PKGS` gains `lite-xl` (both the Python and no-Python images; it needs no Python).
- New app id `code` in `config/rootfs/etc/walpad/apps.json` and
  `config/rootfs-nopython/files/etc/walpad/apps.json`: `exec: ["lite-xl"]`,
  `env: {"SDL_VIDEO_X11_WMCLASS": "lite-xl"}` (forces a known WM_CLASS instead of relying on
  SDL2's undocumented default/fallback), `wm_class: "lite-xl"`, `touch: "scroll"` (reasoned by
  analogy with other scrollable-text-view apps — pcmanfm, gpicview — **not verified**: no touch
  test of any SDL2 app has been done on this device; the touch bridge works at the X11 protocol
  level so it should reach lite-xl the same way it reaches Xlib/GTK clients, but this is
  unverified until an on-device test), `single_instance: true`, `open: true` with a **custom**
  `open_args: ["{path}"]` (NOT the default `["--", "{path}"]`: lite-xl's C main() copies argv
  verbatim into Lua's `ARGS` with no option parsing, and `core/init.lua` treats every
  non-directory `ARGS[i]` as a file to open — a literal `--` would open a new untitled document
  named `--` instead of being skipped).
- New `Icon::Code` in `crates/walpad-ui/src/icons.rs` (24 px grid, 2 px stroke, matching the
  existing vector style): `</>` angle brackets with a slash. Icon id `"code"` (alias `"editor"`).

## Out of scope

- Geany (not shipped; documented as the fallback if Lite XL turns out impractical on-device).
- Building SDL2 or Lite XL from source.
- Any change to `config/boot`, `tools/propshim`, `scripts/mtk_stack_up.sh`, or the CONNSYS section
  of `walpad-wifi`.
- Confirming the real WM_CLASS lite-xl reports and real touch behaviour on-device — both need a
  rebuilt, flashed image (owner's step, explicit approval, out of scope here); see
  `docs/46-ide.md` for what is verified vs. assumed.

## Device risk

None: no flashing, no partitions, no boot files touched. The new package only takes effect in a
future `scripts/build_rootfs.sh` rebuild + flash, which needs the owner's explicit approval. A
read-only on-device SSH sanity check (`apk update`/`apk search`, no install) was in scope if
straightforward, but this change was made from an isolated agent worktree with no
`build/ssh/config` and no network path to the tablet, so it was not done here (see
`docs/46-ide.md` for the exact commands left for the owner or a device-connected agent to run).

## Secrets impact

None.

## Verification

Host: `scripts/rust.sh fmt`, `scripts/check.sh` (fmt, clippy, workspace tests incl. the icon
unit tests, aarch64 cross build, qemu smoke, shell tests incl. `WALPAD_PRINT_PKGS` package-list
checks), `tests/shell/run.sh`. Package availability verified directly against the Alpine 3.24
aarch64 repos using the same docker/apk mechanism as `scripts/build_rootfs.sh` (see
`docs/46-ide.md` for the exact `apk` output).

On the tablet (not done here, needs a rebuilt image + the owner's explicit flash approval):
confirm `lite-xl` launches from the Apps panel, record its real `WM_CLASS` with `xprop` (or the
shell's own window list), and confirm whether a finger drag in its editor view behaves like the
`"scroll"` touch mode expects (tap = click, drag = scroll/fling) the same way it does in pcmanfm.
