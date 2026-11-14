//! Shared X11 bottom-panel window & strut helper used by `SysKeyboard` and `MousePad`.

use super::strut_partial;
use walpad_ui::Rect;
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt as _, CreateGCAux,
    CreateWindowAux, EventMask, MapState, PropMode, StackMode, Window as XWindow, WindowClass,
};
use x11rb::wrapper::ConnectionExt as _;

pub(crate) struct Atoms {
    pub(crate) active: u32,
    window_type: u32,
    type_dock: u32,
    strut_partial: u32,
    strut: u32,
    net_name: u32,
    utf8: u32,
}

impl Atoms {
    pub(crate) fn intern(xc: &XConn) -> Result<Atoms, XError> {
        let intern = |n: &str| -> Result<u32, XError> {
            Ok(xc.conn.intern_atom(false, n.as_bytes())?.reply()?.atom)
        };
        Ok(Atoms {
            active: intern("_NET_ACTIVE_WINDOW")?,
            window_type: intern("_NET_WM_WINDOW_TYPE")?,
            type_dock: intern("_NET_WM_WINDOW_TYPE_DOCK")?,
            strut_partial: intern("_NET_WM_STRUT_PARTIAL")?,
            strut: intern("_NET_WM_STRUT")?,
            net_name: intern("_NET_WM_NAME")?,
            utf8: intern("UTF8_STRING")?,
        })
    }
}

/// The two windows of a bottom panel (the system keyboard, the mouse pad): an override-redirect
/// window that never takes the focus, and a managed 1x1 dock-type window carrying the strut.
pub(crate) struct BottomWindows {
    pub(crate) win: XWindow,
    pub(crate) strut_win: XWindow,
    pub(crate) gc: u32,
    pub(crate) depth: u8,
    pub(crate) atoms: Atoms,
    pub(crate) root_h: i32,
}

/// Create (unmapped) the panel window at `r` and its strut window. `events` is the panel window's
/// event mask. Also selects PropertyChange on the root (`_NET_ACTIVE_WINDOW` changes).
pub(crate) fn create_bottom_windows(
    xc: &XConn,
    instance: &str,
    class: &str,
    strut_class: &str,
    r: Rect,
    events: EventMask,
) -> Result<BottomWindows, XError> {
    let conn = &xc.conn;
    let atoms = Atoms::intern(xc)?;
    let depth = conn
        .setup()
        .roots
        .first()
        .map(|s| s.root_depth)
        .unwrap_or(24);
    let root_h = conn.get_geometry(xc.root)?.reply()?.height as i32;
    let win = conn.generate_id()?;
    let aux = CreateWindowAux::new()
        .event_mask(events)
        .background_pixel(0x002B_3035)
        .override_redirect(1);
    conn.create_window(
        0,
        win,
        xc.root,
        r.x as i16,
        r.y as i16,
        r.w as u16,
        r.h as u16,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &aux,
    )?;
    let gc = conn.generate_id()?;
    conn.create_gc(gc, win, &CreateGCAux::new())?;
    let no_input = [1u32, 0, 0, 0, 0, 0, 0, 0, 0]; // WM_HINTS flags=InputHint, input=false
    let wm_class = format!("{instance}\0{class}\0");
    conn.change_property8(
        PropMode::REPLACE,
        win,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        wm_class.as_bytes(),
    )?;
    conn.change_property8(
        PropMode::REPLACE,
        win,
        AtomEnum::WM_NAME,
        AtomEnum::STRING,
        instance.as_bytes(),
    )?;
    conn.change_property8(
        PropMode::REPLACE,
        win,
        atoms.net_name,
        atoms.utf8,
        instance.as_bytes(),
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        win,
        AtomEnum::WM_HINTS,
        AtomEnum::WM_HINTS,
        &no_input,
    )?;
    let strut_win = conn.generate_id()?;
    conn.create_window(
        0,
        strut_win,
        xc.root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new(),
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        strut_win,
        atoms.window_type,
        AtomEnum::ATOM,
        &[atoms.type_dock],
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        strut_win,
        AtomEnum::WM_HINTS,
        AtomEnum::WM_HINTS,
        &no_input,
    )?;
    conn.change_property8(
        PropMode::REPLACE,
        strut_win,
        atoms.net_name,
        atoms.utf8,
        strut_class.as_bytes(),
    )?;
    conn.change_property8(
        PropMode::REPLACE,
        strut_win,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        format!("{strut_class}\0{strut_class}\0").as_bytes(),
    )?;
    conn.change_window_attributes(
        xc.root,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?;
    Ok(BottomWindows {
        win,
        strut_win,
        gc,
        depth,
        atoms,
        root_h,
    })
}

impl BottomWindows {
    /// Write the bottom strut for a panel of the given visible area.
    pub(crate) fn write_strut(&self, xc: &XConn, area: (i32, i32)) -> Result<(), XError> {
        let s = strut_partial(area, self.root_h);
        let c = &xc.conn;
        c.change_property32(
            PropMode::REPLACE,
            self.strut_win,
            self.atoms.strut_partial,
            AtomEnum::CARDINAL,
            &s,
        )?;
        c.change_property32(
            PropMode::REPLACE,
            self.strut_win,
            self.atoms.strut,
            AtomEnum::CARDINAL,
            &s[..4],
        )?;
        Ok(())
    }

    /// The root's `_NET_ACTIVE_WINDOW` (0 when unset or unreadable).
    pub(crate) fn read_active(&self, xc: &XConn) -> u32 {
        xc.conn
            .get_property(false, xc.root, self.atoms.active, AtomEnum::WINDOW, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().and_then(|mut v| v.next()))
            .unwrap_or(0)
    }

    /// Map (panel on top, at `r`) or unmap both windows.
    pub(crate) fn set_mapped(&self, xc: &XConn, on: bool, r: Rect) -> Result<(), XError> {
        if on {
            xc.conn.map_window(self.strut_win)?;
            xc.conn.map_window(self.win)?;
            xc.conn.configure_window(
                self.win,
                &ConfigureWindowAux::new()
                    .x(r.x)
                    .y(r.y)
                    .width(r.w as u32)
                    .height(r.h as u32)
                    .stack_mode(StackMode::ABOVE),
            )?;
        } else {
            xc.conn.unmap_window(self.win)?;
            xc.conn.unmap_window(self.strut_win)?;
        }
        Ok(())
    }

    pub(crate) fn place(&self, xc: &XConn, r: Rect) -> Result<(), XError> {
        xc.conn.configure_window(
            self.win,
            &ConfigureWindowAux::new()
                .x(r.x)
                .y(r.y)
                .width(r.w as u32)
                .height(r.h as u32),
        )?;
        Ok(())
    }

    pub(crate) fn raise(&self, xc: &XConn) -> Result<(), XError> {
        xc.conn.configure_window(
            self.win,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;
        Ok(())
    }

    pub(crate) fn server_mapped(&self, xc: &XConn) -> bool {
        xc.conn
            .get_window_attributes(self.win)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|a| a.map_state != MapState::UNMAPPED)
    }
}
