//! System keyboard: the shell's own on-screen keyboard for apps (terminal, browser), replacing
//! matchbox-keyboard (openbox maximised it and Xvfb spun in a redraw loop while it was open).
//!
//! Same pattern as the system dock (dock.rs): an override-redirect window that the window manager
//! never moves or maximises, full width, flush with the bottom of the visible area, 30 % of its
//! height; plus a managed 1x1 `_NET_WM_WINDOW_TYPE_DOCK` window carrying the bottom strut so
//! maximised apps shrink above it. It never takes the focus (WM_HINTS input=false; clicks on an
//! override-redirect window do not move the focus), and it types into the focused app itself
//! through XTEST on its own X connection. Every wait on that connection has a deadline (walpad-x).

mod bottom_windows;
mod events;
mod keymap;

#[cfg(test)]
mod tests;

pub(crate) use bottom_windows::{create_bottom_windows, BottomWindows};
pub use keymap::{inject, load_keymap, tap, KeyMap, Stroke, Tap};

use crate::dock::visible_area;
use std::os::unix::net::UnixStream;
use std::path::Path;
use walpad_ui::components::{system_keyboard_layout, Painter, SysKbdState, SysKeyBtn};
use walpad_ui::tokens::bg;
use walpad_ui::{Canvas, Fonts, Rect};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{EventMask, Window as XWindow};

pub const WM_CLASS_INSTANCE: &str = "walpad-keyboard";
pub const WM_CLASS_CLASS: &str = "WalPad-Keyboard";
pub const STRUT_CLASS: &str = "walpad-keyboard-strut";
/// Keyboard height in percent of the visible height.
pub const HEIGHT_PCT: i32 = 30;

// keysyms
pub const XK_SHIFT_L: u32 = 0xffe1;
pub const XK_CONTROL_L: u32 = 0xffe3;
pub const XK_ALT_L: u32 = 0xffe9;
pub const XK_RETURN: u32 = 0xff0d;
pub const XK_TAB: u32 = 0xff09;
pub const XK_ESCAPE: u32 = 0xff1b;
pub const XK_BACKSPACE: u32 = 0xff08;
pub const XK_LEFT: u32 = 0xff51;
pub const XK_UP: u32 = 0xff52;
pub const XK_RIGHT: u32 = 0xff53;
pub const XK_DOWN: u32 = 0xff54;

/// Keyboard height for a visible area (portrait 384, landscape 240).
pub fn height(area: (i32, i32)) -> i32 {
    area.1 * HEIGHT_PCT / 100
}

/// The keyboard window: full width, flush with the bottom of the visible area.
pub fn keyboard_rect(area: (i32, i32)) -> Rect {
    let h = height(area);
    Rect::new(0, area.1 - h, area.0, h)
}

/// `_NET_WM_STRUT_PARTIAL` reserving the screen from `r.y` down to the bottom of the root (the
/// root is 1280x1280, so landscape includes the 480 px below the panel).
pub fn bottom_strut(r: Rect, root_h: i32) -> [u32; 12] {
    let bottom = (root_h - r.y).max(0) as u32;
    [
        0,
        0,
        0,
        bottom,
        0,
        0,
        0,
        0,
        0,
        0,
        r.x as u32,
        (r.right() - 1).max(0) as u32,
    ]
}

pub fn strut_partial(area: (i32, i32), root_h: i32) -> [u32; 12] {
    bottom_strut(keyboard_rect(area), root_h)
}

pub struct SysKeyboard {
    pub(super) xc: XConn,
    pub(super) bw: BottomWindows,
    pub(super) shell: XWindow,
    keymap: KeyMap,
    area: (i32, i32),
    pub(super) mapped: bool,
    pub(super) active: u32,
    pub(super) state: SysKbdState,
    pub(super) keys: Vec<SysKeyBtn>,
    pub(super) pressed: Option<usize>,
    canvas: Canvas,
}

