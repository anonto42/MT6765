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
| SURFACE_STRONG | black 65 % | popovers (Apps panel, Quick Settings), sheets, dialogs, power menu, toasts, banners (+ tone 14 %), the sidebar's Apps toggle; flattened over BG (opaque) for popovers, the dock / keyboard / mouse-pad strips |
| SURFACE_ICON | black 30 % | icon boxes (Home grid, sidebar, Apps panel), password-sheet keys |
| PAGE_SURFACE | black 45 % | page surface behind every full-screen page (Settings and subpages, Network, Device, Mobile, Bluetooth, VPN, Apps store, Sound ...): `Painter::page_surface`, rect `page_rect(frame.content)` (flush edge-to-edge `PAGE_MARGIN = 0` under the header), radius 0, no outline (owner request 2026-11-12, `ui-polish` + boxy restyle). Drawn once into the cached background layer (`ChromeKey.page`, set for every scrolling screen), never per row; rows/cards keep their own SURFACE on top |
| PRESSED_TINT | white 12 % | pressed state, drawn over the surface fill |
| close badge | black 70 % | the Apps panel's sharp square close badge |

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

## Shape (2026-11-12 boxy restyle)
Spacing 4 8 12 16 24 32 48. Zero border radius (`RADIUS = 0`, `RADIUS_CARD = 0`, `RADIUS_ICON = 0`) and zero 4-sided box outlines (`rrect_stroke`) across all components: app icons, dropdowns/popovers, cards, buttons, toggles, sliders, segmented controls, keyboard keys, tabs, pills, chips, and badges are sharp `0 px` rectangular boxes. Visual hierarchy and state separation use surface contrast (`SURFACE` vs `SURFACE_STRONG`), `1 px` full-width hairline `DIVIDER` lines, and `2–3 px` accent/tone indicator bars (left bar for active sidebar/status chips/toasts/banners, bottom bar for buttons/text fields/selected segments/tabs). No unnecessary UI gaps (`PAGE_MARGIN = 0`, popovers dock flush against header/sidebar/screen edges with opaque `SURFACE_STRONG.over(BG)` fill, and dock/wmbar/segmented controls use full-height flush cells separated by `1 px` dividers). Touch target >= 56x56; rows 64 (Wi-Fi 72); buttons 56 high. Icons: 24 px grid, 2 px strokes (28 px in 64 boxes).

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
- **Header stats** (owner request 2026-11-11, change `top-bar-stats`; `topbar.rs`): a second 40 px SURFACE pill with a DIVIDER outline between the clock and the quick pill (right edge 12 px left of it), one chip per enabled item: a 12 px label and value ("CPU 23%", "RAM 1.2/2.0G", "ROM 12/54G", "↓1.2M ↑30K") with a 3 px mini bar under it (white, warning from 90 %; none for the network); "--" for a figure the core could not read. Nothing is drawn until the core has sampled something. Room is made in this order: the date goes first (the clock keeps the time and AM/PM), then RAM/ROM show percentages ("RAM 60%"), then the last chips drop out; nothing overlaps in 800x1280 or 1280x800 (`chips_never_overlap_the_clock_or_the_pill`). Settings > Top bar items has four switches (CPU use, Memory (RAM), Storage (ROM), Network speed; default CPU and RAM on). Goldens `topbar_*`, `settings_top_bar*`.
- **Header**: clock at the left (24 h or "2:22 PM" per Settings > Clock) with the date in secondary;
  at the right a 40 px pill: mobile signal bars (only while a SIM is present; white registered, dim
  otherwise), a Bluetooth mark (only while the controller is powered), Wi-Fi mark (white connected, 32 % not connected/unknown, danger failed),
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
| Mobile network (SIM) | header pill > Quick Settings > Mobile data, or Settings > Mobile network |
| GPS | header pill > Quick Settings > GPS, or Settings > GPS |
| Bluetooth | header pill > Quick Settings > Bluetooth, or Settings > Bluetooth |
| Device | header pill > Quick Settings > Battery, or Settings > Display |
| Settings (+ subpages) | sidebar Settings, or the Settings icon on Home / in the Apps panel |
| Power menu | long power press, or Settings > Maintenance > Power menu |
| Mouse pad | sidebar chevron > Apps panel > Tools > Mouse pad |
| Media (photos, videos, music, recent) | Home grid "Media", or Apps panel > Tools > Media (`Tab::Media`, scrolled screen; details in `docs/41-media.md`) |

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
  row opens Network, the switch toggles the hotspot); Mobile data (SIM icon, subtitle operator · tech /
  No SIM / SIM PIN required / Airplane mode / Modem starting... / Not available / Data failed, small
  signal bars left of the switch while a SIM is present; the row opens Mobile network, the switch toggles
  data); GPS (owner request 2026-11-11, map-pin icon; subtitle Off / Starting... / Stopping... / Searching... or "Searching, N satellites" / "Fix, within N m" / Failed; the row opens the GPS screen, the switch toggles the GPS and is dead while it starts or stops); Bluetooth (owner request 2026-11-10; subtitle Off / Starting... / On / Connected: <name> / N devices connected / Scanning... / Failed to start / Not available; the row opens Bluetooth, the switch toggles it); Battery (NN %) (opens Device;
  "Battery (~62%)" + "Estimated from voltage" in warning for an estimate, "Battery (unavailable)"
  Owner request 2026-11-11: the Battery row subtitle shows "~5 h 20 min left" / "~1 h 10 min to full" (rounded to 5 min), "calculating..." for the first minutes, nothing extra when unknown; the Device battery card has a Time left / Time to full row.
  when there is no reading, the charge state for a gauge); divider; Brightness label + percent and
  the slider; Volume (owner request 2026-11-10: speaker icon, label + percent / "Muted" / "Unavailable",
  the slider and at its right a round outlined mute button, the speaker crossed out and warning-coloured
  while muted; disabled while the core reports no sound control); divider; Rotation: Auto/Locked, Screen off (disabled while the core is away). No
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
- **Mobile network** (owner request 2026-11-10; `mobile.rs`): title + modem chip (Ready / Starting /
  Stopped / Failed / None). SIM card (SIM icon; operator bold or the SIM state: No SIM card, SIM
  locked, SIM blocked (PUK), Airplane mode; "4G · Registered" in success, roaming/searching in warning;
  4-step signal bars + dBm at the right while a SIM is present; "Data via <apn>" while data is on).
  "Enter SIM PIN" primary button while the SIM asks for a PIN (password sheet "SIM PIN", digits layer,
  Unlock; "Wrong PIN" from the core clears the field). CONNECTION group: Mobile radio switch (off =
  airplane mode), Mobile data switch (disabled without radio / unlocked SIM), Access point (APN) row
  (value or "Automatic"; opens the text sheet). Under it the data line: off / Connecting... / Connected,
  <ip> (success) / Failed: <reason> (danger). MODEM group: Firmware, IMEI "Hidden" with Show/Hide (asks
  `mobile_info` once; hidden again when the screen is left).
