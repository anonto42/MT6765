use crate::keyboard::Stroke;
use walpad_ui::edit_bar::{
    bar_size, clip_height, clip_layout, EditBtn, BAR_GAP, CLIP_HEAD_H, CLIP_ROW_H, CLIP_W, MARGIN,
    TERMINAL_BUTTONS, TEXT_BUTTONS,
};
use walpad_ui::Rect;
use walpad_x::XConn;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};

pub const BAR_CLASS: &str = "walpad-seltools";
pub const LIST_CLASS: &str = "walpad-clipboard";
/// The toolbar stays this long after the last selection change when no text input has the focus.
pub const SELECTION_MS: u64 = 10_000;
/// The clipboard list closes itself after this long (a tap outside cannot be seen).
pub const LIST_MS: u64 = 15_000;

/// Terminal emulators: Ctrl+C interrupts, paste is Ctrl+Shift+V. "term-st"/"kali-st"/
/// "scanner-st" are this project's own `st -c` class names (2026-11-12, change
/// terminal-kali-wm-class; 2026-11-13, the Scanner app): st's default class ("st" or
/// "st-256color" depending on build) is kept too in case a future app reuses it.
const TERMINALS: &[&str] = &[
    "st",
    "st-256color",
    "term-st",
    "kali-st",
    "scanner-st",
    "xterm",
    "uxterm",
    "urxvt",
    "rxvt",
    "kitty",
    "alacritty",
    "xfce4-terminal",
    "lxterminal",
];

/// Is the window with this WM_CLASS a terminal?
pub fn is_terminal(class: &[String]) -> bool {
    class
        .iter()
        .any(|c| TERMINALS.iter().any(|t| c.eq_ignore_ascii_case(t)))
}

/// The paste shortcut: Ctrl+V, or Ctrl+Shift+V for a terminal (Ctrl+V there inserts a literal
/// ^V).
pub fn paste_stroke(terminal: bool) -> Stroke {
    Stroke {
        shift: terminal,
        ..Stroke::ctrl('v' as u32)
    }
}

/// The keystroke of a toolbar button for the app in front (None: the button is not a key, or not
/// offered there).
pub fn button_stroke(b: EditBtn, terminal: bool) -> Option<Stroke> {
    let ch = |c: char| Some(Stroke::ctrl(c as u32));
    match (b, terminal) {
        (EditBtn::Paste, t) => Some(paste_stroke(t)),
        (EditBtn::Copy, false) => ch('c'),
        (EditBtn::Cut, false) => ch('x'),
        (EditBtn::SelectAll, false) => ch('a'),
        (EditBtn::Undo, false) => ch('z'),
        _ => None,
    }
}

/// WM_CLASS of the window in `_NET_ACTIVE_WINDOW` (empty when there is none).
pub fn active_class(xc: &XConn, active_atom: u32) -> Vec<String> {
    let win = xc
        .conn
        .get_property(false, xc.root, active_atom, AtomEnum::WINDOW, 0, 1)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().and_then(|mut v| v.next()))
        .unwrap_or(0);
    if win == 0 {
        return Vec::new();
    }
    xc.conn
        .get_property(false, win, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|p| {
            String::from_utf8_lossy(&p.value)
                .split('\0')
                .filter(|x| !x.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// The toolbar window: left margin, directly above whatever sits at the bottom (keyboard, mouse
/// pad, or the screen edge); the dock handle is at the right end, so they never overlap.
pub fn bar_rect(area: (i32, i32), bottom_h: i32, buttons: usize) -> Rect {
    let (w, h) = bar_size(buttons);
    Rect::new(MARGIN, area.1 - bottom_h - BAR_GAP - h, w, h)
}

/// The clipboard list: above the toolbar slot, at most as many rows as fit.
pub fn list_rect(area: (i32, i32), bottom_h: i32, entries: usize) -> Rect {
    let bar_y = bar_rect(area, bottom_h, TEXT_BUTTONS.len()).y;
    let fit = ((bar_y - BAR_GAP - CLIP_HEAD_H) / CLIP_ROW_H).max(1) as usize;
    let rows = entries.min(fit);
    let h = clip_height(rows);
    let w = CLIP_W.min(area.0 - 2 * MARGIN);
    Rect::new(MARGIN, (bar_y - BAR_GAP - h).max(0), w, h)
}

pub(super) fn buttons(terminal: bool) -> &'static [EditBtn] {
    if terminal {
        &TERMINAL_BUTTONS
    } else {
        &TEXT_BUTTONS
    }
}

pub(super) fn list_clip_layout(w: i32, shown: usize) -> Vec<(Rect, walpad_ui::edit_bar::ClipHit)> {
    clip_layout(w, shown)
}

/// When the toolbar is shown (pure).
#[derive(Debug, Default, Clone)]
pub struct BarState {
    selection_until: Option<u64>,
    /// The shell's own screen is in front: no toolbar over it.
    pub home: bool,
    /// The dock's strip is out: it covers the toolbar's place.
    pub dock_out: bool,
}

impl BarState {
    /// Something was selected (PRIMARY changed owner).
    pub fn selection(&mut self, now: u64) {
        self.selection_until = Some(now + SELECTION_MS);
    }
    /// The selection is gone, or an action used it up.
    pub fn clear_selection(&mut self) {
        self.selection_until = None;
    }
    pub fn visible(&self, now: u64) -> bool {
        !self.home && !self.dock_out && self.selection_until.is_some_and(|t| now < t)
    }
    /// When the visibility may change by itself.
    pub fn deadline(&self) -> Option<u64> {
        self.selection_until
    }
}

/// What the windows ask the shell to do (the shell owns the clipboard keeper).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditOut {
    /// Terminal Copy: PRIMARY -> clipboard.
    CopyPrimary,
    /// Paste history entry `i` (the shell makes it the clipboard, then calls `send_paste`).
    Paste(usize),
    ClearHistory,
    /// A toolbar action ran: a selection-only toolbar is done.
    Acted,
}
