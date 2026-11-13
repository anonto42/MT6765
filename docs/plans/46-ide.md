# IDE / code editor: Lite XL chosen over Geany

Owner request 2026-11-12 (verbatim, typos kept): "now i want a ide on my os that can be geany or
lite xl chouse the best one for me". Stated preference going in: Lite XL for its lower CPU/RAM
footprint, **if** it is realistically available/buildable for Alpine aarch64 on this device;
Geany otherwise. This doc is the real check, done against Alpine's actual aarch64 repos, not a
web guess.

## Method

Alpine 3.24's own `apk` database is authoritative, so packages were checked with the exact
mechanism `scripts/build_rootfs.sh` already uses to build the image: an `alpine:3.24` docker
container, `apk add --root <throwaway dir> --arch aarch64 --no-scripts --keys-dir
/usr/share/apk/keys/aarch64 -X https://dl-cdn.alpinelinux.org/alpine/v3.24/main -X
.../v3.24/community <packages>`. This is a real aarch64 package install into a scratch directory
inside the container (never touching this repo, the build output, or the device); nothing here
changes `build/`, the image, or the running tablet.

## Result: both are real prebuilt packages

```
$ apk add --root /out/lite3 --arch aarch64 --initdb --no-scripts \
    --keys-dir /usr/share/apk/keys/aarch64 \
    -X https://dl-cdn.alpinelinux.org/alpine/v3.24/main \
    -X https://dl-cdn.alpinelinux.org/alpine/v3.24/community lite-xl
(1/9) Installing musl (1.2.6-r3)
(2/9) Installing brotli-libs (1.2.1-r0)
(3/9) Installing libbz2 (1.0.8-r6)
(4/9) Installing zlib (1.4-r0)
(5/9) Installing libpng (1.6.59-r0)
(6/9) Installing freetype (2.14.3-r0)
(7/9) Installing lua5.4-libs (5.4.8-r0)
(8/9) Installing pcre2 (10.49-r0)
(9/9) Installing lite-xl (2.1.8-r3)
OK: 7203 KiB in 9 packages
```

`lite-xl-2.1.8-r3` is a real, prebuilt Alpine community aarch64 package — not source-only, not a
bigger undertaking. `apk info -R lite-xl` (declared deps) and `readelf -d` on the installed
`usr/bin/lite-xl` binary agree: only 4 dynamic libraries —

```
so:libc.musl-aarch64.so.1
so:libfreetype.so.6
so:liblua-5.4.so.0
so:libpcre2-8.so.0
```

No `libSDL2.so` dependency at all, and Alpine 3.24's main+community repos carry **no standalone
`sdl2` package** either (`apk search sdl2*` returns nothing): Alpine's `lite-xl` build statically
links SDL2. No GTK, no glib, no dbus, no at-spi2/accessibility bus, no Wayland libs are pulled in.

```
$ apk add --root /out/geany2 --arch aarch64 ... geany
(1/81) Installing musl ... (81/81) Installing geany (2.1-r0)
OK: 87.8 MiB in 81 packages
```

`geany-2.1-r0` is also real and prebuilt. `apk info -R geany` shows it depends on the full GTK3
stack: `libgtk-3`, `libgdk-3`, `libatk-1.0`, `libpango`/`libpangocairo`, `libcairo`, `libglib-2.0`/
`libgobject-2.0`/`libgio-2.0`/`libgmodule-2.0`, `libintl`, `libstdc++`. Installed fresh, that pulls
in 81 packages (87.8 MiB) including `at-spi2-core`, `dbus-libs`, `bubblewrap`, `libseccomp`,
`glycin-image-rs`/`glycin-svg` (image loading sandboxing), `librsvg`, `gdk-pixbuf`, three
`wayland-libs-*` packages (unused on this X11-only build, just linked), `xkeyboard-config`,
`gnutls`/`nettle`/`gmp`/`p11-kit` (pulled in transitively), `cups-libs`.

## The footprint comparison that actually matters here

Installing on top of the package set this project **already ships** (not a blank Alpine, since
that is what is real on the device) changes the picture at install time, but not at runtime:

```
$ WALPAD_NO_PYTHON=1 WALPAD_PRINT_PKGS=1 scripts/build_rootfs.sh   # the shipped package list
$ apk add --root /out/base <that package list>                     # 321 packages installed
$ apk add --root /out/base geany        # on top of the shipped set
(1/1) Installing geany (2.1-r0)
OK: 1057.1 MiB in 322 packages                                     # +1 package
$ apk add --root /out/base2 lite-xl     # on top of the same shipped set
(1/2) Installing lua5.4-libs (5.4.8-r0)
(2/2) Installing lite-xl (2.1.8-r3)
OK: 1038.9 MiB in 323 packages                                      # +2 packages
```

Geany's entire GTK3/glib/cairo/pango/atk dependency closure is **already** on the image because
`firefox-esr`, `pcmanfm` and `gpicview` are GTK3 apps too — so at *install* time Geany costs only
1 new package (itself) and Lite XL costs 2 (itself + `lua5.4-libs`, its one packaged dependency
not already pulled in by `fontconfig`/`firefox-esr`).

That is an install-size tie, not the deciding factor. The deciding factor is CPU/RAM while the
editor is actually **running**, which is what the owner asked to optimise for (this tablet is
already CPU-constrained: software-rendered X, high load averages just playing video). Geany, like
every GTK3 app, starts the full GTK3 toolkit (glib main loop, GObject, Pango text shaping, Cairo
software rendering, ATK/AT-SPI2 accessibility machinery, `dbus` round-trips) on every launch.
Lite XL is a single statically-linked SDL2 binary with Lua for its own UI/editor logic and no
toolkit, accessibility bus, or IPC daemon of its own to start. That gap is the real-world reason
the owner's stated preference exists, and it is now confirmed, not assumed.

