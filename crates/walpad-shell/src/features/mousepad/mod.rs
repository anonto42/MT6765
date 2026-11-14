//! Mouse pad: a laptop-style touchpad docked at the bottom of the screen that drives the X pointer
//! over any app (browser, terminal). Opened from the Apps panel's TOOLS.
//!
//! Windows: exactly like the system keyboard (keyboard.rs), an override-redirect window that never
//! takes the focus (WM_HINTS input=false) plus a managed 1x1 dock-type window with the bottom
//! strut; same size (30 % of the visible height, full width). The keyboard and the pad replace each
//! other (the shell loop closes one when the other opens).
//!
//! Input: the core's touch bridge moves the real X pointer to the finger and holds button 1 (XTEST)
//! while a finger is down, so a touch on the pad arrives here as ButtonPress / MotionNotify /
//! ButtonRelease (the pad keeps the implicit grab until the finger lifts, wherever the pointer is).
//! The pad keeps its own virtual cursor: finger motion moves it relatively (x1.5, clamped to the
//! visible area above the pad) and after every pad event the real pointer is put back on it with
//! an XTEST motion, so the app under it sees the cursor there, not under the finger. Those warps
//! come back as MotionNotify events of their own; `PadState` remembers the positions it warped to
//! and ignores their echoes. Clicks are XTEST button presses at the virtual cursor, sent after the
//! finger lifted (the grab is over, so they reach the app under the cursor).
//!
//! Every wait on the X connection has a deadline (walpad-x); no request here waits for a reply
//! except the setup in `open`.

mod events;
mod layout;
mod marker;
mod paint;
mod state;

#[cfg(test)]
mod tests;

pub use layout::{
    cursor_bounds, pad_layout, pad_rect, strip_color, surface_bg, PadButton, PadLayout, PadOut,
    ACCEL, BRIDGE_LIFT_MS, BUTTON_LEFT, BUTTON_RIGHT, BUTTON_WHEEL_DOWN, BUTTON_WHEEL_UP, JUMP_PX,
    LONG_PRESS_MS, MARKER_CLASS, PAD_BUTTONS, SMOOTH, STRUT_CLASS, TAP_MAX_MS, TAP_SLOP_PX,
    TAP_TRAVEL_PX, WM_CLASS_CLASS, WM_CLASS_INSTANCE,
};
pub use marker::{marker_shape, marker_size, paint_marker, MARKER_SCALE};
pub use paint::{paint_map, paint_pad};
pub use state::PadState;

use crate::dock::visible_area;
use crate::keyboard::{create_bottom_windows, BottomWindows};
use marker::Marker;
use std::os::unix::net::UnixStream;
use std::path::Path;
use walpad_ui::components::Painter;
use walpad_ui::tokens::bg;
use walpad_ui::{Canvas, Fonts};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, EventMask, Window as XWindow};
use x11rb::protocol::xtest::ConnectionExt as _;

pub struct MousePad {
    pub(super) xc: XConn,
    pub(super) bw: BottomWindows,
    pub(super) shell: XWindow,
    pub(super) mapped: bool,
    pub(super) active: u32,
    pub(super) st: PadState,
    canvas: Canvas,
    marker: Option<Marker>,
}

