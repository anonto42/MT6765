use super::keys::XWindow;
use super::{XBridge, R};
use walpad_proto::wm::{self, WRect};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    self, AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, EventMask, Window,
};

/// An app window with the facts the layout engine and `windows` need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinInfo {
    pub win: XWindow,
    pub title: String,
    pub rect: WRect,
    pub maximized: bool,
}

impl XBridge {
    /// Size of the X screen (the root window).
    pub fn screen_size(&self) -> R<(i32, i32)> {
        let g = self.conn.get_geometry(self.root)?.reply()?;
        Ok((g.width as i32, g.height as i32))
    }

    /// `_NET_WM_STRUT` of every mapped window under the root (depth <= 3): the dock, the
    /// keyboard strip, the mouse pad, the window bar and the rotation strut. Order: left, right,
    /// top, bottom. Falls back to the first four values of `_NET_WM_STRUT_PARTIAL`.
    pub fn struts(&self) -> R<Vec<[u32; 4]>> {
        let partial = self.atom("_NET_WM_STRUT_PARTIAL")?;
        let plain = self.atom("_NET_WM_STRUT")?;
        let mut out = Vec::new();
        let mut level = vec![self.root];
        for _ in 0..3 {
            let mut next = Vec::new();
            for w in level {
                for c in self.conn.query_tree(w)?.reply()?.children {
                    next.push(c);
                    let mapped = self
                        .conn
                        .get_window_attributes(c)?
                        .reply()
                        .map(|a| a.map_state == xproto::MapState::VIEWABLE)
                        .unwrap_or(false);
                    if !mapped {
                        continue;
                    }
                    for atom in [partial, plain] {
                        let Ok(p) = self
                            .conn
                            .get_property(false, c, atom, AtomEnum::CARDINAL, 0, 12)?
                            .reply()
                        else {
                            continue;
                        };
                        if let Some(v) = p.value32().map(|v| v.collect::<Vec<u32>>()) {
                            if v.len() >= 4 {
                                out.push([v[0], v[1], v[2], v[3]]);
                                break;
                            }
                        }
                    }
                }
            }
            level = next;
        }
        Ok(out)
    }

    /// Work area: the screen minus the struts of every mapped window; the visible panel of the
    /// orientation when X reports nothing usable.
    pub fn work_area(&self, orientation: u16) -> WRect {
        let screen = self.screen_size().unwrap_or((1280, 1280));
        match self.struts() {
            Ok(s) if !s.is_empty() => wm::workarea(screen, &s),
            _ => wm::panel(orientation),
        }
    }

    /// `_NET_CLIENT_LIST_STACKING`: managed windows, back to front (empty without a manager).
    pub fn stacking(&self) -> R<Vec<Window>> {
        let atom = self.atom("_NET_CLIENT_LIST_STACKING")?;
        let p = self
            .conn
            .get_property(false, self.root, atom, AtomEnum::WINDOW, 0, 4096)?
            .reply()?;
        Ok(p.value32().map(|v| v.collect()).unwrap_or_default())
    }

    /// The window `_NET_ACTIVE_WINDOW` names (0 = none).
    pub fn active_window(&self) -> R<Window> {
        let atom = self.atom("_NET_ACTIVE_WINDOW")?;
        let p = self
            .conn
            .get_property(false, self.root, atom, AtomEnum::WINDOW, 0, 1)?
            .reply()?;
        Ok(p.value32().and_then(|mut v| v.next()).unwrap_or(0))
    }

    /// Title (`_NET_WM_NAME`, else `WM_NAME`), at most 128 bytes; empty when unreadable.
    pub fn title(&self, w: Window) -> String {
        let utf8 = self.atom("UTF8_STRING").unwrap_or(0);
        let net = self.atom("_NET_WM_NAME").unwrap_or(0);
        let read = |atom: u32, ty: u32| -> Option<String> {
            let p = self
                .conn
                .get_property(false, w, atom, ty, 0, 32)
                .ok()?
                .reply()
                .ok()?;
            (!p.value.is_empty()).then(|| String::from_utf8_lossy(&p.value).into_owned())
        };
        read(net, utf8)
            .or_else(|| read(AtomEnum::WM_NAME.into(), AtomEnum::STRING.into()))
            .unwrap_or_default()
    }

