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
use crate::dock::visible_area;
use crate::keyboard::{inject, load_keymap, KeyMap, Stroke, XK_CONTROL_L};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use walpad_ui::components::Painter;
use walpad_ui::edit_bar::{
    bar_layout, bar_size, clip_height, clip_layout, ClipHit, EditBtn, BAR_GAP, CLIP_HEAD_H,
    CLIP_ROW_H, CLIP_W, MARGIN, TERMINAL_BUTTONS, TEXT_BUTTONS,
};
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts, Rect};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    self, AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt as _, CreateGCAux,
    CreateWindowAux, EventMask, PropMode, StackMode, Window as XWindow, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

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

struct Win {
    id: XWindow,
    gc: u32,
    canvas: Canvas,
    mapped: bool,
}

pub struct EditBar {
    xc: XConn,
    shell: XWindow,
    keymap: KeyMap,
    area: (i32, i32),
    bottom_h: i32,
    active_atom: u32,
    active: u32,
    depth: u8,
    bar: Win,
    list: Win,
    /// A terminal is in front.
    terminal: bool,
    /// What the toolbar should show (the shell's rule), and what is mapped.
    wanted: bool,
    bar_pressed: Option<usize>,
    list_pressed: Option<ClipHit>,
    entries: Vec<String>,
    list_hits: Vec<(Rect, ClipHit)>,
    list_until: Option<Instant>,
}

fn buttons(terminal: bool) -> &'static [EditBtn] {
    if terminal {
        &TERMINAL_BUTTONS
    } else {
        &TEXT_BUTTONS
    }
}

fn make_window(xc: &XConn, class: &str, wm_class: &str, r: Rect) -> Result<Win, XError> {
    let conn = &xc.conn;
    let id = conn.generate_id()?;
    conn.create_window(
        0,
        id,
        xc.root,
        r.x as i16,
        r.y as i16,
        r.w as u16,
        r.h as u16,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new()
            .event_mask(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::EXPOSURE)
            .background_pixel(0x002B_3035)
            .override_redirect(1),
    )?;
    let gc = conn.generate_id()?;
    conn.create_gc(gc, id, &CreateGCAux::new())?;
    conn.change_property8(
        PropMode::REPLACE,
        id,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        format!("{class}\0{wm_class}\0").as_bytes(),
    )?;
    conn.change_property8(
        PropMode::REPLACE,
        id,
        AtomEnum::WM_NAME,
        AtomEnum::STRING,
        class.as_bytes(),
    )?;
    // WM_HINTS flags=InputHint, input=false
    conn.change_property32(
        PropMode::REPLACE,
        id,
        AtomEnum::WM_HINTS,
        AtomEnum::WM_HINTS,
        &[1u32, 0, 0, 0, 0, 0, 0, 0, 0],
    )?;
    Ok(Win {
        id,
        gc,
        canvas: Canvas::new(r.w as u32, r.h as u32, BG).ok_or("edit window canvas")?,
        mapped: false,
    })
}

