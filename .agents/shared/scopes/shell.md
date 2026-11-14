# Scope: walpad-shell + walpad-ui (crates/walpad-shell, crates/walpad-ui)

tiny-skia + fontdue software UI in one X window; cached background (wallpaper + sidebar), only
component rects blend per frame. Separate override-redirect strips with 1x1 strut windows: dock
(`dock.rs` + `dock/`), system keyboard (`keyboard.rs` + `keyboard/`), mouse pad (`mousepad.rs` +
`mousepad/`). Screens: `app/` (`home`, `network`, `mobile`, `vpn_screen`, `gps_screen`,
`device_screen`, `settings_screen`, `panels/`), overlays. Spec:
`docs/rust/UI.md`; goldens in `tests/golden/` (look at them after UPDATE_SNAPSHOTS=1).