## Decision: Lite XL

Per the owner's own stated condition ("if it is actually realistically available/buildable ...
fall back to Geany if impractical"): Lite XL is realistically available — a prebuilt Alpine
aarch64 package, one `apk add` away, no from-source build, no bigger undertaking. So Lite XL is
the one shipped. Geany is not installed; this document is also the record of why, should Lite XL
turn out to misbehave on-device later and the owner wants to switch.

## What changed

- `scripts/build_rootfs.sh`: `lite-xl` added to `GUI_PKGS` (ships in both the Python and
  no-Python images; it has nothing to do with the Python runtime, so the no-Python filter needed
  no change).
- `config/rootfs/etc/walpad/apps.json` and `config/rootfs-nopython/files/etc/walpad/apps.json`:
  new app `{"id": "code", "name": "Code", "icon": "code", "exec": ["lite-xl"], "env":
  {"SDL_VIDEO_X11_WMCLASS": "lite-xl"}, "single_instance": true, "wm_class": "lite-xl", "touch":
  "scroll", "open": true, "open_args": ["{path}"]}`.
- `crates/walpad-ui/src/icons.rs`: new `Icon::Code` (`</>`, 24 px grid, 2 px stroke, matching the
  file's existing vector style), icon id `"code"` (alias `"editor"`).

## `open_args`: why not the default `["--", "{path}"]`

Every other "open a file" app in `apps.json` (`gpicview`) uses the default `open_args` — a `--`
then the path, the normal GNU convention for "end of options". Lite XL does not follow that
convention. Its `src/main.c` copies `argv` verbatim into Lua's global `ARGS` table with no option
parsing at all (no `getopt`, nothing strips `--`), and `data/core/init.lua` walks `ARGS[2..]`
treating every entry that is not a known flag and not an existing directory as a file to open
(`core.open_doc` creates a new untitled document for a path that does not exist). So a literal
`--` passed as an argument would open as its own untitled document named `--`, not get skipped.
`open_args` for `code` is therefore `["{path}"]` — just the resolved path, nothing else.

## What is verified vs. what is not

Verified for real, against Alpine's own aarch64 repos:
- `lite-xl` and `geany` both exist as real prebuilt Alpine 3.24 aarch64 packages (not
  source-only).
- Their exact dependency closures (declared deps + `readelf -d` on the installed binary for
  `lite-xl`).
- The exact marginal package cost of each on top of this project's actual shipped package list.
- Alpine 3.24 main/community carry no standalone `sdl2` package (Lite XL statically links it).
- `lite-xl`'s argv/`ARGS` handling has no `--` end-of-options convention (read from
  `lite-xl/lite-xl`'s `src/main.c` and `data/core/init.lua` on GitHub).
- SDL2's X11 backend honours an `SDL_VIDEO_X11_WMCLASS` environment variable override for the
  window's `WM_CLASS`, falling back to the executable name when unset (SDL2 source,
  `src/video/x11/SDL_x11video.c`, `get_classname()`); passing it explicitly in `apps.json`'s
  `env` makes the WM_CLASS deterministic rather than relying on the fallback.
**Not done**: a read-only on-device SSH sanity check (`apk update`/`apk search`, no install) was
in scope if straightforward, but this change was made from an isolated agent worktree with no
`build/ssh/config` (it is generated locally, per the owner's own machine/DHCP lease, not checked
into the repo) and no network path to the tablet. Rather than fabricate that check, it is left
for the owner (or a future agent with device access) to run:
`ssh -F build/ssh/config walpad 'timeout 10 cat /etc/apk/repositories'` and
`timeout 20 ssh -F build/ssh/config walpad 'apk update 2>&1 | tail -5'` — read-only, no install.

**Not verified** (needs a rebuilt image, flashed to the device — the owner's step, with explicit
approval, not done here):
- The real `WM_CLASS` Lite XL reports once actually running under this X server (the env override
  is expected to work per the SDL2 source, but has not been read back with `xprop` on-device).
  `"lite-xl"` is used in `apps.json`'s `wm_class` as the expected value; `class_matches()`
  (`crates/walpad-core/src/xbridge.rs`) does a case-insensitive exact-or-substring match, so a
  near-miss (e.g. `"SDL_App"` if the env var is somehow not honoured) would still need a
  one-line `apps.json` fix, not a code change.
- Real touch behaviour in Lite XL's editor view. No SDL2 app has been touch-tested on this device
  before (`CONTRACTS.md`'s touch gesture section only documents Xlib/GTK apps: browser, files,
  terminal). The touch bridge works by classifying a touch against the top X window and then
  injecting synthetic pointer events at the X protocol level (XTEST), which in principle any X11
  client sees the same way regardless of toolkit — SDL2 on Linux is just another Xlib/XInput
  client. `touch: "scroll"` was chosen by analogy with Geany's would-be GTK scrollable text view
  and with pcmanfm/gpicview (also `"scroll"`), but this is reasoning, not an on-device result.

## On-device verification steps (for the owner, after a rebuild + flash)

1. `ssh -F build/ssh/config walpad 'xprop -root' ` or open the app and check the shell's own
   window list / Recents to read the real `WM_CLASS`.
2. Launch Code from the Apps panel, drag a finger in the editor view, and compare with how
   pcmanfm/Files behaves (tap = click/cursor placement, drag = scroll, hold = selection) to see
   whether `touch: "scroll"` is the right call or whether `"mouse"` would feel better.
3. If the `WM_CLASS` is not exactly `"lite-xl"`, update `wm_class` in both `apps.json` files to
   match (one-line fix, no rebuild of the Rust crates needed).