- **GPS** (owner request 2026-11-11; `gps_screen.rs`): title + state chip (Off / Starting / Stopping / Searching in warning / Fix in success / Failed in danger). State card (map pin, bold title "GPS is off" / "Searching for satellites" / "Position found" / "GPS failed"; the second line is the satellites used, the accuracy or the failure reason, e.g. "turn Wi-Fi on first"). One 64 px row with the GPS switch (dead while starting/stopping and offline). While a position is known (the last fix stays, marked "Last known position, 3 min ago" in warning once it is older than 10 s or not a fix): POSITION card with the coordinates in Mono 22 ("23.75123, 90.40988"), "Accuracy 7 m · Altitude 12 m", "Last fix just now / 12 s ago / 3 min ago" in success, and two 56 px buttons "Copy position" and "Copy map link" (the OpenStreetMap link `https://www.openstreetmap.org/?mlat=..&mlon=..#map=17/..`; copies go to the clipboard keeper and a toast; there is no URL launch, so the link is pasted into the browser). SATELLITES card: "N in view · M used in the fix" and a bar chart of up to 16 signal strengths (0-50 dB-Hz, strongest first, 4 px stubs for satellites without a signal), then a note (open sky, minutes for the first fix, Wi-Fi must be on). Header pill: a map-pin mark after the Bluetooth mark while the GPS is on (white with a fix, 32 % while it searches). Goldens `gps_*`, `quick_settings_gps_*`, `home_landscape_gps_mark`.
- **Bluetooth** (owner request 2026-11-10; `bluetooth.rs`): title + chip (On / Off / Starting / Stopped / Failed / None) and the switch at the right (like Wi-Fi). Off, not started or starting: one state card instead of the lists (a failure reason in danger above it). On: "MY DEVICES" (paired; 72 px rows with a device icon, name, "Connected" in success / "Paired"; the row is the Connect / Disconnect action, "Forget" in danger at the right asks first; a spinner and "Working..." while a request runs), "AVAILABLE DEVICES" with the Scan button (outlined, spinner while scanning, tap again to stop; discovery ends by itself after 30 s) and unpaired devices (name, signal bars from RSSI, "Tap to pair"). Both lists are sorted by name, never by signal (rows must not move under a finger). Pairing dialog (centred, 560 wide): Waiting (spinner, Close), numeric comparison (big mono passkey, "Don't pair" / "Pair"), keyboard passkey ("Type this on <name>", Cancel), failure (reason in danger, OK). Results of connect and pairing arrive as toasts. Goldens: `bluetooth_*`, `quick_settings_bluetooth_on`, `home_landscape_bluetooth_mark`.
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
- **Volume pill** (owner request 2026-11-10): a hardware volume key (core `audio` event, source `key`)
  shows a 280x48 capsule (SURFACE_STRONG, 1.5 px OUTLINE, speaker icon, bar, percent / "Mute") at 72 % of the
  content height, centred, for 1.5 s (each press restarts it). Slider changes from the shell show no pill.
