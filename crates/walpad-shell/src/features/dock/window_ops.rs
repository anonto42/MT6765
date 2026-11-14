use super::layout::*;
use super::window::Dock;
use walpad_ui::Rect;
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, EventMask, MapState,
    PropMode, StackMode, Window as XWindow,
};
use x11rb::wrapper::ConnectionExt as _;

impl Dock {
    pub(super) fn write_strut(&self) -> Result<(), XError> {
        let s = strut_partial(self.area, self.kbd_h, self.root_h);
        let c = &self.xc.conn;
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

    pub(super) fn read_active(&self) -> u32 {
        self.xc
            .conn
            .get_property(
                false,
                self.xc.root,
                self.atoms.active,
                AtomEnum::WINDOW,
                0,
                1,
            )
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().and_then(|mut v| v.next()))
            .unwrap_or(0)
    }

    pub(super) fn class_of(&self, w: XWindow) -> Vec<String> {
        self.xc
            .conn
            .get_property(false, w, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| {
                r.value
                    .split(|b| *b == 0)
                    .filter(|s| !s.is_empty())
                    .map(|s| String::from_utf8_lossy(s).into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn current_handle_rect(&self) -> Rect {
        let (x, y) = match self.handle_pos {
            Some(pos) => clamp_handle_pos(pos, self.area, self.kbd_h),
            None => default_handle_pos(self.area, self.kbd_h),
        };
        Rect::new(x, y, HANDLE_W, HANDLE_H)
    }

    /// The shell window changed size (rotation): move and clamp the handle and the strip.
    pub fn set_shell_size(&mut self, w: i32, h: i32) -> Result<(), XError> {
        let area = visible_area(w, h);
        if area == self.area {
            return Ok(());
        }
        self.area = area;
        if let Some(pos) = self.handle_pos {
            self.handle_pos = Some(clamp_handle_pos(pos, area, self.kbd_h));
        }
        self.place()
    }

    /// The system keyboard / mouse pad appeared (its height) or went away (0): stack handle and
    /// strip directly above it and extend the strut over both.
    pub fn set_keyboard_height(&mut self, kbd_h: i32) -> Result<(), XError> {
        if kbd_h == self.kbd_h {
            return Ok(());
        }
        self.kbd_h = kbd_h;
        self.place()
    }

    pub(super) fn configure(&self, w: XWindow, r: Rect, raise: bool) -> Result<(), XError> {
        let mut aux = ConfigureWindowAux::new()
            .x(r.x)
            .y(r.y)
            .width(r.w as u32)
            .height(r.h as u32);
        if raise {
            aux = aux.stack_mode(StackMode::ABOVE);
        }
        self.xc.conn.configure_window(w, &aux)?;
        Ok(())
    }

    /// Put both windows where the current state wants them.
    fn place(&mut self) -> Result<(), XError> {
        let hr = self.current_handle_rect();
        let dr = dock_rect_for_handle(self.area, hr);
        self.configure(self.win, dr, false)?;
        self.configure(self.handle, hr, false)?;
        self.xc.conn.flush()?;
        Ok(())
    }

    /// Show or hide everything (the handle follows the active window; the strip starts folded).
    pub(super) fn set_mapped(&mut self, on: bool) -> Result<(), XError> {
        if on == self.mapped {
            if on {
                // the window manager restacks the apps it manages; keep the unmanaged windows on
                // top
                self.raise()?;
                self.xc.conn.flush()?;
            }
            return Ok(());
        }
        self.mapped = on;
        self.pressed = None;
        self.drag = None;
        if on {
            self.fold.collapse();
            self.xc.conn.map_window(self.handle)?;
            // openbox places a newly mapped window itself: ask for our place again once mapped,
            // and put it on top (a remapped window keeps its old place in the stack)
            self.configure(self.handle, self.current_handle_rect(), true)?;
        } else {
            self.fold.collapse();
            self.xc.conn.unmap_window(self.handle)?;
            self.xc.conn.unmap_window(self.win)?;
            self.xc.conn.unmap_window(self.strut_win)?;
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    fn raise(&self) -> Result<(), XError> {
        let above = ConfigureWindowAux::new().stack_mode(StackMode::ABOVE);
        if self.fold.expanded {
            self.xc.conn.configure_window(self.win, &above)?;
        } else {
            self.xc.conn.configure_window(self.handle, &above)?;
        }
        Ok(())
    }

    /// Raise and focus the shell: the standard `_NET_ACTIVE_WINDOW` request to the window manager,
    /// plus a plain restack (works without one).
    pub(super) fn raise_shell(&self) -> Result<(), XError> {
        let c = &self.xc.conn;
        let ev = ClientMessageEvent::new(32, self.shell, self.atoms.active, [2, 0, 0, 0, 0]);
        c.send_event(
            false,
            self.xc.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        )?;
        c.configure_window(
            self.shell,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;
        c.flush()?;
        Ok(())
    }

    /// For tests and diagnostics: is the strip currently mapped according to the server?
    pub fn server_mapped(&self) -> bool {
        self.xc
            .conn
            .get_window_attributes(self.win)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|a| a.map_state != MapState::UNMAPPED)
    }
}

/// Round the handle's top corners with the SHAPE extension (a plain rectangle without it).
pub(super) fn shape_handle(xc: &XConn, win: XWindow) {
    use x11rb::connection::RequestConnection as _;
    use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
    use x11rb::protocol::xproto::{ClipOrdering, Rectangle};
    let conn = &xc.conn;
    if !matches!(
        conn.extension_information(x11rb::protocol::shape::X11_EXTENSION_NAME),
        Ok(Some(_))
    ) {
        return;
    }
    let rects: Vec<Rectangle> = handle_shape()
        .iter()
        .map(|r| Rectangle {
            x: r.x as i16,
            y: r.y as i16,
            width: r.w as u16,
            height: r.h as u16,
        })
        .collect();
    let _ = conn.shape_rectangles(
        SO::SET,
        SK::BOUNDING,
        ClipOrdering::UNSORTED,
        win,
        0,
        0,
        &rects,
    );
}
