# Scope: walpad-shell + walpad-ui (crates/walpad-shell, crates/walpad-ui)

tiny-skia + fontdue software UI in one X window; cached background (wallpaper + sidebar), only
component rects blend per frame. Separate override-redirect strips with 1x1 strut windows: dock
(`dock.rs`), system keyboard (`keyboard.rs`), mouse pad (`mousepad.rs`). Screens: home, network
(+hotspot), device, settings, quick settings and apps panel (`panels.rs`), overlays. Spec:
`docs/rust/UI.md`; goldens in `tests/golden/` (look at them after UPDATE_SNAPSHOTS=1).