impl EditBar {
    pub fn open(socket: &Path, shell: XWindow, shell_size: (i32, i32)) -> Result<Self, XError> {
        let xc = walpad_x::connect(socket)?;
        let keymap = load_keymap(&xc)?;
        let area = visible_area(shell_size.0, shell_size.1);
        let active_atom = xc
            .conn
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")?
            .reply()?
            .atom;
        let depth = xc
            .conn
            .setup()
            .roots
            .first()
            .map(|s| s.root_depth)
            .unwrap_or(24);
        let bar = make_window(
            &xc,
            BAR_CLASS,
            "WalPad-SelTools",
            bar_rect(area, 0, TEXT_BUTTONS.len()),
        )?;
        let list = make_window(&xc, LIST_CLASS, "WalPad-Clipboard", list_rect(area, 0, 0))?;
        xc.conn.change_window_attributes(
            xc.root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        xc.conn.flush()?;
        let mut e = EditBar {
            xc,
            shell,
            keymap,
            area,
            bottom_h: 0,
            active_atom,
            active: 0,
            depth,
            bar,
            list,
            terminal: false,
            wanted: false,
            bar_pressed: None,
            list_pressed: None,
            entries: Vec::new(),
            list_hits: Vec::new(),
            list_until: None,
        };
        e.read_active();
        Ok(e)
    }

    pub fn wait_fd(&self) -> &UnixStream {
        &self.xc.wait_fd
    }
    pub fn bar_window(&self) -> XWindow {
        self.bar.id
    }
    pub fn list_window(&self) -> XWindow {
        self.list.id
    }
    pub fn bar_visible(&self) -> bool {
        self.bar.mapped
    }
    pub fn list_visible(&self) -> bool {
        self.list.mapped
    }
    /// The shell's own screen is the active window (Home): no toolbar over it.
    pub fn home(&self) -> bool {
        self.active == self.shell && self.active != 0
    }
    pub fn terminal_mode(&self) -> bool {
        self.terminal
    }
    pub fn next_deadline(&self) -> Option<Instant> {
        self.list_until
    }
    pub fn bar_geometry(&self) -> Rect {
        bar_rect(self.area, self.bottom_h, buttons(self.terminal).len())
    }

    fn read_active(&mut self) {
        let class = active_class(&self.xc, self.active_atom);
        self.terminal = is_terminal(&class);
        self.active = self
            .xc
            .conn
            .get_property(
                false,
                self.xc.root,
                self.active_atom,
                AtomEnum::WINDOW,
                0,
                1,
            )
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().and_then(|mut v| v.next()))
            .unwrap_or(0);
    }

    /// The toolbar and the list follow `wanted` and the list state; call after any change.
    pub fn set_wanted(&mut self, on: bool, fonts: &mut Fonts) -> Result<(), XError> {
        if on == self.wanted && on == self.bar.mapped {
            return Ok(());
        }
        self.wanted = on;
        self.apply_bar(fonts)
    }