- **Settings > Sound**: a slider row (icon, Volume, percent, 56 px slider), a Mute toggle, a "Play test tone"
  button (`volume_test`: 1 s 440 Hz through the default sink) and a Sound output info row (Speaker / No sound
  service). Same controls as Quick Settings; disabled while there is no sound control.
- **Settings**: 64 px rows with outline icons, dividers and chevrons, each run of rows backed by a
  translucent card (`row_group`, also on Device); subpages have an
  outlined 48 px back button. Maintenance actions are outlined buttons; Restart tablet and Shut down
  use the danger style (danger outline and text on a faint danger fill).
- **Dialogs and power menu**: centred 560 wide, BG fill + CARD_BORDER, radius 16. Buttons outlined.
  Restart/Shut down need two taps ("Tap again to shut down" in the danger style for 3 s). Hardware
  focus = a 2 px white ring around the item. Volume keys move focus, a short power press activates,
  the menu closes after 20 s.
- **Navigation gestures and Recents** (`app-switcher-navigation`): only a touch that starts in the 24 px
  edge zone at the bottom of the visible area (not above it, not on the dock handle's columns, not while the
  keyboard/pad/dock strip is up) is a navigation swipe: short up = Home, up and hold 300 ms = Recents; the
  rest of the screen keeps drag = scroll. Recents: SURFACE_STRONG backdrop, up to three cards across
  (larger when fewer, never wider than half the screen, more scroll sideways), each a card with a 8 px
  padded thumbnail of the screen's aspect (app icon in an icon box when there is none), a 56 px label strip
  (icon, name, 48 px close X) and the caption "Swipe a card up to close it"; empty: "No recent apps".
  Snapshots `recents_{portrait,landscape,dragging,empty,scrolled}`. The dock strip is Back, Home, Keyboard,
  Close app (four equal cells; `dock_*` goldens).
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

## Window bar and Windows mode (owner request 2026-11-11, `wmbar.rs`, `windows_ui.rs`)
- Settings > Input ends with the button "Windows: Single|Split|Free (tap to change)"; in Split two more buttons: "Split sizes: 1/2 + 1/2 | 1/3 + 2/3 | 2/3 + 1/3" (cycle) and "Swap the two sides". They send `layout` (CONTRACTS.md); the shell reads the saved mode from `/etc/walpad/shell.json`.
- Window bar: a 48 px strip (WM_CLASS `walpad-wmbar`, override-redirect, WM_HINTS input=false so the terminal keeps the keyboard focus and the system keyboard keeps typing into it) at the top of the visible area, with a managed 1x1 `walpad-wmbar-strut` window carrying a 48 px top strut. It is up while an app window is in front and either that app allows several windows (apps.json `max_instances` > 1: terminal, Kali) or the mode is Free; never over the shell's own screens.
  - Tabs (multi-window apps): one rounded tab per window of the app (all windows with its `wm_class`, by window id), label "Terminal N" (just "N" when the tabs are narrower than 120 px, at 6 tabs in portrait), the window in front outlined; tap = `focus_window`, the x at the right end of a tab = `close_window` (only that window), "+" = `new_instance` (hidden at the limit).
  - Free mode: the empty part of the bar is the title bar: drag = move the window in front (`window_rect`, at most every 70 ms, clamped by the shell like the core does, plus a final one on release); five snap buttons at the right end (left, right, top, bottom, full) send `snap`. A 56 px grip (`walpad-wmgrip`) sits in the bottom-right corner of the window in front: drag = resize (minimum 320x240, never past the work area).
  - The bar follows `_NET_ACTIVE_WINDOW` / `_NET_CLIENT_LIST` changes and refreshes every 500 ms while an app is in front. Goldens: `wmbar_*.png`.

