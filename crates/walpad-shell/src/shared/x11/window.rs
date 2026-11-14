//! The shell's X window: properties (WM_CLASS walpad-shell, title "WalPad Home"), pointer and key
//! events, resize notifications, and presenting canvases. Every X wait has a deadline (walpad-x).
use crate::gesture::Input;
use crate::present;
use std::path::Path;
use walpad_ui::{Canvas, Rect};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt as _, CreateGCAux, CreateWindowAux, EventMask, KeyButMask, PropMode,
    Window as XWindow, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

pub const WM_CLASS_INSTANCE: &str = "walpad-shell";
pub const WM_CLASS_CLASS: &str = "WalPad-Shell";
pub const TITLE: &str = "WalPad Home";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinEvent {
    Input(Input),
    Resize(i32, i32),
    Expose,
    /// Keysym of a pressed key (Up/Down/Return drive hardware-key focus).
    Key(u32),
}

pub struct Window {
    pub xc: XConn,
    pub win: XWindow,
    gc: u32,
    depth: u8,
    min_keycode: u8,
    per: usize,
    syms: Vec<u32>,
    pub size: (i32, i32),
}

impl Window {
    pub fn open(socket: &Path, w: i32, h: i32) -> Result<Self, XError> {
        let xc = walpad_x::connect(socket)?;
        let conn = &xc.conn;
        let setup = conn.setup();
        let depth = setup.roots.first().map(|s| s.root_depth).unwrap_or(24);
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let km = conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
        let win = conn.generate_id()?;
        let events = EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::BUTTON1_MOTION
            | EventMask::EXPOSURE
            | EventMask::STRUCTURE_NOTIFY
            | EventMask::KEY_PRESS;
        let aux = CreateWindowAux::new()
            .event_mask(events)
            .background_pixel(0x0011_1316);
        conn.create_window(
            0,
            win,
            xc.root,
            0,
            0,
            w as u16,
            h as u16,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &aux,
        )?;
        let gc = conn.generate_id()?;
        conn.create_gc(gc, win, &CreateGCAux::new())?;
        let class = format!("{WM_CLASS_INSTANCE}\0{WM_CLASS_CLASS}\0");
        conn.change_property8(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            class.as_bytes(),
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            TITLE.as_bytes(),
        )?;
        let utf8 = conn.intern_atom(false, b"UTF8_STRING")?.reply()?.atom;
        let net_name = conn.intern_atom(false, b"_NET_WM_NAME")?.reply()?.atom;
        conn.change_property8(PropMode::REPLACE, win, net_name, utf8, TITLE.as_bytes())?;
        conn.map_window(win)?;
        conn.flush()?;
        Ok(Window {
            xc,
            win,
            gc,
            depth,
            min_keycode: min,
            per: km.keysyms_per_keycode as usize,
            syms: km.keysyms,
            size: (w, h),
        })
    }

    fn keysym(&self, keycode: u8) -> Option<u32> {
        let i = (keycode.checked_sub(self.min_keycode)? as usize) * self.per.max(1);
        self.syms.get(i).copied().filter(|s| *s != 0)
    }

    /// Everything the server has sent so far (non-blocking).
    pub fn poll_events(&mut self) -> Result<Vec<WinEvent>, XError> {
        let mut out = Vec::new();
        while let Some(ev) = self.xc.conn.poll_for_event()? {
            match ev {
                Event::ButtonPress(e) if e.detail == 1 => out.push(WinEvent::Input(Input::Down {
                    x: e.event_x as i32,
                    y: e.event_y as i32,
                })),
                Event::ButtonRelease(e) if e.detail == 1 => out.push(WinEvent::Input(Input::Up {
                    x: e.event_x as i32,
                    y: e.event_y as i32,
                })),
                Event::MotionNotify(e) if e.state.contains(KeyButMask::BUTTON1) => {
                    out.push(WinEvent::Input(Input::Move {
                        x: e.event_x as i32,
                        y: e.event_y as i32,
                    }))
                }
                Event::ConfigureNotify(e) if e.window == self.win => {
                    let size = (e.width as i32, e.height as i32);
                    if size != self.size {
                        self.size = size;
                        out.push(WinEvent::Resize(size.0, size.1));
                    }
                }
                Event::Expose(e) if e.count == 0 => out.push(WinEvent::Expose),
                Event::KeyPress(e) => {
                    if let Some(k) = self.keysym(e.detail) {
                        out.push(WinEvent::Key(k));
                    }
                }
                Event::Error(e) => return Err(XError(format!("X error: {e:?}"))),
                _ => {}
            }
        }
        Ok(out)
    }

    pub fn present(&self, cv: &Canvas, damage: Rect) -> Result<(), XError> {
        present::present(&self.xc.conn, self.win, self.gc, self.depth, cv, damage)
    }

    /// Wait for X traffic (or the timeout). Always drain `poll_events` first.
    pub fn wait(&self, timeout_ms: Option<u64>) -> bool {
        walpad_x::wait_readable(&self.xc.wait_fd, timeout_ms)
    }
}
