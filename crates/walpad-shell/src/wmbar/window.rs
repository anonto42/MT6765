use super::model::{AppInfo, BarModel, Hit, BAR_CLASS, BAR_H, GRIP, GRIP_CLASS};
use crate::windows_ui::{read_prefs, WindowsPrefs};
use std::path::Path;
use std::time::Instant;
use walpad_hw::Root;
use walpad_proto::wm::WRect;
use walpad_ui::tokens::bg;
use walpad_ui::{Canvas, Rect};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConnectionExt as _, CreateGCAux, CreateWindowAux,
    EventMask, PropMode, Window as XWindow, WindowClass,
};
use x11rb::wrapper::ConnectionExt as _;

pub(super) struct Atoms {
    pub(super) active: u32,
    pub(super) client_list: u32,
    window_type: u32,
    type_dock: u32,
    pub(super) strut_partial: u32,
    pub(super) strut: u32,
    net_name: u32,
    utf8: u32,
}

/// WM_HINTS flags=InputHint, input=false: never take the keyboard focus from the app in front.
const NO_INPUT: [u32; 9] = [1, 0, 0, 0, 0, 0, 0, 0, 0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DragKind {
    Move,
    Resize,
}

pub(super) struct Drag {
    pub(super) kind: DragKind,
    pub(super) window: u32,
    pub(super) start: WRect,
    pub(super) at: (i32, i32),
    pub(super) moved: bool,
    pub(super) last_sent: Instant,
}

pub struct WmBar {
    pub(super) xc: XConn,
    pub(super) bar: XWindow,
    pub(super) grip: XWindow,
    pub(super) strut_win: XWindow,
    pub(super) gc: u32,
    pub(super) grip_gc: u32,
    pub(super) depth: u8,
    pub(super) atoms: Atoms,
    pub(super) apps: Vec<AppInfo>,
    pub(super) root: Root,
    /// visible area (width, height)
    pub(super) area: (i32, i32),
    pub(super) shown: bool,
    pub(super) grip_shown: bool,
    pub(super) model: BarModel,
    pub(super) prefs: WindowsPrefs,
    pub(super) pressed: Option<Hit>,
    pub(super) grip_pressed: bool,
    pub(super) drag: Option<Drag>,
    pub(super) canvas: Canvas,
    pub(super) grip_canvas: Canvas,
    pub(super) next_refresh: Instant,
    /// the active window is an app window (the periodic refresh runs)
    pub(super) app_up: bool,
    pub(super) app_id: String,
}

impl WmBar {
    pub fn open(
        socket: &Path,
        apps: Vec<AppInfo>,
        root: Root,
        shell_size: (i32, i32),
    ) -> Result<WmBar, XError> {
        let xc = walpad_x::connect(socket)?;
        let conn = &xc.conn;
        let intern = |n: &str| -> Result<u32, XError> {
            Ok(conn.intern_atom(false, n.as_bytes())?.reply()?.atom)
        };
        let atoms = Atoms {
            active: intern("_NET_ACTIVE_WINDOW")?,
            client_list: intern("_NET_CLIENT_LIST")?,
            window_type: intern("_NET_WM_WINDOW_TYPE")?,
            type_dock: intern("_NET_WM_WINDOW_TYPE_DOCK")?,
            strut_partial: intern("_NET_WM_STRUT_PARTIAL")?,
            strut: intern("_NET_WM_STRUT")?,
            net_name: intern("_NET_WM_NAME")?,
            utf8: intern("UTF8_STRING")?,
        };
        let depth = conn
            .setup()
            .roots
            .first()
            .map(|s| s.root_depth)
            .unwrap_or(24);
        let area = crate::dock::visible_area(shell_size.0, shell_size.1);
        let events = EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::BUTTON_MOTION
            | EventMask::EXPOSURE;
        let make = |r: Rect, name: &str| -> Result<(XWindow, u32), XError> {
            let win = conn.generate_id()?;
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
                &CreateWindowAux::new()
                    .event_mask(events)
                    .background_pixel(0x001A_1D21)
                    .override_redirect(1),
            )?;
            let gc = conn.generate_id()?;
            conn.create_gc(gc, win, &CreateGCAux::new())?;
            let class = format!("{name}\0{name}\0");
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
                name.as_bytes(),
            )?;
            conn.change_property8(
                PropMode::REPLACE,
                win,
                atoms.net_name,
                atoms.utf8,
                name.as_bytes(),
            )?;
            conn.change_property32(
                PropMode::REPLACE,
                win,
                atoms.window_type,
                AtomEnum::ATOM,
                &[atoms.type_dock],
            )?;
            conn.change_property32(
                PropMode::REPLACE,
                win,
                AtomEnum::WM_HINTS,
                AtomEnum::WM_HINTS,
                &NO_INPUT,
            )?;
            Ok((win, gc))
        };
        let (bar, gc) = make(Rect::new(0, 0, area.0, BAR_H), BAR_CLASS)?;
        let (grip, grip_gc) = make(Rect::new(0, 0, GRIP, GRIP), GRIP_CLASS)?;
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
            &NO_INPUT,
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            strut_win,
            atoms.net_name,
            atoms.utf8,
            b"walpad-wmbar-strut",
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            strut_win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"walpad-wmbar-strut\0walpad-wmbar-strut\0",
        )?;
        let rootw = xc.root;
        conn.change_window_attributes(
            rootw,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        conn.flush()?;
        let b = WmBar {
            xc,
            bar,
            grip,
            strut_win,
            gc,
            grip_gc,
            depth,
            atoms,
            apps,
            prefs: read_prefs(&root),
            root,
            area,
            shown: false,
            grip_shown: false,
            model: BarModel {
                width: area.0,
                tabs: Vec::new(),
                active: 0,
                title: String::new(),
                can_new: false,
                free: false,
            },
            pressed: None,
            grip_pressed: false,
            drag: None,
            canvas: Canvas::new(area.0 as u32, BAR_H as u32, bg()).ok_or("bar canvas")?,
            grip_canvas: Canvas::new(GRIP as u32, GRIP as u32, bg()).ok_or("grip canvas")?,
            next_refresh: Instant::now(),
            app_up: false,
            app_id: String::new(),
        };
        b.write_strut()?;
        Ok(b)
    }
}