    /// Position and size of a window in root coordinates.
    pub fn geometry(&self, w: Window) -> R<WRect> {
        let g = self.conn.get_geometry(w)?.reply()?;
        let t = self
            .conn
            .translate_coordinates(w, self.root, 0, 0)?
            .reply()?;
        Ok(WRect::new(
            t.dst_x as i32,
            t.dst_y as i32,
            g.width as i32,
            g.height as i32,
        ))
    }

    /// Add or remove the maximised state (`_NET_WM_STATE` request to the window manager).
    pub fn set_maximized(&mut self, w: Window, on: bool) -> R<()> {
        let state = self.atom("_NET_WM_STATE")?;
        let vert = self.atom("_NET_WM_STATE_MAXIMIZED_VERT")?;
        let horz = self.atom("_NET_WM_STATE_MAXIMIZED_HORZ")?;
        let ev = ClientMessageEvent::new(32, w, state, [on as u32, vert, horz, 2, 0]);
        self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        )?;
        self.conn.flush()?;
        Ok(())
    }

    /// Is the window maximised in both directions?
    pub fn is_maximized(&self, w: Window) -> bool {
        let (Ok(state), Ok(vert), Ok(horz)) = (
            self.atom("_NET_WM_STATE"),
            self.atom("_NET_WM_STATE_MAXIMIZED_VERT"),
            self.atom("_NET_WM_STATE_MAXIMIZED_HORZ"),
        ) else {
            return false;
        };
        self.conn
            .get_property(false, w, state, AtomEnum::ATOM, 0, 32)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|p| p.value32().map(|v| v.collect::<Vec<u32>>()))
            .is_some_and(|v| v.contains(&vert) && v.contains(&horz))
    }

    /// Place a window: `_NET_MOVERESIZE_WINDOW` (EWMH 1.5; source indication 2 = tool), which the
    /// window manager honours for managed windows; without one the plain `ConfigureWindow`.
    pub fn move_resize(&mut self, w: Window, r: WRect) -> R<()> {
        let atom = self.atom("_NET_MOVERESIZE_WINDOW")?;
        // gravity 0 (the window's own) | x | y | width | height | source indication 2
        let flags: u32 = (0xf << 8) | (2 << 12);
        let ev = ClientMessageEvent::new(
            32,
            w,
            atom,
            [flags, r.x as u32, r.y as u32, r.w as u32, r.h as u32],
        );
        self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        )?;
        self.conn.configure_window(
            w,
            &ConfigureWindowAux::new()
                .x(r.x)
                .y(r.y)
                .width(r.w as u32)
                .height(r.h as u32),
        )?;
        self.conn.flush()?;
        Ok(())
    }

    /// App windows with title, geometry and close support, back to front; the shell's own
    /// windows (WM_CLASS starting with `walpad`) are left out.
    pub fn app_windows(&self) -> R<Vec<WinInfo>> {
        let stack = self.stacking().unwrap_or_default();
        let mut out: Vec<WinInfo> = Vec::new();
        for w in self.windows()? {
            if w.class
                .iter()
                .any(|c| c.to_ascii_lowercase().starts_with("walpad"))
            {
                continue;
            }
            let Ok(rect) = self.geometry(w.id) else {
                continue;
            };
            out.push(WinInfo {
                title: self.title(w.id),
                maximized: self.is_maximized(w.id),
                rect,
                win: w,
            });
        }
        // back to front; windows missing from the stacking list (no manager) keep their order
        out.sort_by_key(|i| stack.iter().position(|s| *s == i.win.id).unwrap_or(0));
        Ok(out)
    }
}
