//! X bridge (M3.2/M3.3): struts, keyboard docking, key and pointer injection (XTEST) and window
//! focus, over a stream whose every wait has a deadline (a hung X server cannot hang the core).
//! The core connects lazily and drops/reconnects the bridge when X goes away (session restarts).
use rustix::event::{poll, PollFd, PollFlags, Timespec};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    self, AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, CreateWindowAux,
    EventMask, PropMode, Window, WindowClass,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::{PollMode, RustConnection, Stream};
use x11rb::utils::RawFdContainer;
use x11rb::wrapper::ConnectionExt as _;

const X_TIMEOUT_S: i64 = 2;

/// Bridge error (message only). Deliberately not `Display`, so the blanket `From` below is coherent.
#[derive(Debug)]
pub struct XError(pub String);
impl<E: std::fmt::Display> From<E> for XError {
    fn from(e: E) -> Self {
        XError(e.to_string())
    }
}
type R<T> = Result<T, XError>;

/// Unix stream with a deadline on every poll.
pub struct TimedStream(UnixStream);

impl Stream for TimedStream {
    fn poll(&self, mode: PollMode) -> std::io::Result<()> {
        let mut flags = PollFlags::empty();
        if mode.readable() {
            flags |= PollFlags::IN;
        }
        if mode.writable() {
            flags |= PollFlags::OUT;
        }
        loop {
            let mut fds = [PollFd::new(&self.0, flags)];
            let t = Timespec {
                tv_sec: X_TIMEOUT_S,
                tv_nsec: 0,
            };
            match poll(&mut fds, Some(&t)) {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        ErrorKind::TimedOut,
                        "X server did not respond",
                    ))
                }
                Ok(_) => return Ok(()),
                Err(rustix::io::Errno::INTR) => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }
    fn read(&self, buf: &mut [u8], _fds: &mut Vec<RawFdContainer>) -> std::io::Result<usize> {
        (&self.0).read(buf)
    }
    fn write(&self, buf: &[u8], _fds: &mut Vec<RawFdContainer>) -> std::io::Result<usize> {
        (&self.0).write(buf)
    }
}

/// X keysyms for the names used in keys.json (xdotool syntax).
pub fn keysym(name: &str) -> Option<u32> {
    let n = name.trim();
    let named = match n.to_ascii_lowercase().as_str() {
        "return" | "enter" => 0xff0d,
        "tab" => 0xff09,
        "escape" | "esc" => 0xff1b,
        "backspace" => 0xff08,
        "delete" => 0xffff,
        "home" => 0xff50,
        "left" => 0xff51,
        "up" => 0xff52,
        "right" => 0xff53,
        "down" => 0xff54,
        "prior" | "page_up" | "pageup" => 0xff55,
        "next" | "page_down" | "pagedown" => 0xff56,
        "end" => 0xff57,
        "space" => 0x20,
        "ctrl" | "control" => 0xffe3,
        "shift" => 0xffe1,
        "alt" => 0xffe9,
        "super" => 0xffeb,
        _ => 0,
    };
    if named != 0 {
        return Some(named);
    }
    if let Some(rest) = n.strip_prefix(['F', 'f']) {
        if let Ok(k) = rest.parse::<u32>() {
            if (1..=12).contains(&k) {
                return Some(0xffbd + k);
            }
        }
    }
    let mut c = n.chars();
    match (c.next(), c.next()) {
        (Some(ch), None) if ch.is_ascii_graphic() => Some(ch as u32),
        _ => None,
    }
}

/// Parse `ctrl+Tab` / `Prior` into (modifier keysyms, key keysym).
pub fn parse_key_spec(spec: &str) -> Option<(Vec<u32>, u32)> {
    let parts: Vec<&str> = spec.split('+').collect();
    let (key, mods) = parts.split_last()?;
    let mut m = Vec::new();
    for x in mods {
        let k = keysym(x)?;
        if !matches!(k, 0xffe3 | 0xffe1 | 0xffe9 | 0xffeb) {
            return None;
        }
        m.push(k);
    }
    Some((m, keysym(key)?))
}

pub struct XBridge {
    conn: RustConnection<TimedStream>,
    root: Window,
    min_keycode: u8,
    per: usize,
    syms: Vec<u32>,
    strut_win: Option<Window>,
    button_down: bool,
}

