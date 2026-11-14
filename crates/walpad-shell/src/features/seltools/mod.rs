//! Selection toolbar and clipboard list (owner request 2026-11-10: "I'm unable to copy and paste
//! anything"): two small windows the shell owns, over the apps.
//!
//! - `walpad-seltools`: Cut, Copy, Paste, Select all, Undo (a terminal in front gets only Copy and
//!   Paste: Ctrl+C there is an interrupt). Shown for a while after something was selected (PRIMARY
//!   changed owner: a long-press selection, a text selection in a browser), just above the
//!   keyboard or at the bottom edge - NOT just because a text input has the focus (owner report
//!   2026-11-13: "i don't need [it on top of the keyboard] becose our latest sytem of copy will
//!   cover thows" - merely focusing a field used to show it with nothing to act on yet).
//! - `walpad-clipboard`: the clipboard history (Apps panel > Tools > Clipboard); a tap on an entry
//!   pastes it.
//!
//! Same pattern as the keyboard and the mouse pad: override-redirect windows that never take the
//! focus (WM_HINTS input=false; clicks on an override-redirect window do not move the focus) and
//! type into the focused app through XTEST on the shell's own X connection. They are shell
//! windows (WM_CLASS `walpad-*`), so the core's touch bridge treats a touch on them as a mouse
//! touch, exactly like the keyboard. Actions run on release over the button that was pressed.
mod events;
mod list;
mod model;
#[cfg(test)]
mod tests;
mod windows;

pub use model::{
    active_class, bar_rect, button_stroke, is_terminal, list_rect, paste_stroke, BarState, EditOut,
    BAR_CLASS, LIST_CLASS, LIST_MS, SELECTION_MS,
};
pub use windows::EditBar;
