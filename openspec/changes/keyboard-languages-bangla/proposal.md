# Keyboard languages, Bangla and emoji

## Why

Owner request 2026-11-10: a tablet for a Bangladeshi owner needs Bangla typing and emoji; the system keyboard has one QWERTY layout.

## What changes

- Layouts are data files in /etc/walpad/keyboards/*.json (rows, keys, label, commit string, shift variant): en-us, bn-phonetic (Avro style), bn-inscript.
- Language/globe key cycles layouts, long-press lists them, the choice is remembered per app class.
- Text that is not a Latin keysym is committed through the XIM server (XIM_COMMIT with UTF-8/COMPOUND_TEXT) instead of XTEST keycodes.
- Phonetic engine from the Rust crate riti (OpenBangla Keyboard, MPL-2.0) or an own table-driven port; shipped fonts: font-noto-bengali, font-noto-emoji.
- Emoji layer: grouped grid, recents row, search by short name.

## Out of scope

Predictive text and autocorrect (Phase 4), voice typing, handwriting, other scripts.

## Device risk

No flashing or reboot. Touches ime.rs and keyboard.rs, which the clipboard change also touches: implement clipboard-selection first.

## Verification

On the tablet: select Bangla, type `ami` in Firefox's search field (expect আমি), type in st (needs a UTF-8 capable font), switch back, open emoji and insert one.

## Open questions

- Does st render Bangla conjuncts correctly (shaping)? Else recommend Firefox/GTK apps for Bangla.
