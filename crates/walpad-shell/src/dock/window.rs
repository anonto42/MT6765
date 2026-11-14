use super::layout::{
    dock_rect, handle_rect, visible_area, DockButton, DOCK_H, HANDLE_CLASS, HANDLE_H, HANDLE_W,
    WM_CLASS_CLASS, WM_CLASS_INSTANCE,
};
use super::state::Fold;
use super::window_ops::shape_handle;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
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
    window_type: u32,
    type_dock: u32,
    pub(super) strut_partial: u32,
    pub(super) strut: u32,
    net_name: u32,
    utf8: u32,
}

/// What is pressed (highlighted until the release).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pressed {
    Handle,
    Button(DockButton),
}

pub(super) struct HandleDrag {
    pub(super) start_root: (i32, i32),
    pub(super) origin: (i32, i32),
    pub(super) moved: bool,
}

pub struct Dock {
    pub(super) xc: XConn,
    /// the strip (override-redirect: openbox moved a managed strip to the top of the screen)
    pub(super) win: XWindow,
    /// the arrow handle (override-redirect, SHAPE-d with rounded top corners when available)
    pub(super) handle: XWindow,
    /// 1x1 managed window (kept unmapped; the compact icon-only strip floats without resizing apps)
    pub(super) strut_win: XWindow,
    pub(super) shell: XWindow,
    pub(super) gc: u32,
    pub(super) handle_gc: u32,
    pub(super) depth: u8,
    pub(super) atoms: Atoms,
    pub(super) apps: Vec<(String, String)>,
    pub(super) root_h: i32,
    pub(super) area: (i32, i32),
    /// Height of the system keyboard / mouse pad below the dock (0 while hidden).
    pub(super) kbd_h: i32,
    /// User-dragged handle position (None = default right side).
    pub(super) handle_pos: Option<(i32, i32)>,
    /// Active drag on the handle window.
    pub(super) drag: Option<HandleDrag>,
    /// the handle is up (a foreign app is active)
    pub(super) mapped: bool,
    pub(super) fold: Fold,
    pub(super) active: u32,
    pub(super) pressed: Option<Pressed>,
    pub(super) canvas: Canvas,
    pub(super) handle_canvas: Canvas,
    epoch: Instant,
}

/// WM_HINTS flags=InputHint, input=false: never take the keyboard focus from the app in front.
pub(super) const NO_INPUT: [u32; 9] = [1, 0, 0, 0, 0, 0, 0, 0, 0];

impl Dock {
    pub fn open(
        socket: &Path,
        shell: XWindow,
        apps: Vec<(String, String)>,
        shell_size: (i32, i32),
    ) -> Result<Dock, XError> {
        let xc = walpad_x::connect(socket)?;
        let conn = &xc.conn;
        let intern = |n: &str| -> Result<u32, XError> {
            Ok(conn.intern_atom(false, n.as_bytes())?.reply()?.atom)
        };
        let atoms = Atoms {
            active: intern("_NET_ACTIVE_WINDOW")?,
            window_type: intern("_NET_WM_WINDOW_TYPE")?,
            type_dock: intern("_NET_WM_WINDOW_TYPE_DOCK")?,
            strut_partial: intern("_NET_WM_STRUT_PARTIAL")?,
            strut: intern("_NET_WM_STRUT")?,
            net_name: intern("_NET_WM_NAME")?,
            utf8: intern("UTF8_STRING")?,
        };
        let setup = conn.setup();
        let depth = setup.roots.first().map(|s| s.root_depth).unwrap_or(24);
        let root_h = conn.get_geometry(xc.root)?.reply()?.height as i32;
        let area = visible_area(shell_size.0, shell_size.1);
        let events = EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::BUTTON1_MOTION
            | EventMask::POINTER_MOTION
            | EventMask::EXPOSURE
            | EventMask::STRUCTURE_NOTIFY;
        let make = |r: Rect, instance: &str, class: &str| -> Result<(XWindow, u32), XError> {
            let win = conn.generate_id()?;
            let aux = CreateWindowAux::new()
                .event_mask(events)
                .background_pixel(0x001A_1D21)
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
        let r = dock_rect(area, 0);
        let (win, gc) = make(r, WM_CLASS_INSTANCE, WM_CLASS_CLASS)?;
        let (handle, handle_gc) = make(handle_rect(area, 0, false), HANDLE_CLASS, HANDLE_CLASS)?;
        shape_handle(&xc, handle);
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
            b"walpad-dock-strut",
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            strut_win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"walpad-dock-strut\0walpad-dock-strut\0",
        )?;
        let mut d = Dock {
            xc,
            win,
            handle,
            strut_win,
            shell,
            gc,
            handle_gc,
            depth,
            atoms,
            apps,
            root_h,
            area,
            kbd_h: 0,
            handle_pos: None,
            drag: None,
            mapped: false,
            fold: Fold::default(),
            active: 0,
            pressed: None,
            canvas: Canvas::new(r.w as u32, DOCK_H as u32, bg()).ok_or("dock canvas")?,
            handle_canvas: Canvas::new(HANDLE_W as u32, HANDLE_H as u32, bg())
                .ok_or("dock canvas")?,
            epoch: Instant::now(),
        };
        d.write_strut()?;
        let root = d.xc.root;
        d.xc.conn.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        d.active = d.read_active();
        d.xc.conn.flush()?;
        Ok(d)
    }

    pub fn wait_fd(&self) -> &UnixStream {
        &self.xc.wait_fd
    }

    /// The strip window.
    pub fn window(&self) -> XWindow {
        self.win
    }

    /// The handle window.
    pub fn handle_window(&self) -> XWindow {
        self.handle
    }

    /// The handle is up (a foreign app is in front).
    pub fn visible(&self) -> bool {
        self.mapped
    }

    /// The strip is expanded.
    pub fn expanded(&self) -> bool {
        self.mapped && self.fold.expanded
    }

    pub(super) fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    /// Time until the strip folds back by itself (None while collapsed).
    pub fn next_deadline(&self) -> Option<Duration> {
        self.fold
            .deadline()
            .map(|t| Duration::from_millis(t.saturating_sub(self.now_ms())))
    }
}