    fn apply_bar(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        let on = self.wanted;
        let r = self.bar_geometry();
        if on {
            self.bar.canvas = Canvas::new(r.w as u32, r.h as u32, BG).ok_or("toolbar canvas")?;
            self.xc.conn.configure_window(
                self.bar.id,
                &ConfigureWindowAux::new()
                    .x(r.x)
                    .y(r.y)
                    .width(r.w as u32)
                    .height(r.h as u32),
            )?;
            if !self.bar.mapped {
                self.xc.conn.map_window(self.bar.id)?;
                self.bar.mapped = true;
            }
            self.raise(self.bar.id)?;
            self.redraw_bar(fonts)?;
        } else if self.bar.mapped {
            self.xc.conn.unmap_window(self.bar.id)?;
            self.bar.mapped = false;
            self.bar_pressed = None;
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    fn raise(&self, w: XWindow) -> Result<(), XError> {
        self.xc
            .conn
            .configure_window(w, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        Ok(())
    }

    fn redraw_bar(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        Painter {
            cv: &mut self.bar.canvas,
            fonts,
        }
        .edit_bar(buttons(self.terminal), self.bar_pressed);
        crate::present::present(
            &self.xc.conn,
            self.bar.id,
            self.bar.gc,
            self.depth,
            &self.bar.canvas,
            self.bar.canvas.bounds(),
        )
    }

    /// The keyboard / mouse pad changed height: the toolbar and the list move with it.
    pub fn set_bottom(&mut self, h: i32, fonts: &mut Fonts) -> Result<(), XError> {
        if h == self.bottom_h {
            return Ok(());
        }
        self.bottom_h = h;
        self.relayout(fonts)
    }

    /// Rotation.
    pub fn set_shell_size(&mut self, w: i32, h: i32, fonts: &mut Fonts) -> Result<(), XError> {
        let area = visible_area(w, h);
        if area == self.area {
            return Ok(());
        }
        self.area = area;
        self.relayout(fonts)
    }

    fn relayout(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        if self.bar.mapped {
            self.apply_bar(fonts)?;
        }
        if self.list.mapped {
            self.apply_list(fonts)?;
        }
        Ok(())
    }

    // ---- clipboard list
    /// Show the list with these entries (newest first).
    pub fn show_list(&mut self, entries: &[String], fonts: &mut Fonts) -> Result<(), XError> {
        self.entries = entries.to_vec();
        self.list_until = Some(Instant::now() + Duration::from_millis(LIST_MS));
        self.list_pressed = None;
        self.apply_list(fonts)
    }

    /// The history changed while the list is up.
    pub fn update_list(&mut self, entries: &[String], fonts: &mut Fonts) -> Result<(), XError> {
        if self.list.mapped {
            self.entries = entries.to_vec();
            self.apply_list(fonts)?;
        }
        Ok(())
    }

    pub fn hide_list(&mut self) -> Result<(), XError> {
        self.list_until = None;
        if self.list.mapped {
            self.xc.conn.unmap_window(self.list.id)?;
            self.list.mapped = false;
            self.list_pressed = None;
            self.xc.conn.flush()?;
        }
        Ok(())
    }

    fn apply_list(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        let r = list_rect(self.area, self.bottom_h, self.entries.len());
        let shown = self.shown_entries(r);
        self.list.canvas = Canvas::new(r.w as u32, r.h as u32, BG).ok_or("list canvas")?;
        self.xc.conn.configure_window(
            self.list.id,
            &ConfigureWindowAux::new()
                .x(r.x)
                .y(r.y)
                .width(r.w as u32)
                .height(r.h as u32),
        )?;
        if !self.list.mapped {
            self.xc.conn.map_window(self.list.id)?;
            self.list.mapped = true;
        }
        self.raise(self.list.id)?;
        self.redraw_list(fonts, shown)?;
        self.xc.conn.flush()?;
        Ok(())
    }

    /// How many entries the window of height `r.h` shows.
    fn shown_entries(&self, r: Rect) -> usize {
        if self.entries.is_empty() {
            0
        } else {
            (((r.h - CLIP_HEAD_H) / CLIP_ROW_H) as usize).min(self.entries.len())
        }
    }

    fn redraw_list(&mut self, fonts: &mut Fonts, shown: usize) -> Result<(), XError> {
        let entries = &self.entries[..shown];
        self.list_hits = Painter {
            cv: &mut self.list.canvas,
            fonts,
        }
        .clipboard_list(entries, self.list_pressed);
        crate::present::present(
            &self.xc.conn,
            self.list.id,
            self.list.gc,
            self.depth,
            &self.list.canvas,
            self.list.canvas.bounds(),
        )
    }

    /// Type the paste shortcut of the app in front (after the shell made the entry the clipboard).
    pub fn send_paste(&self) -> Result<(), XError> {
        let class = active_class(&self.xc, self.active_atom);
        inject(&self.xc, &self.keymap, paste_stroke(is_terminal(&class)))
    }

    /// Handle everything the server has sent (non-blocking).
    pub fn poll(&mut self, fonts: &mut Fonts) -> Result<Vec<EditOut>, XError> {
        let mut out = Vec::new();
        while let Some(ev) = self.xc.conn.poll_for_event()? {
            match ev {
                Event::PropertyNotify(e) if e.atom == self.active_atom => {
                    let was = (self.active, self.terminal);
                    self.read_active();
                    if was != (self.active, self.terminal) {
                        if self.bar.mapped {
                            if was.1 != self.terminal {
                                self.apply_bar(fonts)?;
                            } else {
                                // the window manager restacked: keep the toolbar on top
                                self.raise(self.bar.id)?;
                            }
                        }
                        if self.list.mapped {
                            self.raise(self.list.id)?;
                        }
                        if self.active == self.shell && was.0 != 0 {
                            // Home: the list goes away (the toolbar follows the shell's rule)
                            self.hide_list()?;
                        }
                        self.xc.conn.flush()?;
                    }
                }
                Event::Expose(e) if e.count == 0 => {
                    if e.window == self.bar.id && self.bar.mapped {
                        self.redraw_bar(fonts)?;
                    } else if e.window == self.list.id && self.list.mapped {
                        let r = list_rect(self.area, self.bottom_h, self.entries.len());
                        let shown = self.shown_entries(r);
                        self.redraw_list(fonts, shown)?;
                    }
                }
                Event::ButtonPress(e) if e.detail == 1 => {
                    if e.event == self.bar.id {
                        self.bar_pressed = self.bar_at(e.event_x as i32, e.event_y as i32);
                        self.redraw_bar(fonts)?;
                    } else if e.event == self.list.id {
                        self.list_pressed = self.list_at(e.event_x as i32, e.event_y as i32);
                        let r = list_rect(self.area, self.bottom_h, self.entries.len());
                        let shown = self.shown_entries(r);
                        self.redraw_list(fonts, shown)?;
                    }
                }
                Event::ButtonRelease(e) if e.detail == 1 => {
                    if e.event == self.bar.id {
                        let hit = self.bar_at(e.event_x as i32, e.event_y as i32);
                        let down = self.bar_pressed.take();
                        if let (Some(h), Some(d)) = (hit, down) {
                            if h == d {
                                self.run_button(buttons(self.terminal)[h], &mut out);
                            }
                        }
                        if self.bar.mapped {
                            self.redraw_bar(fonts)?;
                        }
                    } else if e.event == self.list.id {
                        let hit = self.list_at(e.event_x as i32, e.event_y as i32);
                        let down = self.list_pressed.take();
                        if let (Some(h), Some(d)) = (hit, down) {
                            if h == d {
                                match h {
                                    ClipHit::Entry(i) => {
                                        out.push(EditOut::Paste(i));
                                        self.hide_list()?;
                                    }
                                    ClipHit::Clear => out.push(EditOut::ClearHistory),
                                    ClipHit::Close => self.hide_list()?,
                                }
                            }
                        }
                        if self.list.mapped {
                            let r = list_rect(self.area, self.bottom_h, self.entries.len());
                            let shown = self.shown_entries(r);
                            self.redraw_list(fonts, shown)?;
                        }
                    }
                }
                Event::Error(e) => return Err(XError(format!("edit bar X error: {e:?}"))),
                _ => {}
            }
        }
        if self.list_until.is_some_and(|t| Instant::now() >= t) {
            self.hide_list()?;
        }
        Ok(out)
    }

    fn bar_at(&self, x: i32, y: i32) -> Option<usize> {
        bar_layout(buttons(self.terminal))
            .iter()
            .position(|(r, _)| r.contains(x, y))
    }

    fn list_at(&self, x: i32, y: i32) -> Option<ClipHit> {
        let r = list_rect(self.area, self.bottom_h, self.entries.len());
        let shown = self.shown_entries(r);
        clip_layout(r.w, shown)
            .into_iter()
            .find(|(b, _)| b.contains(x, y))
            .map(|(_, h)| h)
    }

    fn run_button(&mut self, b: EditBtn, out: &mut Vec<EditOut>) {
        // the app in front may have changed since the toolbar was drawn
        let terminal = is_terminal(&active_class(&self.xc, self.active_atom));
        if terminal && b == EditBtn::Copy {
            out.push(EditOut::CopyPrimary);
        } else if let Some(s) = button_stroke(b, terminal) {
            if let Err(e) = inject(&self.xc, &self.keymap, s) {
                eprintln!("walpad-shell: edit toolbar: {}", e.0);
            }
        }
        out.push(EditOut::Acted);
    }

    /// For tests and diagnostics: is the window mapped according to the server?
    pub fn server_mapped(&self, w: XWindow) -> bool {
        self.xc
            .conn
            .get_window_attributes(w)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|a| a.map_state != xproto::MapState::UNMAPPED)
    }

    /// The Control_L keysym is part of every edit shortcut: false when the server has no keycode
    /// for it (the toolbar then cannot type).
    pub fn can_type(&self) -> bool {
        self.keymap.lookup(XK_CONTROL_L).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cls(a: &str, b: &str) -> Vec<String> {
        vec![a.to_string(), b.to_string()]
    }

    #[test]
    fn terminals_are_recognised_by_wm_class() {
        assert!(is_terminal(&cls("st", "St")));
        assert!(is_terminal(&cls("xterm", "XTerm")));
        assert!(is_terminal(&cls("st-256color", "st-256color")));
        // regression (2026-11-13): the "terminal"/"kali" apps got their own real WM_CLASS
        // (term-st/kali-st, not a shared "st") so a restart could tell them apart - this list
        // must track that or Ctrl+C stops being an interrupt and paste stops using Ctrl+Shift+V.
        assert!(is_terminal(&cls("term-st", "term-st")));
        assert!(is_terminal(&cls("kali-st", "kali-st")));
        assert!(!is_terminal(&cls("Navigator", "firefox-esr")));
        assert!(!is_terminal(&cls("pcmanfm", "Pcmanfm")));
        assert!(!is_terminal(&[]));
    }

    #[test]
    fn button_strokes_are_the_ctrl_shortcuts_and_never_interrupt_a_terminal() {
        let k = |c: char| Some(Stroke::ctrl(c as u32));
        assert_eq!(button_stroke(EditBtn::Cut, false), k('x'));
        assert_eq!(button_stroke(EditBtn::Copy, false), k('c'));
        assert_eq!(button_stroke(EditBtn::Paste, false), k('v'));
        assert_eq!(button_stroke(EditBtn::SelectAll, false), k('a'));
        assert_eq!(button_stroke(EditBtn::Undo, false), k('z'));
        // terminal: paste is Ctrl+Shift+V; nothing else types (Ctrl+C would interrupt)
        let tv = button_stroke(EditBtn::Paste, true).unwrap();
        assert!(tv.ctrl && tv.shift && tv.keysym == 'v' as u32);
        for b in [
            EditBtn::Cut,
            EditBtn::Copy,
            EditBtn::SelectAll,
            EditBtn::Undo,
        ] {
            assert_eq!(button_stroke(b, true), None, "{b:?}");
        }
        assert_eq!(paste_stroke(false), Stroke::ctrl('v' as u32));
    }

    #[test]
    fn toolbar_sits_above_the_keyboard_or_the_screen_edge() {
        let r = bar_rect((800, 1280), 384, 5);
        assert_eq!((r.x, r.w, r.h), (8, 480, 52));
        assert_eq!(r.bottom(), 1280 - 384 - 8);
        let r = bar_rect((800, 1280), 0, 5);
        assert_eq!(r.bottom(), 1280 - 8);
        // the dock handle (56 wide, 8 from the right edge) does not overlap it
        assert!(r.right() <= 800 - 8 - 56);
        let r = bar_rect((1280, 800), 240, 2);
        assert_eq!((r.w, r.bottom()), (192, 800 - 240 - 8));
    }

    #[test]
    fn list_sits_above_the_toolbar_and_shrinks_to_fit() {
        let r = list_rect((800, 1280), 384, 10);
        assert_eq!(r.h, clip_height(10));
        assert!(r.bottom() <= bar_rect((800, 1280), 384, 5).y);
        assert!(r.y >= 0);
        // landscape with the keyboard: fewer rows fit
        let r = list_rect((1280, 800), 240, 10);
        assert!(r.h < clip_height(10));
        assert!(r.y >= 0 && r.bottom() <= bar_rect((1280, 800), 240, 5).y);
        let r = list_rect((800, 1280), 0, 0);
        assert_eq!(r.h, clip_height(0));
        assert_eq!(r.w, 520);
    }

    #[test]
    fn toolbar_visibility_rule() {
        let mut s = BarState::default();
        assert!(!s.visible(0));
        // owner report 2026-11-13, "i don't need [it on top of the keyboard] becose our latest
        // sytem of copy will cover thows": merely focusing a text input no longer shows it -
        // only a real selection does.
        s.home = true;
        s.selection(0);
        assert!(!s.visible(0), "never over the shell's own screen");
        s.home = false;
        s.dock_out = true;
        assert!(!s.visible(0), "the expanded dock covers its place");
        s.dock_out = false;
        s.clear_selection();
        s.selection(1000);
        assert!(s.visible(1000 + SELECTION_MS - 1));
        assert!(
            !s.visible(1000 + SELECTION_MS),
            "a selection toolbar times out"
        );
        s.selection(5000);
        s.clear_selection();
        assert!(!s.visible(5001));
        assert_eq!(s.deadline(), None);
    }
}
