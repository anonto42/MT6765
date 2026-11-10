//! System keyboard: the shell's own on-screen keyboard for apps (terminal, browser), replacing
//! matchbox-keyboard (openbox maximised it and Xvfb spun in a redraw loop while it was open).
//!
//! Same pattern as the system dock (dock.rs): an override-redirect window that the window manager
//! never moves or maximises, full width, flush with the bottom of the visible area, 30 % of its
//! height; plus a managed 1x1 `_NET_WM_WINDOW_TYPE_DOCK` window carrying the bottom strut so
//! maximised apps shrink above it. It never takes the focus (WM_HINTS input=false; clicks on an
//! override-redirect window do not move the focus), and it types into the focused app itself
//! through XTEST on its own X connection. Every wait on that connection has a deadline (walpad-x).
use crate::dock::visible_area;
use std::os::unix::net::UnixStream;
use std::path::Path;
use walpad_ui::components::{
    sys_key_char, system_keyboard_layout, Painter, SysKbdState, SysKey, SysKeyBtn,
};
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts, Rect};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    self, AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt as _, CreateGCAux,
    CreateWindowAux, EventMask, MapState, PropMode, StackMode, Window as XWindow, WindowClass,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

pub const WM_CLASS_INSTANCE: &str = "walpad-keyboard";
pub const WM_CLASS_CLASS: &str = "WalPad-Keyboard";
pub const STRUT_CLASS: &str = "walpad-keyboard-strut";
/// Keyboard height in percent of the visible height.
pub const HEIGHT_PCT: i32 = 30;

// keysyms
pub const XK_SHIFT_L: u32 = 0xffe1;
pub const XK_CONTROL_L: u32 = 0xffe3;
pub const XK_RETURN: u32 = 0xff0d;
pub const XK_TAB: u32 = 0xff09;
pub const XK_ESCAPE: u32 = 0xff1b;
pub const XK_BACKSPACE: u32 = 0xff08;
pub const XK_LEFT: u32 = 0xff51;
pub const XK_UP: u32 = 0xff52;
pub const XK_RIGHT: u32 = 0xff53;
pub const XK_DOWN: u32 = 0xff54;

/// Keyboard height for a visible area (portrait 384, landscape 240).
pub fn height(area: (i32, i32)) -> i32 {
    area.1 * HEIGHT_PCT / 100
}

/// The keyboard window: full width, flush with the bottom of the visible area.
pub fn keyboard_rect(area: (i32, i32)) -> Rect {
    let h = height(area);
    Rect::new(0, area.1 - h, area.0, h)
}

/// `_NET_WM_STRUT_PARTIAL` reserving the screen from `r.y` down to the bottom of the root (the
/// root is 1280x1280, so landscape includes the 480 px below the panel).
pub fn bottom_strut(r: Rect, root_h: i32) -> [u32; 12] {
    let bottom = (root_h - r.y).max(0) as u32;
    [
        0,
        0,
        0,
        bottom,
        0,
        0,
        0,
        0,
        0,
        0,
        r.x as u32,
        (r.right() - 1).max(0) as u32,
    ]
}

pub fn strut_partial(area: (i32, i32), root_h: i32) -> [u32; 12] {
    bottom_strut(keyboard_rect(area), root_h)
}

/// One key event to inject: `keysym` plus the modifiers to hold around it. Characters that need
/// Shift in the keymap (upper case, `!`, ...) get it from the keymap lookup, not from `shift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stroke {
    pub keysym: u32,
    pub ctrl: bool,
    pub shift: bool,
}

/// What a tap on a key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tap {
    /// Only the keyboard's own state changed (modifier latch, layer): redraw.
    State,
    Send(Stroke),
    Hide,
}

