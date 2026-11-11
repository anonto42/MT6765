## 1. Host
- [x] 1.1 Pure geometry in `walpad-proto::wm` (work area, tiles, snap, move/resize clamp, slots) with unit tests
- [x] 1.2 IPC parsing tests for the new commands (`walpad-proto`), CONTRACTS.md section
- [x] 1.3 Multi-instance apps in the core (`max_instances`, instance numbers, pid to window), tests
- [x] 1.4 X side: window list with geometry and title, work area, move/resize, state, Xvfb test
- [x] 1.5 Layout engine tick (split/free), rotation re-tile, core integration tests
- [x] 1.6 Shell window bar (tabs, snap, move, grip), Settings rows, goldens
- [x] 1.7 `scripts/rust.sh fmt`, `scripts/check.sh` prints ALL CHECKS PASSED

## 2. Device (needs the tablet; no flashing)
- [ ] 2.1 Three terminals, tabs, close one tab, the others stay
- [ ] 2.2 Split in both orientations, ratio, swap, rotation re-tile
- [ ] 2.3 Free windows: move, resize, snap, keyboard strut respected
- [ ] 2.4 Memory check with three terminals and two apps tiled
