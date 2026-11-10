# WalPad Shell UI spec: line-art theme (v2, 2026-11-10; changes go through the owner)

The owner's design (HTML mock-up) translated to the native renderer (tiny-skia + fontdue, DejaVu).
One charcoal theme, white line-art controls, a sidebar dock on the left, a header with the clock and
a quick-settings pill, and a faint iced-coffee watermark behind the main area. Touch only: the
pressed state replaces hover. Reboot returns to Home; leaving a subpage returns to its screen.

## Colour tokens (crates/walpad-ui/src/tokens.rs)
Background `#393E46` + the coffee wallpaper. "White at N %" stroke/text colours are pre-blended over
the background at compile time (`white(pct)`): opaque writes. Surfaces get a translucent dark fill
(`Tint`, owner request 2026-11-10) drawn with real alpha blending over whatever is beneath (the
wallpaper art), so a component always reads apart from the background; the white outline stays.
No gradients or shadows.

| Translucent token | Value | Use |
|---|---|---|
| SURFACE | black 55 % | cards, row groups (Settings, Device), buttons, fields, segmented controls, header pill, dock buttons, pad surface/buttons, keyboard keys |
| SURFACE_STRONG | black 65 % | popovers (Apps panel, Quick Settings), sheets, dialogs, power menu, toasts, banners (+ tone 14 %), the sidebar's round Apps toggle; flattened over BG (opaque) for the dock / keyboard / mouse-pad strips |
| SURFACE_ICON | black 30 % | icon boxes (Home grid, sidebar, Apps panel), password-sheet keys |
| PRESSED_TINT | white 12 % | pressed state, drawn over the surface fill (outline turns white) |
| close badge | black 70 % | the Apps panel's close badge (it overlaps the icon box outline) |

Tuning (2026-11-10): the brief's 35 / 50 / 25 % left grey (55 %) text unreadable where cards and
panels cross the white cup art at the bottom of the screen (checked in the goldens), so cards went to
55 % and panels to 65 %. The dock, keyboard and mouse pad are separate X windows over other apps (no
compositor): their strip is SURFACE_STRONG over BG flattened to one opaque colour (`Tint::over`).

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
| PRESSED_FILL | 10 % | opaque pressed fill (legacy; surfaces use PRESSED_TINT) |
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
  with a chevron that toggles the Apps panel (right when closed, left when open). Every state of
  the round toggle is a circle (circle fill, circle pressed tint, circle outline); a pressed sidebar
  button is drawn over the bare background (cached sidebar strip without buttons), never over a
  square of colour, so nothing outside the circle changes and releasing restores the frame. The current
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
| Mouse pad | sidebar chevron > Apps panel > Tools > Mouse pad |

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
- **Quick Settings** (popover 280 wide under the pill, right-aligned, SURFACE_STRONG fill, 1 px CARD_BORDER,
  radius 16): Wi-Fi Network (subtitle SSID / Not connected / Off / Paused (hotspot) / Unknown /
  "Failed: restart to retry"; the row opens Network, the switch at its right toggles Wi-Fi);
  Hotspot (globe icon, subtitle Off / No password set / Starting... / On, N devices / Failed; the
  row opens Network, the switch toggles the hotspot); Bluetooth (disabled, "Not supported on this tablet"); Battery (NN %) (opens Device;
  "Battery (~62%)" + "Estimated from voltage" in warning for an estimate, "Battery (unavailable)"
  when there is no reading, the charge state for a gauge); divider; Brightness label + percent and
  the slider; divider; Rotation: Auto/Locked, Screen off (disabled while the core is away). No
  Keyboard row: the keyboard opens where it is needed (a focused text input, dock button). Rows 56 high. The quick toggles keep it open; the two navigation rows and a tap outside
  close it.
- **Apps panel** (popover 320 wide, bottom-left next to the sidebar, 16 px from the bottom, padding
  20). Translucent (owner request 2026-11-10): SURFACE_STRONG fill, no scrim, 1 px CARD_BORDER
  outline (radius 16); no text plates (the panel fill keeps the text readable over the art); Home
  grid items that would sit under the panel are not drawn while it is open. "ACTIVE APPS": the running apps as 48x48
  outlined icons (or "No apps running"); each has a 20 px round close badge (x, white outline) on
  its icon's top-right corner, tapped through a 40x40 rectangle that stays clear of the icon
  centre: it sends `close_app` for that app id and the panel stays open. Tapping the icon focuses
  the app (launch = focus) and closes the panel; long press opens the app sheet. A divider, then
  "TOOLS": a generic 4-column grid of tools (`panels::TOOLS`: icon, label, action); the first is
  "Mouse pad". Installed apps are on Home and the sidebar, not here. Cells are 70x72 touch
  targets; a tap outside closes the panel.