impl MousePad {
    pub fn open(socket: &Path, shell: XWindow, shell_size: (i32, i32)) -> Result<Self, XError> {
        let xc = walpad_x::connect(socket)?;
        let area = visible_area(shell_size.0, shell_size.1);
        let r = pad_rect(area);
        let bw = create_bottom_windows(
            &xc,
            WM_CLASS_INSTANCE,
            WM_CLASS_CLASS,
            STRUT_CLASS,
            r,
            EventMask::BUTTON_PRESS
                | EventMask::BUTTON_RELEASE
                | EventMask::POINTER_MOTION
                | EventMask::EXPOSURE,
        )?;
        bw.write_strut(&xc, area)?;
        let active = bw.read_active(&xc);
        let marker = Marker::create(&xc, bw.depth);
        xc.conn.flush()?;
        Ok(MousePad {
            marker,
            xc,
            bw,
            shell,
            mapped: false,
            active,
            st: PadState::new(area),
            canvas: Canvas::new(r.w as u32, r.h as u32, bg()).ok_or("mouse pad canvas")?,
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

    /// Height of the pad for the current orientation (also while hidden).
    pub fn height(&self) -> i32 {
        pad_rect(self.st.area()).h
    }

    /// The virtual cursor (root coordinates).
    pub fn cursor(&self) -> (i32, i32) {
        self.st.cursor
    }

    /// The shell window changed size (rotation): move and resize the pad and its strut.
    pub fn set_shell_size(&mut self, w: i32, h: i32, fonts: &mut Fonts) -> Result<(), XError> {
        let area = visible_area(w, h);
        if area == self.st.area() {
            return Ok(());
        }
        self.st.set_area(area);
        let r = pad_rect(area);
        self.canvas = Canvas::new(r.w as u32, r.h as u32, bg()).ok_or("mouse pad canvas")?;
        self.bw.place(&self.xc, r)?;
        self.bw.write_strut(&self.xc, area)?;
        if self.mapped {
            self.redraw(fonts)?;
            self.warp()?;
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    /// Show (cursor in the middle of the area above the pad, the pointer put there) or hide.
    pub fn set_visible(&mut self, on: bool, fonts: &mut Fonts) -> Result<(), XError> {
        if on == self.mapped {
            return Ok(());
        }
        self.mapped = on;
        self.st.reset_touch();
        self.bw.set_mapped(&self.xc, on, pad_rect(self.st.area()))?;
        if let Some(m) = &self.marker {
            m.set_mapped(&self.xc, on)?;
        }
        if on {
            self.st.recenter();
            // paint right away: a remapped window is not always sent an Expose
            self.redraw(fonts)?;
            self.warp()?;
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    pub(super) fn redraw(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        paint_pad(
            &mut Painter {
                cv: &mut self.canvas,
                fonts,
            },
            &self.st,
        );
        crate::present::present(
            &self.xc.conn,
            self.bw.win,
            self.bw.gc,
            self.bw.depth,
            &self.canvas,
            self.canvas.bounds(),
        )
    }

    /// Only the mini-map changed (cursor motion): repaint and send that corner.
    pub(super) fn redraw_map(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        let map = pad_layout(self.st.area()).map;
        paint_map(
            &mut Painter {
                cv: &mut self.canvas,
                fonts,
            },
            map,
            self.st.area(),
            self.st.cursor,
        );
        crate::present::present(
            &self.xc.conn,
            self.bw.win,
            self.bw.gc,
            self.bw.depth,
            &self.canvas,
            map.inset(-2),
        )
    }

    /// Put the real pointer on the virtual cursor (XTEST absolute motion).
    pub(super) fn warp(&mut self) -> Result<(), XError> {
        let (x, y) = self.st.cursor;
        self.xc.conn.xtest_fake_input(
            xproto::MOTION_NOTIFY_EVENT,
            0,
            0,
            self.xc.root,
            x as i16,
            y as i16,
            0,
        )?;
        self.st.note_warp();
        if let Some(m) = &self.marker {
            m.place(&self.xc, (x, y))?;
        }
        Ok(())
    }

    /// Press and release `button` at the virtual cursor.
    pub(super) fn click(&mut self, button: u8) -> Result<(), XError> {
        self.warp()?;
        let (c, root) = (&self.xc.conn, self.xc.root);
        c.xtest_fake_input(xproto::BUTTON_PRESS_EVENT, button, 0, root, 0, 0, 0)?;
        c.xtest_fake_input(xproto::BUTTON_RELEASE_EVENT, button, 0, root, 0, 0, 0)?;
        Ok(())
    }
}
