# Clipboard and touch text editing

## Why

Owner request 2026-11-10 (tablet completeness). X11 selections disappear when the owning app exits and a touch-only device has no way to Cut/Copy/Paste or select all.

## What changes

- The shell becomes the clipboard manager: it watches CLIPBOARD with XFIXES, takes ownership of text, serves TARGETS and UTF8_STRING after the source exits.
- In-memory history of 25 items (never written to disk, never logged), pin and clear; history sheet from a keyboard key and a Quick Settings row.
- Long-press in a text field opens a popup (Cut, Copy, Paste, Select all) that sends Ctrl+X/C/V/A via XTEST.
- Items from password-manager style hints are not recorded.

## Out of scope

Images or files in the clipboard, cross-device clipboard sync, drag and drop.

## Device risk

No flashing or reboot. Clipboard text is sensitive: RAM only, never in logs or argv.

## Verification

On the tablet: copy in Firefox, quit Firefox, paste in `st` (shift+insert or the popup); open the history sheet and check the entries; `grep -r` the log directory for a copied marker string finds nothing.

## Open questions

- Which apps request non-text targets and what do they do when we answer only text?
