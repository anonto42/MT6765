# WalPad Shell UI spec: line-art theme (v2, 2026-11-10; changes go through the owner)

The owner's design (HTML mock-up) translated to the native renderer (tiny-skia + fontdue, DejaVu).
One charcoal theme, white line-art controls, a sidebar dock on the left, a header with the clock and
a quick-settings pill, and a faint iced-coffee watermark behind the main area. Touch only: the
pressed state replaces hover. Reboot returns to Home; leaving a subpage returns to its screen.

## Colour tokens (crates/walpad-ui/src/tokens.rs)
Background `#2B3035` everywhere. "White at N %" is pre-blended over the background at compile time
(`white(pct)`), so every draw is an opaque write: no translucent layers, gradients or shadows.

| Token | Value | Use |
|---|---|---|
| BG | #2B3035 | background, popover/sheet fill |
| TEXT / ACCENT | #FFFFFF (100 %) | text, icons, pressed/active outlines, selected marks, slider fill |
| TEXT_LABEL | 90 % | app labels under icons |
| TEXT_SECONDARY | 55 % | secondary text, captions |
| TEXT_DISABLED | 32 % | disabled text and icons |
| OUTLINE | 60 % | resting outline of buttons and icon boxes |
| CARD_BORDER | 50 % | cards, popovers, sheets, dialogs |
| DIVIDER | 30 % | dividers, sidebar edge, unlit signal bars, disabled outlines |
| PRESSED_FILL | 10 % | pressed fill |
| WATERMARK | 15 % | the coffee watermark |
| SUCCESS / WARNING / DANGER | #6DBE82 / #D9B25A / #E0736B | states only (connected, estimate, errors, destructive) |

## Type (px, DejaVu Sans; "light/medium" of the mock-up are approximated by Regular/Bold)
Header clock Regular 20 | Screen title Regular 26 | Section label Bold 13 uppercase 55 % | Body/row 17 |
Secondary 14 | Caption 13 | App label 12 (90 %) | Tiny caption Bold 11 uppercase ("ACTIVE APPS") |
Logs/IPs DejaVu Sans Mono 15.

## Shape
Spacing 4 8 12 16 24 32 48. Outline 1.5 px (cards and popovers 1 px). Radius 12 buttons/fields/rows,
16 icon boxes, cards, popovers, sheets; pills fully round. Touch target >= 56x56; rows 64 (Wi-Fi 72);
buttons 56 high. Icons: 24 px grid, round 2 px strokes (28 px in 64 boxes).

## Frame (crates/walpad-ui/src/layout.rs)
| Region | Portrait 800x1280 | Landscape 1280x800 |
|---|---|---|
| Sidebar | x 0-96, full height, right edge 1 px DIVIDER | same |
| Header | x 96-800, y 0-64 | x 96-1280, y 0-64 |
| Content | x 96-800, y 64-1280 (24 px side padding), vertical scroll | x 96-1280, y 64-800 |

- **Sidebar** (top to bottom, 56x56 outlined rounded squares, 20 px from the top, 12 px gaps):
  Home; the pinned apps (shell.json `pinned_apps`, at most four, the Settings shortcut excluded);
  a short 40 px divider; Settings; a flexible spacer; at the bottom a round 56 px outlined button
  with a chevron that toggles the Apps panel (right when closed, left when open). The current
  destination (Home, Settings, or the toggle while the panel is open) has a full-white outline.
  Touch areas span the full 96 px width. Pinned apps that would not fit are dropped, never Settings.
- **Header**: clock at the left (24 h or "2:22 PM" per Settings > Clock) with the date in secondary;
  at the right a 40 px pill: Wi-Fi mark (white connected, 32 % not connected/unknown, danger failed),
  battery percent + battery icon (warning colour when low; "~62%" for a voltage estimate, "--" when
  unavailable), chevron-down. Tapping it opens Quick Settings.
- **Banners** (system service not responding, battery low): 48 px rounded strip at the top of the
  content, tone-tinted fill + tone border; the content moves down 56 px.
- **Toast**: one at a time for 3 s, a 48 px pill centred under the header (tone border, icon, text).

## Destinations (how every screen is reached by touch)
| Screen | Reached from |
|---|---|
| Home (app grid) | sidebar Home |
| Apps panel | sidebar chevron (popover over any screen) |
| Network (Wi-Fi) | header pill > Quick Settings > Wi-Fi Network |
| Device | header pill > Quick Settings > Battery, or Settings > Display |
| Settings (+ subpages) | sidebar Settings, or the Settings icon on Home / in the Apps panel |
| Power menu | long power press, or Settings > Maintenance > Power menu |

The old bottom navigation is gone. `Tab::Apps` is kept for compatibility and shows Home (the grid
is the app list); `Target::Nav(1)` is the Apps-panel toggle.

## Screens and popovers
- **Home** (no scroll): every installed app plus the Settings shortcut as 64x64 outlined icon boxes
  with the name (12 px, 90 %) underneath; 120 px cells, 32 px row gap, centred, 40 px below the
  header (5 columns portrait, 9 landscape). A running app has a small white dot in its box. Tap
  launches by id (apps.json); long press opens the app sheet (Close app if running, Pin to / Unpin
  from the sidebar, Cancel).
