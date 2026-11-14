use super::{Nav, THUMB_MAX};
use crate::dock::app_for_class;
use walpad_ui::Canvas;
use walpad_x::XError;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, EventMask, ImageFormat,
    MapState, StackMode, Window as XWindow,
};

impl Nav {
    pub(super) fn read_active(&self) -> u32 {
        self.prop32(self.xc.root, self.atoms.active, AtomEnum::WINDOW, 1)
            .first()
            .copied()
            .unwrap_or(0)
    }

    fn prop32(&self, w: XWindow, atom: u32, ty: AtomEnum, len: u32) -> Vec<u32> {
        self.xc
            .conn
            .get_property(false, w, atom, ty, 0, len)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    fn class_of(&self, w: XWindow) -> Vec<String> {
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

    /// The app id of a window (None for the shell's and the system's own windows).
    fn app_of(&self, w: XWindow) -> Option<String> {
        if w == self.shell {
            return None;
        }
        app_for_class(&self.classes, &self.class_of(w))
    }

    /// Running apps with a window, most recently used first (top of the stack first), one window
    /// per app.
    pub(super) fn windows(&self) -> Vec<(String, XWindow)> {
        let mut list = self.prop32(self.xc.root, self.atoms.stacking, AtomEnum::WINDOW, 4096);
        if list.is_empty() {
            list = self.prop32(self.xc.root, self.atoms.client_list, AtomEnum::WINDOW, 4096);
            list.reverse();
        }
        let mut out: Vec<(String, XWindow)> = Vec::new();
        for w in list.into_iter().rev() {
            if let Some(app) = self.app_of(w) {
                if !out.iter().any(|(a, _)| *a == app) {
                    out.push((app, w));
                }
            }
        }
        out
    }

    /// The shell window changed size (rotation).
    pub(super) fn update_edge(&mut self) -> Result<(), XError> {
        let want = !self.blocked && self.active != 0 && self.active != self.shell;
        if want {
            if !self.edge_mapped {
                self.xc.conn.map_window(self.edge)?;
            }
            // the window manager restacks the apps it manages: stay above them
            self.xc.conn.configure_window(
                self.edge,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            )?;
        } else if self.edge_mapped {
            self.xc.conn.unmap_window(self.edge)?;
            self.swipe.set_area(self.area);
        }
        self.edge_mapped = want;
        self.xc.conn.flush()?;
        Ok(())
    }

    /// `XGetImage` of a viewable window, boxed down to at most `THUMB_MAX` per side.
    fn capture(&self, w: XWindow) -> Option<Canvas> {
        let c = &self.xc.conn;
        let at = c.get_window_attributes(w).ok()?.reply().ok()?;
        if at.map_state != MapState::VIEWABLE {
            return None;
        }
        let g = c.get_geometry(w).ok()?.reply().ok()?;
        let (sw, sh) = (g.width as usize, g.height as usize);
        if sw == 0 || sh == 0 {
            return None;
        }
        let img = c
            .get_image(ImageFormat::Z_PIXMAP, w, 0, 0, g.width, g.height, !0)
            .ok()?
            .reply()
            .ok()?;
        if img.data.len() != sw * sh * 4 {
            return None;
        }
        let k = (THUMB_MAX as f32 / sw as f32)
            .min(THUMB_MAX as f32 / sh as f32)
            .min(1.0);
        let tw = ((sw as f32 * k) as u32).max(1);
        let th = ((sh as f32 * k) as u32).max(1);
        Canvas::from_bgrx_scaled(sw, sh, &img.data, tw, th)
    }

    /// Refresh the cached thumbnail of the app in front.
    pub(super) fn capture_active(&mut self) {
        let w = self.active;
        if w == 0 || self.blocked {
            return;
        }
        let Some(app) = self.app_of(w) else { return };
        if let Some(t) = self.capture(w) {
            self.thumbs.insert(app, t);
        }
    }

    /// `_NET_ACTIVE_WINDOW` request to the window manager (source: pager), plus a restack.
    pub(super) fn activate(&self, w: XWindow) -> Result<(), XError> {
        let c = &self.xc.conn;
        let ev = ClientMessageEvent::new(32, w, self.atoms.active, [2, 0, 0, 0, 0]);
        c.send_event(
            false,
            self.xc.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        )?;
        c.configure_window(w, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        c.flush()?;
        Ok(())
    }
}
