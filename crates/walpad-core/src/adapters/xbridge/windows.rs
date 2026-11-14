use super::keys::{back_spec, class_matches, XWindow};
use super::{XBridge, R};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    self, AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, EventMask, Window,
};

impl XBridge {
    /// Top-level client windows: _NET_CLIENT_LIST when the window manager provides it, else the
    /// first three levels of the window tree.
    fn client_windows(&self) -> R<Vec<Window>> {
        let atom = self.atom("_NET_CLIENT_LIST")?;
        let p = self
            .conn
            .get_property(false, self.root, atom, AtomEnum::WINDOW, 0, 4096)?
            .reply()?;
        if let Some(v) = p.value32() {
            let l: Vec<Window> = v.collect();
            if !l.is_empty() {
                return Ok(l);
            }
        }
        let mut out = Vec::new();
        let mut level = vec![self.root];
        for _ in 0..3 {
            let mut next = Vec::new();
            for w in level {
                for c in self.conn.query_tree(w)?.reply()?.children {
                    out.push(c);
                    next.push(c);
                }
            }
            level = next;
        }
        Ok(out)
    }

    /// Windows that have a WM_CLASS, with pid and close support.
    pub fn windows(&self) -> R<Vec<XWindow>> {
        let pid_atom = self.atom("_NET_WM_PID")?;
        let proto = self.atom("WM_PROTOCOLS")?;
        let del = self.atom("WM_DELETE_WINDOW")?;
        let mut out = Vec::new();
        for w in self.client_windows()? {
            let Ok(c) =
                self.conn
                    .get_property(false, w, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)
            else {
                continue;
            };
            let Ok(c) = c.reply() else { continue };
            if c.value.is_empty() {
                continue;
            }
            let class = String::from_utf8_lossy(&c.value)
                .split('\0')
                .filter(|x| !x.is_empty())
                .map(String::from)
                .collect();
            let pid = self
                .conn
                .get_property(false, w, pid_atom, AtomEnum::CARDINAL, 0, 1)?
                .reply()?
                .value32()
                .and_then(|mut v| v.next());
            let deletable = self
                .conn
                .get_property(false, w, proto, AtomEnum::ATOM, 0, 32)?
                .reply()?
                .value32()
                .is_some_and(|mut v| v.any(|a| a == del));
            out.push(XWindow {
                id: w,
                class,
                pid,
                deletable,
            });
        }
        Ok(out)
    }

    /// WM_CLASS of the window `_NET_ACTIVE_WINDOW` names (empty when none or unreadable).
    pub fn active_class(&self) -> R<Vec<String>> {
        let atom = self.atom("_NET_ACTIVE_WINDOW")?;
        let p = self
            .conn
            .get_property(false, self.root, atom, AtomEnum::WINDOW, 0, 1)?
            .reply()?;
        let Some(w) = p.value32().and_then(|mut v| v.next()).filter(|w| *w != 0) else {
            return Ok(Vec::new());
        };
        let c = self
            .conn
            .get_property(false, w, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)?
            .reply()?;
        Ok(String::from_utf8_lossy(&c.value)
            .split('\0')
            .filter(|x| !x.is_empty())
            .map(String::from)
            .collect())
    }

    /// The Back control: Alt+Left or Esc to the active window (see `back_spec`). Returns the spec.
    pub fn back(&mut self) -> R<&'static str> {
        let spec = back_spec(&self.active_class()?);
        self.send_key(spec)?;
        Ok(spec)
    }

    /// Ask a window to close (WM_PROTOCOLS / WM_DELETE_WINDOW ClientMessage, ICCCM 4.2.8.1).
    pub fn close_window(&mut self, w: Window) -> R<()> {
        let proto = self.atom("WM_PROTOCOLS")?;
        let del = self.atom("WM_DELETE_WINDOW")?;
        let ev = ClientMessageEvent::new(32, w, proto, [del, 0, 0, 0, 0]);
        self.conn.send_event(false, w, EventMask::NO_EVENT, ev)?;
        self.conn.flush()?;
        Ok(())
    }

    // ---- focus
    pub(super) fn activate(&mut self, w: Window) -> R<()> {
        let atom = self.atom("_NET_ACTIVE_WINDOW")?;
        let ev = ClientMessageEvent::new(32, w, atom, [2, 0, 0, 0, 0]);
        self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        )?;
        self.conn.map_window(w)?;
        self.conn.configure_window(
            w,
            &ConfigureWindowAux::new().stack_mode(xproto::StackMode::ABOVE),
        )?;
        self.conn.flush()?;
        Ok(())
    }

    /// Raise the first window whose WM_CLASS matches. Returns false if none exists.
    pub fn focus_class(&mut self, needle: &str) -> R<bool> {
        let w = self
            .windows()?
            .into_iter()
            .find(|w| class_matches(&w.class, needle));
        match w {
            Some(w) => self.activate(w.id).map(|_| true),
            None => Ok(false),
        }
    }

    /// Raise the window with this title (WM_NAME), e.g. "WalPad Home".
    pub fn focus_title(&mut self, title: &str) -> R<bool> {
        let mut level = vec![self.root];
        for _ in 0..3 {
            let mut next = Vec::new();
            for w in level {
                let children = self.conn.query_tree(w)?.reply()?.children;
                for c in children {
                    let name = self
                        .conn
                        .get_property(false, c, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 64)?
                        .reply()?;
                    if name.value == title.as_bytes() {
                        return self.activate(c).map(|_| true);
                    }
                    next.push(c);
                }
            }
            level = next;
        }
        Ok(false)
    }

    /// Raise and focus a window by id (`_NET_ACTIVE_WINDOW`).
    pub fn focus_window(&mut self, w: Window) -> R<()> {
        self.activate(w)
    }
}
