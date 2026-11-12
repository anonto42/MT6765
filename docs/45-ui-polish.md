# UI polish: remove box-in-box borders (owner request 2026-11-12)

Owner request (verbatim intent): remove the borders on the scrollable UI styling, trim unneeded
spacing, and make the shell read like a professionally designed product rather than a first pass.

## What the screenshots showed

Looking at the goldens (Settings, Device, Mobile network, Bluetooth, GPS, VPN, App store) before
this change, every full-screen page had the same problem: `Painter::page_surface` drew a 1 px
outline (`PAGE_BORDER`, white 22 %) around the whole scrollable content area, and *every* card or
row-group inside it (`Painter::card`, used by `row_group` and by every standalone info card: the
SIM/Wi-Fi/GPS/VPN/Bluetooth state cards, the App store's Documents/Media/Office groups, Bluetooth's
My devices/Available devices) drew its own 1 px `CARD_BORDER` (white 50 %) on top of its `SURFACE`
fill. Rows inside those cards already had their own hairline dividers (`list_row`/`network_row`
call `Painter::divider` under each row). The net effect on every single screen was boxes nested
inside boxes: an outer rounded-rect border for the page, an inner rounded-rect border for each
section, and a divider under every row inside it - exactly the "wireframe mock-up" look the owner
flagged, most visible in the App store (3 near-identical bordered boxes stacked) and Bluetooth/
Mobile network/VPN (one bordered state card followed by one bordered row-group, nested in the
bordered page).

## What changed

- `Painter::page_surface` (`crates/walpad-ui/src/components.rs`) no longer strokes `PAGE_BORDER`
  around the scrollable content area; it keeps the `PAGE_SURFACE` fill (black 45 % over the
  wallpaper). The `PAGE_BORDER` token is removed (nothing else used it).
- `Painter::card` no longer strokes `CARD_BORDER`; it keeps the `SURFACE` fill (black 55 %) and the
  pressed tint. This is the single shared card primitive, so the fix is one change in one place:
  every row-group (`row_group`), every standalone info/state card, and the App store/Recents tiles
  that reuse `card()` all lose their outline at once, with no call-site changes needed anywhere.
- Net result: a page now reads as one subtly-shaded surface over the wallpaper, with cards/
  row-groups one shade darker again, and hairline dividers between the rows inside a group - the
  grouped-section language iOS/Material settings use (background colour difference + dividers
  between items, never a box around the whole group), which is what the owner asked for.

## What was kept, and why

Not every outline was removed; each use was judged on its own merits per the task brief:

- **Popovers** (`Painter::popover`: Quick Settings, the Apps panel, sheets, dialogs, the power
  menu) keep their `CARD_BORDER` outline. These float over arbitrary content (the Home grid, an
  app, the wallpaper), not over a `PAGE_SURFACE`-shaded page, so there is no background-shade
  difference to lean on; the outline is what reads as "this is a layer above the page," the exact
  "a modal's edge against the page" case the brief called out as correct to keep.
- **Buttons, text fields, the segmented control, toggles, icon boxes/outline boxes, the header
  pill** keep their outlines: these are interactive controls in the line-art theme (the owner's
  established visual language), not passive grouping containers, and the brief explicitly flagged
  "a focused input field" as a keep-case.
- The dock/keyboard/mouse-pad/window-bar chrome strips (`dock.rs`, `mousepad.rs`, `wmbar.rs`) keep
  their direct `CARD_BORDER` strokes: separate override-redirect OS-chrome windows, out of scope
  for this pass (scope guard: touch only `walpad-ui`/screen-rendering code, not window-manager
  chrome) and not part of the "scroll UI" the owner was pointing at.

## Spacing and typography audit

Checked `tokens.rs`'s `SP_1..SP_7` usage and `Face`/`SIZE_*` usage for row titles/subtitles/section
labels across `panels.rs`, `phone_ui.rs`, `media.rs`, `gps_screen.rs`, `mobile.rs`, `vpn_screen.rs`,
`bluetooth.rs`: titles consistently use `Face::Bold`/`SIZE_BODY` (or go through the shared
`section_label`/`screen_title` helpers), subtitles `SIZE_SECONDARY`, captions `SIZE_CAPTION`; the
two literal large-mono sizes (GPS coordinates, the Bluetooth pairing passkey) are deliberate,
spec'd exceptions for a single big readout, not drift. No inconsistent or redundant padding stack
was found worth touching without risking hit-test regressions in shared layout code the other
in-flight agents (notifications, headphone/mic) also touch; this pass stayed to the token-level
border removal above, which already reaches every screen.

## Verification

- `UPDATE_SNAPSHOTS=1 scripts/rust.sh test -p walpad-shell` and `-p walpad-ui`: all tests pass; 99
  shell goldens regenerated (every screen that uses `card`/`row_group`/`page_surface`) plus the
  `walpad-ui` component-harness goldens.
- Personally viewed (before/after) and confirmed no overlap/regression, text stays readable, and
  the page reads flatter/more deliberate: `settings_list`, `device_portrait`, `quick_settings_portrait`,
  `bluetooth_devices_portrait`, `mobile_registered_data_on`, `home_portrait`, `media_music_portrait`,
  `gps_fix`, `vpn_connected_portrait`, `apps_panel_portrait`, `store_browse`, `recents_portrait`.
- `scripts/rust.sh fmt`: no changes needed. `scripts/check.sh`: see commit message / handoff for the
  result of this run.