/// Apply a tap to the keyboard state. Shift and Ctrl are one-shot: they latch on a tap, apply to
/// the next key and clear (tapping the latched modifier again releases it).
pub fn tap(st: &mut SysKbdState, key: SysKey) -> Tap {
    let named = |k: u32| (k, true);
    let (keysym, named_key) = match key {
        SysKey::Shift => {
            st.shift = !st.shift;
            return Tap::State;
        }
        SysKey::Ctrl => {
            st.ctrl = !st.ctrl;
            return Tap::State;
        }
        SysKey::Layer => {
            st.symbols = !st.symbols;
            st.shift = false;
            return Tap::State;
        }
        SysKey::Hide => {
            *st = SysKbdState::default();
            return Tap::Hide;
        }
        SysKey::Char(c) => (sys_key_char(c, *st) as u32, false),
        SysKey::Space => (0x20, false),
        SysKey::Esc => named(XK_ESCAPE),
        SysKey::Tab => named(XK_TAB),
        SysKey::Backspace => named(XK_BACKSPACE),
        SysKey::Enter => named(XK_RETURN),
        SysKey::Up => named(XK_UP),
        SysKey::Down => named(XK_DOWN),
        SysKey::Left => named(XK_LEFT),
        SysKey::Right => named(XK_RIGHT),
    };
    let s = Stroke {
        keysym,
        ctrl: st.ctrl,
        // Shift+Tab, Shift+arrows; letters already carry it in their keysym
        shift: st.shift && named_key,
    };
    st.shift = false;
    st.ctrl = false;
    Tap::Send(s)
}

/// The server's keyboard mapping (GetKeyboardMapping), for keysym -> keycode.
#[derive(Debug, Clone)]
pub struct KeyMap {
    pub min_keycode: u8,
    pub per: usize,
    pub syms: Vec<u32>,
}

impl KeyMap {
    /// Keycode for a keysym and whether it sits in the shifted column (same rule as the core's
    /// xbridge: only the first two columns, the plain and shifted level of group 1).
    pub fn lookup(&self, sym: u32) -> Option<(u8, bool)> {
        for (i, chunk) in self.syms.chunks(self.per.max(1)).enumerate() {
            for (col, s) in chunk.iter().enumerate().take(2) {
                if *s == sym {
                    return Some((self.min_keycode.checked_add(i as u8)?, col == 1));
                }
            }
        }
        None
    }

    /// Keycodes to press in order (modifiers first, the key last); released in reverse.
    pub fn plan(&self, s: Stroke) -> Option<Vec<u8>> {
        let (kc, shifted) = self.lookup(s.keysym)?;
        let mut codes = Vec::new();
        if s.ctrl {
            codes.push(self.lookup(XK_CONTROL_L)?.0);
        }
        if s.shift || shifted {
            codes.push(self.lookup(XK_SHIFT_L)?.0);
        }
        codes.push(kc);
        Some(codes)
    }
}

struct Atoms {
    active: u32,
    window_type: u32,
    type_dock: u32,
    strut_partial: u32,
    strut: u32,
    net_name: u32,
    utf8: u32,
}

pub struct SysKeyboard {
    xc: XConn,
    win: XWindow,
    strut_win: XWindow,
    shell: XWindow,
    gc: u32,
    depth: u8,
    atoms: Atoms,
    keymap: KeyMap,
    root_h: i32,
    area: (i32, i32),
    mapped: bool,
    active: u32,
    state: SysKbdState,
    keys: Vec<SysKeyBtn>,
    pressed: Option<usize>,
    canvas: Canvas,
}