- **Network**: title "Wi-Fi" + outlined radio chip (Up/Starting/Failed/Down, Off when switched off,
  Paused while an exclusive hotspot runs; Failed shows "Restart the tablet to retry Wi-Fi") and the
  Wi-Fi switch at the right (owner request 2026-11-10; off = "Wi-Fi is off" empty state, Scan
  disabled). At the bottom a Hotspot section: label + switch, an outlined card with Name and
  Password rows (value "Set"/"Not set"; each opens the password sheet as "Hotspot name" / "Hotspot
  password" with Save), then up to three status lines (On, N devices connected in success; internet
  shared / local network only / sharing unavailable in warning; "Wi-Fi client paused" in warning;
  Failed: reason in danger). Turning the hotspot on without a password opens the password sheet.
  Connected card (outlined: SSID, signal, IP mono,
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
- **Settings**: 64 px rows with outline icons, dividers and chevrons, each run of rows backed by a
  translucent card (`row_group`, also on Device); subpages have an
  outlined 48 px back button. Maintenance actions are outlined buttons; Restart tablet and Shut down
  use the danger style (danger outline and text on a faint danger fill).
- **Dialogs and power menu**: centred 560 wide, BG fill + CARD_BORDER, radius 16. Buttons outlined.
  Restart/Shut down need two taps ("Tap again to shut down" in the danger style for 3 s). Hardware
  focus = a 2 px white ring around the item. Volume keys move focus, a short power press activates,
  the menu closes after 20 s.
- **System dock (D3)**: 56 px charcoal strip with a top divider and three outlined pill buttons
  (Home, Keyboard, Close app in danger), only while another app is in front. Collapsible (owner
  request 2026-11-10): by default only an arrow handle shows, 56x28 at the right end of the bottom
  edge (8 px margin), the strip colour with a CARD_BORDER line-art outline, rounded top corners
  (radius 10, SHAPE) and a white chevron pointing up; no strut, so the app keeps its full height.
  A tap on the handle unfolds the strip with its strut; the handle then sits on top of the strip
  with the chevron pointing down. The strip folds again after an action (Home, Keyboard, Close
  app), after 5 s without a tap, or on a second tap on the handle. Pressed handle = PRESSED_TINT
  over its fill. With the keyboard or the mouse pad up, handle and strip sit directly above it.
  Both windows are override-redirect with WM_HINTS input=false (never focused). Goldens:
  `dock_handle_{portrait,landscape,portrait_pressed}`, `dock_expanded_{portrait,landscape}`.
- **System keyboard** (replaces matchbox-keyboard): override-redirect strip, full width, 30 % of
  the visible height at the bottom (384 portrait, 240 landscape), BG with a top divider, outlined
  keys (radius 8), pressed or latched = 10 % fill + white outline. Five rows, 12 key units wide:
  Esc Tab 1-0 / q-p Backspace / Ctrl a-l Enter / Shift z-m . Up / / ?123, minimise, space,
  Left Down Right (symbols layer: ! @ # ... and < > ` ~ \ ; : ' " , ?). Shift and Ctrl are
  one-shot. Dynamic (owner request 2026-11-10, `ime.rs`): the shell is the XIM input method
  (`@im=walpad`); it appears 150 ms after a text input gets the focus (Firefox URL bar and fields,
  GTK entries, the st window) if the focus stayed, and goes away 250 ms after the focus left every
  input (moving between two fields does not flicker). It is not shown just because an app is open.
  The minimise key hides it until the next focus-in; the dock Keyboard button and the core's
  `keyboard` command still toggle it by hand (opened by hand with no input focused, it stays until
  closed by hand). The terminal start only opens it as a fallback when no XIM client spoke within
  3 s (or the XIM server is not running). Going Home hides it. The dock moves directly above it.
  Opening it closes the mouse pad (and the other way round).
- **Mouse pad** (Apps panel > Tools; `mousepad.rs`): the keyboard's twin: override-redirect window
  (WM_HINTS input=false, never focused) + a managed 1x1 dock-type strut window, full width, 30 % of
  the visible height at the bottom, the dock directly above it. Inside: an outlined touch surface
  with a hint ("Drag to move the pointer", "Tap to click · hold for a right click") and a mini-map
  of the screen (up to 200 px high: the pad's band greyed, a crosshair and a white dot at the
  cursor), and a row of five outlined
  56 px buttons: Left click, Right click, Scroll up (button 4), Scroll down (button 5), minimise
  (mouse + chevron). The pad keeps a virtual cursor (starts in the middle of the area above the
  pad, clamped to that area: 800 x 896 portrait, 1280 x 560 landscape, so a click never lands on
  the pad). One-finger drag moves it relatively x1.5 once the finger left a 10 px slop (the whole
  travel then counts, fractions carry), low-passed (60 % of the pending travel per sample, the
  rest on release, so the total is kept); a single sample that jumps > 80 px is dropped (two that
  agree re-anchor without moving); every press re-anchors on the finger. Tap = travel < 20 px and
  released within 300 ms = left click at the cursor where the touch began (jitter is undone); held
  >= 500 ms within 20 px = right click (the core bridge's 150 ms lift delay is subtracted); buttons
  act on release over the same button. Our own cursor: a SHAPE-d override-redirect arrow window
  (WM_CLASS `walpad-cursor`, 24x38, empty input region so clicks pass through) follows the virtual
  cursor while the pad is open (the Xvfb cursor may not reach the panel); without the SHAPE
  extension the pad works without it. The core's touch bridge moves the real pointer to the finger, so after every pad event
  the shell puts it back on the virtual cursor with an XTEST motion (echoes of those warps are
  recognised and ignored) and clicks with XTEST at the cursor after the finger lifted. Hidden by
  its minimise button, by going Home (dock Home, or the shell becoming active), or by the keyboard
  opening. Only the mini-map is repainted while the cursor moves.

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
- Translucent fills blend only component rects (cards, rows, popovers, buttons) per frame, never a
  full-screen layer; the sidebar buttons at rest are blended once into the cached layer. Opaque
  colours keep tiny-skia's opaque fast path.
- The frame is presented whole through the existing PutImage path (present.rs).

## Interaction
Tap = release within 10 px and 500 ms; long press = 500 ms within 10 px; drag > 10 px scrolls (no
inertia) or moves the brightness slider. Pressed state on the next frame. Popovers (Quick Settings,
Apps panel) close on a tap outside; sheets and dialogs are modal. Hardware Up/Down/Return focus works
in the power menu.
