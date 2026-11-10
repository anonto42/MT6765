# App switcher and navigation

## Why

Owner request 2026-11-10: make WalPad a complete tablet. Android/iPadOS users expect Back, Home and Recents; today the dock has Home/Keyboard/Close and the Apps panel lists active apps without a switcher.

## What changes

- Bottom-edge swipe in the shell gesture layer: short swipe = Home, swipe and hold = Recents (cards with window thumbnails taken with XGetImage and cached when a window loses focus).
- Swipe a card away to close that app (existing `close_app`); tap a card to focus (launch = focus).
- Back action: Alt+Left for browser/files, Esc otherwise; a Back button in the system dock; the hardware `keys.json` action `back` is added.
- Terminal toolbar (Ctrl/Alt/Esc/Tab/arrows) is out of scope here.

## Out of scope

Split view and floating windows (Phase 4), screen pinning, a compositor.

## Device risk

No flashing, no kernel modules, no reboot. Gesture code must not steal touches from apps (edge zone only).

## Verification

On the tablet: open Browser, Terminal and Files, swipe up and hold from the bottom edge, check three cards; tap one; swipe one away and confirm the process exits (`ssh walpad pidof st`).

## Open questions

- Is XGetImage of an unmapped/obscured window reliable without a compositor? Fall back to the app icon if not.