impl XBridge {
    /// Connect to the X server listening on `socket` (e.g. /tmp/.X11-unix/X0).
    pub fn connect(socket: &Path) -> R<Self> {
        let s = UnixStream::connect(socket)?;
        s.set_nonblocking(true)?;
        let conn = RustConnection::connect_to_stream(TimedStream(s), 0)?;
        let setup = conn.setup();
        let root = setup
            .roots
            .first()
            .ok_or_else(|| XError("no screen".into()))?
            .root;
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

    fn atom(&self, name: &str) -> R<u32> {
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
        let ty = if press {
            xproto::BUTTON_PRESS_EVENT
        } else {
            xproto::BUTTON_RELEASE_EVENT
        };
        self.conn.xtest_fake_input(ty, 1, 0, self.root, 0, 0, 0)?;
        self.conn.flush()?;
        self.button_down = press;
        Ok(())
    }
    /// Release a stuck button (e.g. before the bridge is dropped).
    pub fn release_if_down(&mut self) {
        if self.button_down {
            let _ = self.button(false);
        }
    }

    // ---- keyboard
    fn keycode(&self, sym: u32) -> Option<(u8, bool)> {
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

    // ---- focus
    fn activate(&mut self, w: Window) -> R<()> {
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
        match self.find_by_class(needle)?.first().copied() {
            Some(w) => self.activate(w).map(|_| true),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    #[test]
    fn keysym_and_spec_parsing() {
        assert_eq!(keysym("Return"), Some(0xff0d));
        assert_eq!(keysym("Prior"), Some(0xff55));
        assert_eq!(keysym("F5"), Some(0xffc2));
        assert_eq!(keysym("a"), Some(0x61));
        assert_eq!(keysym("rm -rf"), None);
        assert_eq!(parse_key_spec("ctrl+Tab"), Some((vec![0xffe3], 0xff09)));
        assert_eq!(
            parse_key_spec("ctrl+alt+Left"),
            Some((vec![0xffe3, 0xffe9], 0xff51))
        );
        assert_eq!(parse_key_spec("Down"), Some((vec![], 0xff54)));
        assert_eq!(parse_key_spec("a+b"), None, "non-modifier before the key");
        assert_eq!(parse_key_spec(""), None);
    }

    struct Xvfb(Child, std::path::PathBuf);
    impl Drop for Xvfb {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn xvfb(n: u32) -> Option<Xvfb> {
        let child = Command::new("Xvfb")
            .args([
                &format!(":{n}"),
                "-screen",
                "0",
                "1280x1280x24",
                "-nolisten",
                "tcp",
                "-ac",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let sock = std::path::PathBuf::from(format!("/tmp/.X11-unix/X{n}"));
        let t = Instant::now();
        while !sock.exists() && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        sock.exists().then_some(Xvfb(child, sock))
    }

    #[test]
    fn struts_pointer_keys_dock_against_real_xvfb() {
        let Some(x) = xvfb(60 + std::process::id() % 30) else {
            eprintln!("Xvfb not available: skipping");
            return;
        };
        let mut b = XBridge::connect(&x.1).unwrap();

        // struts
        b.set_struts(0).unwrap();
        b.set_struts(90).unwrap();
        let sp = b.atom("_NET_WM_STRUT_PARTIAL").unwrap();
        let w = b.strut_win.unwrap();
        let got = b
            .conn
            .get_property(false, w, sp, AtomEnum::CARDINAL, 0, 12)
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!(
            got.value32().unwrap().collect::<Vec<_>>(),
            crate::logic::rotation::strut_partial(90).to_vec()
        );

        // pointer
        b.pointer_move(123, 456).unwrap();
        let q = b.conn.query_pointer(b.root).unwrap().reply().unwrap();
        assert_eq!((q.root_x, q.root_y), (123, 456));

        // keyboard dock: a fake matchbox-keyboard window gets moved and resized
        let kw = b.conn.generate_id().unwrap();
        b.conn
            .create_window(
                0,
                kw,
                b.root,
                0,
                0,
                100,
                100,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )
            .unwrap();
        b.conn
            .change_property8(
                PropMode::REPLACE,
                kw,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                b"matchbox-keyboard\0Matchbox-keyboard\0",
            )
            .unwrap();
        b.conn.map_window(kw).unwrap();
        assert_eq!(b.dock_keyboard(0).unwrap(), 1);
        let g = b.conn.get_geometry(kw).unwrap().reply().unwrap();
        assert_eq!((g.x, g.y, g.width, g.height), (0, 860, 800, 420));
        b.dock_keyboard(90).unwrap();
        let g = b.conn.get_geometry(kw).unwrap().reply().unwrap();
        assert_eq!((g.x, g.y, g.width, g.height), (0, 460, 1280, 340));

        // key injection reaches a window that listens for key presses (pointer is over it)
        let lw = b.conn.generate_id().unwrap();
        b.conn
            .create_window(
                0,
                lw,
                b.root,
                0,
                0,
                1280,
                1280,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new().event_mask(EventMask::KEY_PRESS),
            )
            .unwrap();
        b.conn.map_window(lw).unwrap();
        b.conn
            .set_input_focus(xproto::InputFocus::POINTER_ROOT, lw, 0u32)
            .unwrap();
        b.conn.flush().unwrap();
        b.send_key("Return").unwrap();
        let want = b.keycode(0xff0d).unwrap().0;
        let t = Instant::now();
        let mut seen = false;
        while t.elapsed() < Duration::from_secs(3) && !seen {
            if let Some(x11rb::protocol::Event::KeyPress(e)) = b.conn.poll_for_event().unwrap() {
                seen = e.detail == want;
            } else {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        assert!(seen, "Return key press was not delivered");
        assert!(b.send_key("bogus key").is_err());
    }

    #[test]
    fn connect_to_missing_server_fails_fast() {
        let t = Instant::now();
        assert!(XBridge::connect(Path::new("/tmp/.X11-unix/X999")).is_err());
        assert!(t.elapsed() < Duration::from_secs(2));
    }
}
