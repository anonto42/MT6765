use super::keys::{parse_key_spec, TouchHit};
use super::{XBridge, XError, R};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, AtomEnum, ConnectionExt as _, Window};
use x11rb::protocol::xtest::ConnectionExt as _;

impl XBridge {
    // ---- pointer (touch bridge)
    pub fn pointer_move(&mut self, x: i32, y: i32) -> R<()> {
        self.conn.xtest_fake_input(
            xproto::MOTION_NOTIFY_EVENT,
            0,
            0,
            self.root,
            x as i16,
            y as i16,
            0,
        )?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn button(&mut self, press: bool) -> R<()> {
        self.button_n(1, press)
    }

    /// Press or release pointer button `n` (XTEST). Button 1 is tracked for `release_if_down`.
    pub fn button_n(&mut self, n: u8, press: bool) -> R<()> {
        let ty = if press {
            xproto::BUTTON_PRESS_EVENT
        } else {
            xproto::BUTTON_RELEASE_EVENT
        };
        self.conn.xtest_fake_input(ty, n, 0, self.root, 0, 0, 0)?;
        self.conn.flush()?;
        if n == 1 {
            self.button_down = press;
        }
        Ok(())
    }

    /// One wheel click (button 4 up, 5 down, 6 left, 7 right) at the pointer, with Control held
    /// when `ctrl` (pinch zoom).
    pub fn wheel(&mut self, button: u8, ctrl: bool) -> R<()> {
        let ctl = if ctrl {
            self.keycode(0xffe3).map(|k| k.0)
        } else {
            None
        };
        if let Some(c) = ctl {
            self.conn
                .xtest_fake_input(xproto::KEY_PRESS_EVENT, c, 0, self.root, 0, 0, 0)?;
        }
        self.conn
            .xtest_fake_input(xproto::BUTTON_PRESS_EVENT, button, 0, self.root, 0, 0, 0)?;
        self.conn
            .xtest_fake_input(xproto::BUTTON_RELEASE_EVENT, button, 0, self.root, 0, 0, 0)?;
        if let Some(c) = ctl {
            self.conn
                .xtest_fake_input(xproto::KEY_RELEASE_EVENT, c, 0, self.root, 0, 0, 0)?;
        }
        self.conn.flush()?;
        Ok(())
    }

    /// What is under a touch at this root position: used once per touch to choose between the
    /// mouse and the touchscreen behaviour.
    pub fn touch_hit(&self, x: i32, y: i32) -> R<TouchHit> {
        let children = self.conn.query_tree(self.root)?.reply()?.children;
        // pipeline: every child's attributes and geometry in flight at once
        let asked: Vec<_> = children
            .iter()
            .rev()
            .filter_map(|&w| {
                let a = self.conn.get_window_attributes(w).ok()?;
                let g = self.conn.get_geometry(w).ok()?;
                Some((w, a, g))
            })
            .collect();
        for (w, a, g) in asked {
            let (Ok(a), Ok(g)) = (a.reply(), g.reply()) else {
                continue;
            };
            if a.map_state != xproto::MapState::VIEWABLE
                || a.class != xproto::WindowClass::INPUT_OUTPUT
                || g.width <= 1
                || g.height <= 1
            {
                continue;
            }
            let b = g.border_width as i32;
            let (gx, gy) = (g.x as i32 - b, g.y as i32 - b);
            if x < gx
                || y < gy
                || x >= gx + g.width as i32 + 2 * b
                || y >= gy + g.height as i32 + 2 * b
            {
                continue;
            }
            let class = self.class_in(w, 2);
            if class
                .iter()
                .any(|c| c.eq_ignore_ascii_case("walpad-cursor"))
            {
                continue; // the pad's pointer marker: clicks pass through it
            }
            return Ok(if class.is_empty() {
                TouchHit::None
            } else if class
                .iter()
                .any(|c| c.to_ascii_lowercase().starts_with("walpad"))
            {
                TouchHit::Shell
            } else {
                TouchHit::App(class)
            });
        }
        Ok(TouchHit::None)
    }

    /// WM_CLASS of the window, or of its first descendant (up to `depth` levels) that has one
    /// (the window manager's frame has none, its client does).
    fn class_in(&self, w: Window, depth: u32) -> Vec<String> {
        let own = self
            .conn
            .get_property(false, w, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|p| {
                String::from_utf8_lossy(&p.value)
                    .split('\0')
                    .filter(|x| !x.is_empty())
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !own.is_empty() || depth == 0 {
            return own;
        }
        let kids = self
            .conn
            .query_tree(w)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|t| t.children)
            .unwrap_or_default();
        for k in kids.into_iter().rev() {
            let c = self.class_in(k, depth - 1);
            if !c.is_empty() {
                return c;
            }
        }
        Vec::new()
    }

    /// Release a stuck button (e.g. before the bridge is dropped).
    pub fn release_if_down(&mut self) {
        if self.button_down {
            let _ = self.button(false);
        }
    }

    // ---- keyboard
    pub(super) fn keycode(&self, sym: u32) -> Option<(u8, bool)> {
        for (i, chunk) in self.syms.chunks(self.per.max(1)).enumerate() {
            for (col, s) in chunk.iter().enumerate().take(2) {
                if *s == sym {
                    return Some((self.min_keycode.checked_add(i as u8)?, col == 1));
                }
            }
        }
        None
    }

    /// Send a key spec such as `ctrl+Tab` through XTEST using the server's keyboard mapping.
    pub fn send_key(&mut self, spec: &str) -> R<()> {
        let (mods, key) =
            parse_key_spec(spec).ok_or_else(|| XError(format!("bad key spec {spec:?}")))?;
        let (kc, shifted) = self
            .keycode(key)
            .ok_or_else(|| XError(format!("no keycode for {spec:?}")))?;
        let mut codes = Vec::new();
        for m in mods {
            codes.push(
                self.keycode(m)
                    .ok_or_else(|| XError("no modifier keycode".into()))?
                    .0,
            );
        }
        if shifted {
            if let Some((s, _)) = self.keycode(0xffe1) {
                codes.push(s);
            }
        }
        for c in &codes {
            self.conn
                .xtest_fake_input(xproto::KEY_PRESS_EVENT, *c, 0, self.root, 0, 0, 0)?;
        }
        self.conn
            .xtest_fake_input(xproto::KEY_PRESS_EVENT, kc, 0, self.root, 0, 0, 0)?;
        self.conn
            .xtest_fake_input(xproto::KEY_RELEASE_EVENT, kc, 0, self.root, 0, 0, 0)?;
        for c in codes.iter().rev() {
            self.conn
                .xtest_fake_input(xproto::KEY_RELEASE_EVENT, *c, 0, self.root, 0, 0, 0)?;
        }
        self.conn.flush()?;
        Ok(())
    }
}