impl SysKeyboard {
    pub fn open(socket: &Path, shell: XWindow, shell_size: (i32, i32)) -> Result<Self, XError> {
        let xc = walpad_x::connect(socket)?;
        let keymap = load_keymap(&xc)?;
        let area = visible_area(shell_size.0, shell_size.1);
        let r = keyboard_rect(area);
        let bw = create_bottom_windows(
            &xc,
            WM_CLASS_INSTANCE,
            WM_CLASS_CLASS,
            STRUT_CLASS,
            r,
            EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::EXPOSURE,
        )?;
        bw.write_strut(&xc, area)?;
        let active = bw.read_active(&xc);
        xc.conn.flush()?;
        Ok(SysKeyboard {
            xc,
            bw,
            shell,
            keymap,
            area,
            mapped: false,
            active,
            state: SysKbdState::default(),
            keys: Vec::new(),
            pressed: None,
            canvas: Canvas::new(r.w as u32, r.h as u32, bg()).ok_or("keyboard canvas")?,
        })
    }

    pub fn wait_fd(&self) -> &UnixStream {
        &self.xc.wait_fd
    }
    pub fn window(&self) -> XWindow {
        self.bw.win
    }
    pub fn visible(&self) -> bool {
        self.mapped
    }
    /// Height of the keyboard for the current orientation (also while hidden).
    pub fn height(&self) -> i32 {
        height(self.area)
    }

    /// The shell window changed size (rotation): move and resize the keyboard and its strut.
    pub fn set_shell_size(&mut self, w: i32, h: i32, fonts: &mut Fonts) -> Result<(), XError> {
        let area = visible_area(w, h);
        if area == self.area {
            return Ok(());
        }
        self.area = area;
        let r = keyboard_rect(area);
        self.canvas = Canvas::new(r.w as u32, r.h as u32, bg()).ok_or("keyboard canvas")?;
        self.pressed = None;
        self.bw.place(&self.xc, r)?;
        self.bw.write_strut(&self.xc, area)?;
        if self.mapped {
            self.redraw(fonts)?;
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    /// Show or hide. Showing maps the strut window too (apps shrink above the keyboard) and puts
    /// the keyboard on top; hiding resets the latched modifiers.
    pub fn set_visible(&mut self, on: bool, fonts: &mut Fonts) -> Result<(), XError> {
        if on == self.mapped {
            return Ok(());
        }
        self.mapped = on;
        self.pressed = None;
        self.bw.set_mapped(&self.xc, on, keyboard_rect(self.area))?;
        if on {
            // paint right away: a remapped window is not always sent an Expose
            self.redraw(fonts)?;
        } else {
            self.state = SysKbdState::default();
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    pub(super) fn redraw(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        self.keys = Painter {
            cv: &mut self.canvas,
            fonts,
        }
        .system_keyboard(self.state, self.pressed);
        crate::present::present(
            &self.xc.conn,
            self.bw.win,
            self.bw.gc,
            self.bw.depth,
            &self.canvas,
            self.canvas.bounds(),
        )
    }

    pub(super) fn redraw_key(&mut self, idx: usize, fonts: &mut Fonts) -> Result<(), XError> {
        let Some(k) = self.keys.get(idx) else {
            return Ok(());
        };
        let rect = k.rect;
        Painter {
            cv: &mut self.canvas,
            fonts,
        }
        .system_keyboard_key(k, self.state, self.pressed == Some(idx));
        crate::present::present(
            &self.xc.conn,
            self.bw.win,
            self.bw.gc,
            self.bw.depth,
            &self.canvas,
            rect,
        )
    }

    pub(super) fn key_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.keys.is_empty() {
            return self.keys.iter().position(|k| k.rect.contains(x, y));
        }
        let r = keyboard_rect(self.area);
        system_keyboard_layout(Rect::new(0, 0, r.w, r.h), self.state.symbols)
            .iter()
            .position(|k| k.rect.contains(x, y))
    }

    /// Inject one stroke into the focused window through XTEST.
    pub fn send(&self, s: Stroke) -> Result<(), XError> {
        inject(&self.xc, &self.keymap, s)
    }
}