- **Watermark**: the mock-up's iced coffee (SVG viewBox 200x300: straw with three ticks, dome lid,
  rim, tapered body with a rounded bottom, inner rim, three rotated ice cubes, three waves), stroked
  3 px in WATERMARK, scaled to 62 % of the content area (aspect kept) and centred. It sits in the
  cached background layer, so it shows behind every screen at no per-frame cost.
- **Quick Settings** (popover 280 wide under the pill, right-aligned, BG fill, 1 px CARD_BORDER,
  radius 16): Wi-Fi Network (subtitle SSID / Not connected / Unknown / "Failed: restart to retry",
  opens Network); Bluetooth (disabled, "Not supported on this tablet"); Battery (NN %) (opens Device;
  "Battery (~62%)" + "Estimated from voltage" in warning for an estimate, "Battery (unavailable)"
  when there is no reading, the charge state for a gauge); divider; Brightness label + percent and
  the slider; divider; Rotation: Auto/Locked, Keyboard, Screen off (disabled while the core is
  away). Rows 56 high. The quick toggles keep it open; the two navigation rows and a tap outside
  close it.
- **Apps panel** (popover 320 wide, bottom-left next to the sidebar, 16 px from the bottom, padding
  20): "ACTIVE APPS" with the running apps as 48x48 outlined icons (or "No apps running"), a
  divider, "INSTALLED APPS" in a 4-column grid of 48x48 icons with 11 px names. Cells are 70x72
  touch targets. Tap launches (and closes the panel); long press opens the app sheet; a tap outside
  closes it.
- **Network**: title "Wi-Fi" + outlined radio chip (Up/Starting/Failed/Down; Failed shows "Restart
  the tablet to retry Wi-Fi"); no on/off toggle. Connected card (outlined: SSID, signal, IP mono,
  Disconnect). Scan button (outlined, spinner while scanning). Available networks in an outlined
  card: transparent 72 px rows with dividers (SSID, lock, 4-step signal bars white / success when
  connected). Saved networks with Forget (danger text, confirm dialog). Password sheet: full-width
  bottom sheet (BG, CARD_BORDER, handle), outlined field (white when focused, danger on error),
  show/hide, keyboard with outlined keys (special keys brighter, pressed key filled white, shift
  held shows the pressed fill), Cancel / Connect; errors "Wrong password" / "Timed out after 25 s".
- **Device**: Display (outlined brightness card with slider; rotation rows Auto/Portrait/Landscape/
  Portrait flipped/Landscape flipped with a white check; dim after; screen off after; off on cover
  close toggle); Battery (level + source, state, voltage, temperature, thresholds read-only); Touch;
  Hardware. Unavailable hardware stays labelled (warning/danger text), never hidden.
- **Settings**: transparent 64 px rows with outline icons, dividers and chevrons; subpages have an
  outlined 48 px back button. Maintenance actions are outlined buttons; Restart tablet and Shut down
  use the danger style (danger outline and text on a faint danger fill).
- **Dialogs and power menu**: centred 560 wide, BG fill + CARD_BORDER, radius 16. Buttons outlined.
  Restart/Shut down need two taps ("Tap again to shut down" in the danger style for 3 s). Hardware
  focus = a 2 px white ring around the item. Volume keys move focus, a short power press activates,
  the menu closes after 20 s.
- **System dock (D3)**: 56 px charcoal strip with a top divider and three outlined pill buttons
  (Home, Keyboard, Close app in danger), bottom strut only while another app is in front.

## Components and states
Outline box / icon box / round button (rest, active, pressed); Button primary (2 px white outline),
secondary (60 % outline), danger (danger outline + faint danger fill) x (default, pressed, disabled,
loading); app icon (default, pressed, running dot); card (outline, pressed fill); popover; StatusCard
(value, unknown, estimated, error); ListRow (default, pressed, disabled; accessories chevron, toggle,
value, action); Toggle (on = white pill with a dark knob, off = outline), Slider (4 px track, white
fill and knob), SegmentedChoice (selected = white outline + 10 % fill + bold); NetworkRow (open,
secured, connected, connecting); Sheet/Dialog; TextField + Keyboard; Toast/Banner (info, warning,
danger); Spinner, EmptyState, ErrorState; header clock and quick pill.

## Rendering and performance (the tablet renders in software on ARM)
- Redraw only on input, core events, the minute boundary and running timers (unchanged loop).
- The background (charcoal, watermark, sidebar edge and buttons at rest) is rendered once into a
  cached canvas keyed by size, sidebar contents, active destination and panel state; each frame
  starts with one memcpy of it. Only a pressed sidebar button is drawn on top per frame.
- Scrolled screens draw straight into the frame through a canvas origin offset (no off-screen
  canvas of the full content height, no copy); rows outside the frame are skipped, and the header
  strip is restored from the cache afterwards.
- All colours are opaque pre-blends; the only per-frame fills are the small pressed/popover areas.
- The frame is presented whole through the existing PutImage path (present.rs).

## Interaction
Tap = release within 10 px and 500 ms; long press = 500 ms within 10 px; drag > 10 px scrolls (no
inertia) or moves the brightness slider. Pressed state on the next frame. Popovers (Quick Settings,
Apps panel) close on a tap outside; sheets and dialogs are modal. Hardware Up/Down/Return focus works
in the power menu.
