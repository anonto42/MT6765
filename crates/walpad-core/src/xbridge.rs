//! X bridge (M3.2/M3.3): struts, keyboard docking, key and pointer injection (XTEST) and window
//! focus, over a stream whose every wait has a deadline (a hung X server cannot hang the core).
//! The core connects lazily and drops/reconnects the bridge when X goes away (session restarts).

mod geometry;
mod input;
mod keys;
#[cfg(test)]
mod tests;
mod windows;

pub use geometry::WinInfo;
pub use keys::{
    back_spec, class_matches, keysym, parse_key_spec, TouchHit, XWindow, BACK_ALT_LEFT,
};
pub use walpad_x::XError;

use std::path::Path;
use walpad_x::TimedStream;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ConfigureWindowAux, ConnectionExt as _, CreateWindowAux, PropMode, Window,
    WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

pub(super) type R<T> = Result<T, XError>;

pub struct XBridge {
    pub(super) conn: RustConnection<TimedStream>,
    pub(super) root: Window,
    pub(super) min_keycode: u8,
    pub(super) per: usize,
    pub(super) syms: Vec<u32>,
    pub(super) strut_win: Option<Window>,
    pub(super) button_down: bool,
}

impl XBridge {
    /// Connect to the X server listening on `socket` (e.g. /tmp/.X11-unix/X0).
    pub fn connect(socket: &Path) -> R<Self> {
        let xc = walpad_x::connect(socket)?;
        let (conn, root) = (xc.conn, xc.root);
        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let km = conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
        Ok(XBridge {
            conn,
            root,
            min_keycode: min,
            per: km.keysyms_per_keycode as usize,
            syms: km.keysyms,
            strut_win: None,
            button_down: false,
        })
    }

    pub(super) fn atom(&self, name: &str) -> R<u32> {
        Ok(self.conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
    }

    /// Pointer position (tests and diagnostics).
    pub fn pointer_position(&self) -> R<(i16, i16)> {
        let q = self.conn.query_pointer(self.root)?.reply()?;
        Ok((q.root_x, q.root_y))
    }

    /// Cheap round trip used to notice a dead/restarted X server.
    pub fn ping(&self) -> R<()> {
        self.conn.get_input_focus()?.reply()?;
        Ok(())
    }

    // ---- struts and keyboard docking (rotation)
    pub fn set_struts(&mut self, o: u16) -> R<()> {
        let win = match self.strut_win {
            Some(w) => w,
            None => {
                let w = self.conn.generate_id()?;
                self.conn.create_window(
                    0,
                    w,
                    self.root,
                    0,
                    0,
                    1,
                    1,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    0,
                    &CreateWindowAux::new(),
                )?;
                let utf8 = self.atom("UTF8_STRING")?;
                let wt = self.atom("_NET_WM_WINDOW_TYPE")?;
                let dock = self.atom("_NET_WM_WINDOW_TYPE_DOCK")?;
                self.conn
                    .change_property32(PropMode::REPLACE, w, wt, AtomEnum::ATOM, &[dock])?;
                self.conn.change_property8(
                    PropMode::REPLACE,
                    w,
                    AtomEnum::WM_NAME,
                    AtomEnum::STRING,
                    b"walpad-strut",
                )?;
                let nn = self.atom("_NET_WM_NAME")?;
                self.conn
                    .change_property8(PropMode::REPLACE, w, nn, utf8, b"walpad-strut")?;
                self.conn.map_window(w)?;
                self.strut_win = Some(w);
                w
            }
        };
        let partial = crate::logic::rotation::strut_partial(o);
        let plain = crate::logic::rotation::strut(o);
        let sp = self.atom("_NET_WM_STRUT_PARTIAL")?;
        let s = self.atom("_NET_WM_STRUT")?;
        self.conn
            .change_property32(PropMode::REPLACE, win, sp, AtomEnum::CARDINAL, &partial)?;
        self.conn
            .change_property32(PropMode::REPLACE, win, s, AtomEnum::CARDINAL, &plain)?;
        self.conn.flush()?;
        Ok(())
    }

    fn is_class(&self, w: Window, needle: &str) -> bool {
        self.conn
            .get_property(false, w, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|p| {
                String::from_utf8_lossy(&p.value)
                    .to_ascii_lowercase()
                    .contains(needle)
            })
    }

    /// Windows (depth <= 3 under the root) whose WM_CLASS contains `needle`.
    pub fn find_by_class(&self, needle: &str) -> R<Vec<Window>> {
        let needle = needle.to_ascii_lowercase();
        let mut out = Vec::new();
        let mut level = vec![self.root];
        for _ in 0..3 {
            let mut next = Vec::new();
            for w in level {
                for c in self.conn.query_tree(w)?.reply()?.children {
                    if self.is_class(c, &needle) {
                        out.push(c);
                    }
                    next.push(c);
                }
            }
            level = next;
        }
        Ok(out)
    }

    /// Dock matchbox-keyboard to the bottom of the visible area; returns how many windows moved.
    pub fn dock_keyboard(&mut self, o: u16) -> R<usize> {
        let (x, y, w, h) = crate::logic::rotation::keyboard_geometry(o);
        let wins = self.find_by_class("matchbox-keyboard")?;
        for win in &wins {
            self.conn.configure_window(
                *win,
                &ConfigureWindowAux::new()
                    .x(x as i32)
                    .y(y as i32)
                    .width(w)
                    .height(h),
            )?;
        }
        self.conn.flush()?;
        Ok(wins.len())
    }
}