impl SysKeyboard {
    pub fn open(socket: &Path, shell: XWindow, shell_size: (i32, i32)) -> Result<Self, XError> {
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
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let km = conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
        let keymap = KeyMap {
            min_keycode: min,
            per: km.keysyms_per_keycode as usize,
            syms: km.keysyms,
        };
        let root_h = conn.get_geometry(xc.root)?.reply()?.height as i32;
        let area = visible_area(shell_size.0, shell_size.1);
        let r = keyboard_rect(area);
        let win = conn.generate_id()?;
        let aux = CreateWindowAux::new()
            .event_mask(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::EXPOSURE)
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
            WM_CLASS_INSTANCE.as_bytes(),
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            win,
            atoms.net_name,
            atoms.utf8,
            WM_CLASS_INSTANCE.as_bytes(),
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
            STRUT_CLASS.as_bytes(),
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            strut_win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            format!("{STRUT_CLASS}\0{STRUT_CLASS}\0").as_bytes(),
        )?;
        let mut k = SysKeyboard {
            xc,
            win,
            strut_win,
            shell,
            gc,
            depth,
            atoms,
            keymap,
            root_h,
            area,
            mapped: false,
            active: 0,
            state: SysKbdState::default(),
            keys: Vec::new(),
            pressed: None,
            canvas: Canvas::new(r.w as u32, r.h as u32, BG).ok_or("keyboard canvas")?,
        };
        k.write_strut()?;
        let root = k.xc.root;
        k.xc.conn.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        k.active = k.read_active();
        k.xc.conn.flush()?;
        Ok(k)
    }

    pub fn wait_fd(&self) -> &UnixStream {
        &self.xc.wait_fd
    }
    pub fn window(&self) -> XWindow {
        self.win
    }
    pub fn visible(&self) -> bool {
        self.mapped
    }
    /// Height of the keyboard for the current orientation (also while hidden).
    pub fn height(&self) -> i32 {
        height(self.area)
    }

    fn write_strut(&self) -> Result<(), XError> {
        let s = strut_partial(self.area, self.root_h);
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

    fn read_active(&self) -> u32 {
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

    /// The shell window changed size (rotation): move and resize the keyboard and its strut.
    pub fn set_shell_size(&mut self, w: i32, h: i32, fonts: &mut Fonts) -> Result<(), XError> {
        let area = visible_area(w, h);
        if area == self.area {
            return Ok(());
        }
        self.area = area;
        let r = keyboard_rect(area);
        self.canvas = Canvas::new(r.w as u32, r.h as u32, BG).ok_or("keyboard canvas")?;
        self.pressed = None;
        self.xc.conn.configure_window(
            self.win,
            &ConfigureWindowAux::new()
                .x(r.x)
                .y(r.y)
                .width(r.w as u32)
                .height(r.h as u32),
        )?;
        self.write_strut()?;
        if self.mapped {
            self.redraw(fonts)?;
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    /// Show or hide. Showing maps the strut window too (apps shrink above the keyboard) and puts
    /// the keyboard on top; hiding resets the latched modifiers.
    pub fn set_visible(&mut self, on: bool, fonts: &mut Fonts) -> Result<(), XError> {
        if on == self.mapped {
            return Ok(());
        }
        self.mapped = on;
        self.pressed = None;
        if on {
            self.xc.conn.map_window(self.strut_win)?;
            self.xc.conn.map_window(self.win)?;
            let r = keyboard_rect(self.area);
            self.xc.conn.configure_window(
                self.win,
                &ConfigureWindowAux::new()
                    .x(r.x)
                    .y(r.y)
                    .width(r.w as u32)
                    .height(r.h as u32)
                    .stack_mode(StackMode::ABOVE),
            )?;
            // paint right away: a remapped window is not always sent an Expose
            self.redraw(fonts)?;
        } else {
            self.state = SysKbdState::default();
            self.xc.conn.unmap_window(self.win)?;
            self.xc.conn.unmap_window(self.strut_win)?;
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    fn redraw(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        self.keys = Painter {
            cv: &mut self.canvas,
            fonts,
        }
        .system_keyboard(self.state, self.pressed);
        crate::present::present(
            &self.xc.conn,
            self.win,
            self.gc,
            self.depth,
            &self.canvas,
            self.canvas.bounds(),
        )
    }

    fn key_at(&self, x: i32, y: i32) -> Option<usize> {
        let r = keyboard_rect(self.area);
        system_keyboard_layout(Rect::new(0, 0, r.w, r.h), self.state.symbols)
            .iter()
            .position(|k| k.rect.contains(x, y))
    }

    /// Inject one stroke into the focused window through XTEST.
    pub fn send(&self, s: Stroke) -> Result<(), XError> {
        let codes = self
            .keymap
            .plan(s)
            .ok_or_else(|| XError(format!("no keycode for keysym {:#x}", s.keysym)))?;
        let c = &self.xc.conn;
        let root = self.xc.root;
        for kc in &codes {
            c.xtest_fake_input(xproto::KEY_PRESS_EVENT, *kc, 0, root, 0, 0, 0)?;
        }
        for kc in codes.iter().rev() {
            c.xtest_fake_input(xproto::KEY_RELEASE_EVENT, *kc, 0, root, 0, 0, 0)?;
        }
        c.flush()?;
        Ok(())
    }

    /// Handle everything the server has sent (non-blocking). Returns true when the keyboard hid
    /// itself (the minimise key, or the shell came to the front = the user went Home).
    pub fn poll(&mut self, fonts: &mut Fonts) -> Result<bool, XError> {
        let mut hid = false;
        while let Some(ev) = self.xc.conn.poll_for_event()? {
            match ev {
                Event::PropertyNotify(e) if e.atom == self.atoms.active => {
                    let was = self.active;
                    self.active = self.read_active();
                    if self.mapped && was != self.active {
                        if self.active == self.shell && was != 0 {
                            self.set_visible(false, fonts)?;
                            hid = true;
                        } else {
                            // the window manager restacks its windows: keep the keyboard on top
                            self.xc.conn.configure_window(
                                self.win,
                                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                            )?;
                            self.xc.conn.flush()?;
                        }
                    }
                }
                Event::Expose(e) if e.window == self.win && e.count == 0 && self.mapped => {
                    self.redraw(fonts)?;
                }
                Event::ButtonPress(e) if e.event == self.win && e.detail == 1 => {
                    self.pressed = self.key_at(e.event_x as i32, e.event_y as i32);
                    self.redraw(fonts)?;
                }
                Event::ButtonRelease(e) if e.event == self.win && e.detail == 1 => {
                    let hit = self.key_at(e.event_x as i32, e.event_y as i32);
                    let down = self.pressed.take();
                    let key = match (hit, down) {
                        (Some(h), Some(d)) if h == d => self.keys.get(h).map(|k| k.key),
                        _ => None,
                    };
                    match key.map(|k| tap(&mut self.state, k)) {
                        Some(Tap::Hide) => {
                            self.set_visible(false, fonts)?;
                            hid = true;
                        }
                        Some(Tap::Send(s)) => {
                            if let Err(e) = self.send(s) {
                                eprintln!("walpad-shell: keyboard: {}", e.0);
                            }
                            self.redraw(fonts)?;
                        }
                        Some(Tap::State) | None => self.redraw(fonts)?,
                    }
                }
                Event::Error(e) => return Err(XError(format!("keyboard X error: {e:?}"))),
                _ => {}
            }
        }
        Ok(hid)
    }

    /// For tests and diagnostics: is the window currently mapped according to the server?
    pub fn server_mapped(&self) -> bool {
        self.xc
            .conn
            .get_window_attributes(self.win)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|a| a.map_state != MapState::UNMAPPED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thirty_percent_of_the_visible_height_flush_with_the_bottom() {
        assert_eq!(height((800, 1280)), 384);
        assert_eq!(height((1280, 800)), 240);
        assert_eq!(keyboard_rect((800, 1280)), Rect::new(0, 896, 800, 384));
        assert_eq!(keyboard_rect((1280, 800)), Rect::new(0, 560, 1280, 240));
    }

    #[test]
    fn strut_counts_from_the_screen_edge() {
        let p = strut_partial((800, 1280), 1280);
        assert_eq!(p, [0, 0, 0, 384, 0, 0, 0, 0, 0, 0, 0, 799]);
        let l = strut_partial((1280, 800), 1280);
        assert_eq!(l, [0, 0, 0, 720, 0, 0, 0, 0, 0, 0, 0, 1279], "480 + 240");
    }

    #[test]
    fn layout_fills_both_orientations_with_big_enough_keys() {
        for area in [(800, 1280), (1280, 800)] {
            let r = keyboard_rect(area);
            for symbols in [false, true] {
                let keys = system_keyboard_layout(Rect::new(0, 0, r.w, r.h), symbols);
                assert!(keys.iter().all(|k| k.rect.x >= 0 && k.rect.right() <= r.w));
                assert!(keys.iter().all(|k| k.rect.y >= 0 && k.rect.bottom() <= r.h));
                assert!(
                    keys.iter().all(|k| k.rect.h >= 40 && k.rect.w >= 40),
                    "{area:?}: keys >= 40 px"
                );
                // no overlaps
                for (i, a) in keys.iter().enumerate() {
                    for b in &keys[i + 1..] {
                        let overlap = a.rect.x < b.rect.right()
                            && b.rect.x < a.rect.right()
                            && a.rect.y < b.rect.bottom()
                            && b.rect.y < a.rect.bottom();
                        assert!(!overlap, "{:?} overlaps {:?}", a.key, b.key);
                    }
                }
                for k in [
                    SysKey::Esc,
                    SysKey::Tab,
                    SysKey::Ctrl,
                    SysKey::Layer,
                    SysKey::Space,
                    SysKey::Backspace,
                    SysKey::Enter,
                    SysKey::Up,
                    SysKey::Down,
                    SysKey::Left,
                    SysKey::Right,
                    SysKey::Hide,
                ] {
                    assert_eq!(keys.iter().filter(|b| b.key == k).count(), 1, "{k:?} once");
                }
                assert_eq!(
                    keys.iter().any(|b| b.key == SysKey::Shift),
                    !symbols,
                    "Shift on the letter layer"
                );
                // inverted T: Up sits right above Down
                let at = |k| keys.iter().find(|b| b.key == k).unwrap().rect;
                let (up, down) = (at(SysKey::Up), at(SysKey::Down));
                assert_eq!((up.x, up.w), (down.x, down.w));
                assert!(up.bottom() < down.y);
                // the right edges line up, with the side padding
                let right = keys.iter().map(|k| k.rect.right()).max().unwrap();
                assert!(r.w - right <= 8);
            }
            let lower = system_keyboard_layout(Rect::new(0, 0, r.w, r.h), false);
            let letters = lower
                .iter()
                .filter(|k| matches!(k.key, SysKey::Char(c) if c.is_ascii_lowercase()))
                .count();
            assert_eq!(letters, 26);
            let digits = lower
                .iter()
                .filter(|k| matches!(k.key, SysKey::Char(c) if c.is_ascii_digit()))
                .count();
            assert_eq!(digits, 10);
        }
        let land = keyboard_rect((1280, 800));
        let k = system_keyboard_layout(Rect::new(0, 0, land.w, land.h), false);
        assert!(k.iter().all(|b| b.rect.h >= 40), "landscape keys >= 40 px");
    }

    fn stroke(keysym: u32, ctrl: bool, shift: bool) -> Tap {
        Tap::Send(Stroke {
            keysym,
            ctrl,
            shift,
        })
    }

    #[test]
    fn taps_map_to_keysyms_with_one_shot_modifiers() {
        let mut st = SysKbdState::default();
        assert_eq!(
            tap(&mut st, SysKey::Char('a')),
            stroke('a' as u32, false, false)
        );
        // shift: one upper-case letter, then back to lower case
        assert_eq!(tap(&mut st, SysKey::Shift), Tap::State);
        assert!(st.shift);
        assert_eq!(
            tap(&mut st, SysKey::Char('a')),
            stroke('A' as u32, false, false)
        );
        assert!(!st.shift);
        assert_eq!(
            tap(&mut st, SysKey::Char('a')),
            stroke('a' as u32, false, false)
        );
        // ctrl+c
        tap(&mut st, SysKey::Ctrl);
        assert!(st.ctrl);
        assert_eq!(
            tap(&mut st, SysKey::Char('c')),
            stroke('c' as u32, true, false)
        );
        assert!(!st.ctrl);
        // tapping a latched modifier again releases it
        tap(&mut st, SysKey::Ctrl);
        tap(&mut st, SysKey::Ctrl);
        assert!(!st.ctrl);
        // named keys
        for (k, sym) in [
            (SysKey::Esc, XK_ESCAPE),
            (SysKey::Tab, XK_TAB),
            (SysKey::Backspace, XK_BACKSPACE),
            (SysKey::Enter, XK_RETURN),
            (SysKey::Up, XK_UP),
            (SysKey::Down, XK_DOWN),
            (SysKey::Left, XK_LEFT),
            (SysKey::Right, XK_RIGHT),
            (SysKey::Space, 0x20),
        ] {
            assert_eq!(tap(&mut st, k), stroke(sym, false, false), "{k:?}");
        }
        // shift+Tab carries an explicit shift; digits are unaffected by shift
        tap(&mut st, SysKey::Shift);
        assert_eq!(tap(&mut st, SysKey::Tab), stroke(XK_TAB, false, true));
        tap(&mut st, SysKey::Shift);
        assert_eq!(
            tap(&mut st, SysKey::Char('1')),
            stroke('1' as u32, false, false)
        );
        // ctrl+shift+left
        tap(&mut st, SysKey::Ctrl);
        tap(&mut st, SysKey::Shift);
        assert_eq!(tap(&mut st, SysKey::Left), stroke(XK_LEFT, true, true));
        // layer toggle clears shift, hide resets everything
        tap(&mut st, SysKey::Shift);
        assert_eq!(tap(&mut st, SysKey::Layer), Tap::State);
        assert!(st.symbols && !st.shift);
        tap(&mut st, SysKey::Ctrl);
        assert_eq!(tap(&mut st, SysKey::Hide), Tap::Hide);
        assert_eq!(st, SysKbdState::default());
    }

    /// A tiny US-like keymap: keycode 10 = a/A, 11 = 1/!, 12 = Tab, 13 = Shift_L, 14 = Control_L,
    /// 15 = Left.
    fn keymap() -> KeyMap {
        KeyMap {
            min_keycode: 10,
            per: 2,
            syms: vec![
                'a' as u32,
                'A' as u32,
                '1' as u32,
                '!' as u32,
                XK_TAB,
                0,
                XK_SHIFT_L,
                0,
                XK_CONTROL_L,
                0,
                XK_LEFT,
                0,
            ],
        }
    }

    #[test]
    fn keymap_plans_modifiers_before_the_key() {
        let m = keymap();
        let s = |keysym, ctrl, shift| Stroke {
            keysym,
            ctrl,
            shift,
        };
        assert_eq!(m.lookup('a' as u32), Some((10, false)));
        assert_eq!(m.lookup('A' as u32), Some((10, true)));
        assert_eq!(m.plan(s('a' as u32, false, false)), Some(vec![10]));
        assert_eq!(m.plan(s('A' as u32, false, false)), Some(vec![13, 10]));
        assert_eq!(m.plan(s('!' as u32, false, false)), Some(vec![13, 11]));
        assert_eq!(m.plan(s('a' as u32, true, false)), Some(vec![14, 10]));
        assert_eq!(m.plan(s(XK_TAB, false, true)), Some(vec![13, 12]));
        assert_eq!(m.plan(s(XK_LEFT, true, true)), Some(vec![14, 13, 15]));
        assert_eq!(m.plan(s('z' as u32, false, false)), None, "not in the map");
    }
}
