//! Keysym mapping, modifier planning, tap state transitions, and XTEST key injection.

use super::{
    XK_ALT_L, XK_BACKSPACE, XK_CONTROL_L, XK_DOWN, XK_ESCAPE, XK_LEFT, XK_RETURN, XK_RIGHT,
    XK_SHIFT_L, XK_TAB, XK_UP,
};
use walpad_ui::components::{sys_key_char, SysKbdState, SysKey};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, ConnectionExt as _};
use x11rb::protocol::xtest::ConnectionExt as _;

/// One key event to inject: `keysym` plus the modifiers to hold around it. Characters that need
/// Shift in the keymap (upper case, `!`, ...) get it from the keymap lookup, not from `shift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stroke {
    pub keysym: u32,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Stroke {
    /// Ctrl + a character key (the edit shortcuts).
    pub const fn ctrl(keysym: u32) -> Stroke {
        Stroke {
            keysym,
            ctrl: true,
            shift: false,
            alt: false,
        }
    }
}

/// What a tap on a key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tap {
    /// Only the keyboard's own state changed (modifier latch, layer): redraw.
    State,
    Send(Stroke),
    /// The paste shortcut: the keyboard picks Ctrl+V or (terminal in front) Ctrl+Shift+V.
    Paste,
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
        SysKey::Alt => {
            st.alt = !st.alt;
            return Tap::State;
        }
        SysKey::Paste => {
            st.ctrl = false;
            st.alt = false;
            st.shift = false;
            return Tap::Paste;
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
        alt: st.alt,
    };
    st.shift = false;
    st.ctrl = false;
    st.alt = false;
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
        if s.alt {
            codes.push(self.lookup(XK_ALT_L)?.0);
        }
        if s.shift || shifted {
            codes.push(self.lookup(XK_SHIFT_L)?.0);
        }
        codes.push(kc);
        Some(codes)
    }
}

/// The server's current keyboard mapping.
pub fn load_keymap(xc: &XConn) -> Result<KeyMap, XError> {
    let setup = xc.conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let km = xc.conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
    Ok(KeyMap {
        min_keycode: min,
        per: km.keysyms_per_keycode as usize,
        syms: km.keysyms,
    })
}

/// Inject one stroke into the focused window through XTEST (modifiers pressed first, released
/// in reverse).
pub fn inject(xc: &XConn, keymap: &KeyMap, s: Stroke) -> Result<(), XError> {
    let codes = keymap
        .plan(s)
        .ok_or_else(|| XError(format!("no keycode for keysym {:#x}", s.keysym)))?;
    let c = &xc.conn;
    for kc in &codes {
        c.xtest_fake_input(xproto::KEY_PRESS_EVENT, *kc, 0, xc.root, 0, 0, 0)?;
    }
    for kc in codes.iter().rev() {
        c.xtest_fake_input(xproto::KEY_RELEASE_EVENT, *kc, 0, xc.root, 0, 0, 0)?;
    }
    c.flush()?;
    Ok(())
}
