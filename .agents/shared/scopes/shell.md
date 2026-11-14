# Scope: walpad-shell + walpad-ui (crates/walpad-shell, crates/walpad-ui)

Feature-Sliced Design (FSD) under `crates/walpad-shell/src/`: `app/` (`state/`, `nav`, `shell/`),
`features/` (`bluetooth/`, `call/`, `clipboard/`, `device/`, `dock/`, `gps/`, `home/`,
`keyboard/`, `lock/`, `media/`, `mobile/`, `mousepad/`, `network/`, `notifications/`, `panels/`,
`phone/`, `recents/`, `seltools/`, `settings/`, `store/`, `volume/`, `vpn/`, `windows/`, `wmbar/`),
and `shared/` (`config/`, `input/`, `ipc/`, `ui/`, `x11/`). Crate root (`lib.rs`) re-exports
module names for public/test compatibility.

tiny-skia + fontdue software UI in one X window; cached background (wallpaper + sidebar), only
component rects blend per frame. Separate override-redirect strips with 1x1 strut windows: dock
(`features/dock/`), system keyboard (`features/keyboard/`), mouse pad (`features/mousepad/`).
Screens and overlays live under `features/*` driven by `app/state/`. Spec: `docs/rust/UI.md`;
goldens in `tests/golden/` (look at them after UPDATE_SNAPSHOTS=1).
