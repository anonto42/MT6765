# Multitasking: several terminals and PC-like windows

## Why

Owner request 2026-11-11 (verbatim, typos kept): "i don't have multitasking on the terminal and also on the the screen that i'm unable to run multipull app on one screen like a pc".

Today `launch terminal` only focuses the running `st`, and every window is maximised by openbox (`<maximized>yes</maximized>`), so only one app is visible.

## What changes

- Terminal: apps.json gets `max_instances` (terminal 6, Kali 3). `new_instance` starts one more instance; the core tracks instances by process id and window id. A shell "window bar" strip shows one tab per window of the app in front (tap = focus, x = close) and a "+" button.
- Window layout in walpad-core (`logic`-style pure math in `walpad-proto::wm`, X side in `xbridge.rs`): modes `single` (today), `split` (two apps side by side in landscape, stacked in portrait, ratio 1/3, 1/2 or 2/3, swap sides) and `free` (floating windows, move/resize, clamped to the work area, minimum size).
- Work area comes from the window manager's `_NET_WORKAREA` (struts of the dock, keyboard strip, mouse pad and the window bar), with the visible panel as fallback; rotation re-tiles.
- New IPC commands: `windows`, `new_instance`, `focus_window`, `close_window`, `window_rect`, `snap`, `layout` (CONTRACTS.md). Mode and ratio persist in `/etc/walpad/shell.json` (`windows_mode`, `split_ratio`) through `set_setting`.
- Shell: Settings > Input rows "Windows: Single/Split/Free", split ratio and swap; the window bar doubles as the title bar in free mode (drag = move, corner grip = resize, snap buttons left/right/top/bottom/full).

## Out of scope

A compositor, thumbnails of tiled windows, a draggable divider (the ratio is set by tap: 1/3, 1/2, 2/3), more than two tiles, tmux, window animations, multi-touch resize.

## Device risk

None: no partition write, no flash, no kernel module, no reboot. openbox is not restarted; windows are un-maximised and moved through EWMH requests only.

## Secrets impact

None.

## Verification on the tablet (not done by the agent that wrote this)

1. `ssh walpad`, `walpad-ctl launch terminal`; `walpad-ctl new_instance terminal` twice; `pidof st` shows three; the bar shows three tabs.
2. Settings > Input > Windows: Split; open Browser and Files; both are visible side by side (landscape) or stacked (portrait); rotate: they re-tile.
3. Windows: Free; drag the bar, drag the corner grip; the window never leaves the panel and never gets smaller than 320x240.
4. Open the system keyboard: the work area shrinks and tiles move above it.

## Open questions

- Does openbox honour `_NET_MOVERESIZE_WINDOW` and un-maximise on `_NET_WM_STATE` for the `decor no` windows of this image? (Expected yes; verified only against Xvfb without a window manager.)
- Does st keep its grid correct when resized by the window manager? (st handles ConfigureNotify.)
