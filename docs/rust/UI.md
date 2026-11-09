# WalPad Shell v1 UI spec (final for v1; changes go through the owner)

One dark theme, one accent, DejaVu Sans family, five tabs: Home, Apps, Network, Device, Settings.
Reboot returns to Home; leaving a subpage returns to its tab.

## Colour tokens (no gradients, transparency or shadows; elevation = surface_raised + 1 px border)
bg #111316 | surface #1A1D21 | surface_raised #22262B | border #2C3137 | text #E9ECEF | text_secondary #9BA4AE |
text_disabled #5D646C | accent #4FA89E | accent_pressed #3D8A81 | on_accent #0E1716 | success #6DBE82 | warning #D9B25A | danger #D9665E

## Type (px)
Clock ExtraLight 96 | Screen title Bold 28 | Section label Bold 15 uppercase text_secondary | Body/row title 18 |
Secondary 15 | Caption/nav label 13 | Logs/IPs DejaVu Sans Mono 15

## Spacing and shape
Scale 4 8 12 16 24 32 48. Radius 12 buttons/fields, 16 cards/sheets. Touch target >= 56x56; rows 64 (Wi-Fi 72); buttons 56 high; icons 24 grid, 2 px strokes.

## Frame
| Region | Portrait 800x1280 | Landscape 1280x800 |
|---|---|---|
| Status bar | y 0-40: left WALPAD (13 bold), right Wi-Fi icon+SSID, battery icon+percent | same, full width |
| Content | y 40-1192, 24 px padding, vertical scroll | y 40-712, grids 4 columns |
| Bottom nav | y 1192-1280, five 160 px items, active = accent on 64x32 surface_raised pill | y 712-800, five 256 px items |

## Screens
- **Home** (no scroll): date line + clock (minute updates); Wi-Fi card 364x112 (SSID or "Not connected", IP mono) and Battery card (percent with "Estimated from voltage" in warning when source=voltage_estimate, "Unknown" when null); "Apps" 2x2 tiles 364x160 (first four pinned+installed); quick controls: brightness slider 56 high, buttons Rotation (Auto/Locked), Keyboard, Screen off. No greeting/stats/news.
- **Apps**: installed apps only, 2 columns (4 landscape), tiles 364x148, 48 px icon + name, 6 px accent dot when running; long press sheet: Close app, Pin/Unpin. No search until >12 apps.
- **Network**: header "Wi-Fi" + radio chip (Up/Starting/Failed; Failed shows "Restart the tablet to retry Wi-Fi"); no on/off toggle. Connected card (SSID, signal, IP, Disconnect). Scan button (spinner, disabled while scanning); 72 px rows (SSID, lock, 4-step signal). Saved networks with Forget (confirm dialog). Password sheet: show/hide, built-in keyboard (10 keys of 72x64, 6 px gaps, shift/123/space/backspace), Cancel/Connect; errors "Wrong password" / "Timed out after 25 s".
- **Device**: Display (brightness; rotation Auto/Portrait/Landscape/Portrait flipped/Landscape flipped; dim after; screen off after 30 s/1 min/90 s/2 min/4 min/10 min/Never; off on cover close); Battery (level+source, state, voltage, temp, thresholds read-only); Touch (status; calibration "Not available yet"); Hardware chips (accelerometer, audio Unverified, Bluetooth, SD, USB OTG, Camera Unsupported).
- **Settings**: 64 px rows with chevrons: Display, Input (keys.json read-only), Power, Network diagnostics, Security (SSH, authorised keys, telnet scope), System, Clock (24 h), Maintenance (logs last 200 lines redacted, restart shell, restart tablet, shut down), About. No theme picker.
- **Power menu**: centred sheet 560 wide: Screen off, Restart, Shut down, Cancel (focused). Restart/Shut down need two taps (danger + "Tap again to shut down" for 3 s). Volume keys move focus, short power press activates, closes after 20 s.
- **System dock (D3)**: 56 px strip Home/Keyboard/Close app, bottom strut only while another app is in front.
- **Notices**: one toast at a time for 3 s (Wi-Fi disconnected/connected, app closed unexpectedly); banners: battery low, "System service not responding".

## Components and states
StatusBar/BottomNav (normal, active); Button primary/secondary/danger (default, pressed, disabled, loading); AppTile (default, pressed, running); StatusCard (value, unknown, estimated, error); ListRow (default, pressed, disabled); Toggle/Slider/SegmentedChoice; NetworkRow (open, secured, connected, connecting); Sheet/Dialog (open, confirm); TextField+Keyboard; Toast/Banner (info, warning, danger); Spinner, EmptyState, ErrorState.

## Interaction
Tap = release within 10 px and 500 ms; long press = 500 ms within 10 px; drag > 10 px scrolls (no inertia). Pressed state on the next frame. Transitions <= 120 ms (sheet slide, tab swap only). Hardware Up/Down/Return focus works on Home and the power menu.