## Lock screen (owner request 2026-11-10, `lock_screen.rs` / `lock_window.rs`)
Own window above everything (see CONTRACTS "Screen lock"), charcoal BG. Clock 88 px with the date under it, a prompt line ("Enter PIN", "Wrong PIN. 2 tries left", "Try again in 27 s" in danger). PIN: dots row (at least 4 slots, filled per digit) and a 3x4 pad of 128x96 keys with backspace and a check mark (portrait: stacked, landscape: clock left, pad right); the pad is dimmed and dead during a countdown. Password: field with show/hide above the shell's on-screen keyboard and an Unlock button. Pattern: 3x3 dots, drag, release submits (at least 4 dots; a jumped-over dot is included). Goldens `lock_*`. Settings > Security: Screen lock state, Set a PIN / Set a password (type twice), Change / Remove (current secret first), Lock now, Lock after screen off (cycles Immediately, 30 s, 1 min, 5 min, 15 min, 30 min), and a note that the USB recovery shell is not protected. A pattern lock is changed with `walpad-ctl lock-set` over SSH.

## Components and states
Outline box / icon box / round button (rest, active, pressed); Button primary (2 px white outline),
secondary (60 % outline), danger (danger outline + faint danger fill) x (default, pressed, disabled,
loading); app icon (default, pressed, running dot); card (fill only, no outline; pressed fill); popover; StatusCard
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

## Touch gestures and editing (owner request 2026-11-10: "think like a mobile device")
- **Gestures** (core, `logic/gesture.rs`; parameters in CONTRACTS touch.json): tap = left click at the touch point (sent when the lift is detected, no other delay); drag > 12 px = scroll, wheel button per 48 px of travel (finger down = wheel up 4, up = 5, finger right = 6, left = 7; axis locked when the first move is 2:1 dominant), clicks capped at 8 per call; release speed >= 0.25 px/ms continues as a fling decaying (tau 150 ms) over 400 ms, speed capped at 3 px/ms; a touch during a fling stops it and does not click; hold 500 ms within 10 px then lift = a double click at the touch point (word select, like Android's long-press-to-select, owner report 2026-11-13, change long-press-selects-text; was a right click before, traded away by the owner's own choice); hold then drag = button 1 pressed at the touch point, released at the lift (text selection); a second tap within 450 ms and 40 px is snapped onto the first click (double / triple click); two fingers = pinch as Ctrl + wheel (apart = up = zoom in) one click per 40 px, only when the panel reports two contacts (unverified on the hardware).
- **Modes**: per app `touch` in apps.json (scroll default; terminal = mouse so a drag selects). Shell windows are always mouse, so the sidebar, dock, keyboard, mouse pad, popovers, toolbar and list work as before.
- **Selection toolbar** (`walpad-seltools`, 52 px, 96 px buttons: Cut Copy Paste Select all Undo = Ctrl+X/C/V/A/Z through XTEST): left margin, 8 px above the keyboard / mouse pad / screen edge (the dock handle is at the right end). Shown while a text input has the XIM focus, and for 10 s after PRIMARY changed owner (a selection); hidden at Home, while the dock strip is out, after an action. A terminal in front gets only Copy (copies PRIMARY into the clipboard: Ctrl+C would interrupt) and Paste (Ctrl+Shift+V). Never takes the focus.
- **Clipboard** (Apps panel > Tools > Clipboard): list of the last 10 text entries (520 wide, 56 px rows, Clear, close; closes after 15 s); a tap pastes the entry (it becomes the clipboard, then Ctrl+V / Ctrl+Shift+V). The keeper keeps copied text alive after the source app closes. Text only, <= 64 KiB, never logged; a selection is not copied until Copy.
- **System keyboard**: bottom row is now ?123, hide, Alt (one-shot), space, Paste (Ctrl+V, Ctrl+Shift+V in a terminal), arrows; Ctrl, Esc, Tab, arrows as before.
- **Pointer**: the X cursor is hidden (XFixes HideCursor in the shell, plus a blank root cursor in the session): no pointer under the finger in normal use, including apps' own cursors (Firefox); while the mouse pad is open its `walpad-cursor` marker is the only cursor.
