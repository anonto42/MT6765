/// The input method name: `XMODIFIERS=@im=walpad`.
pub const IM_NAME: &str = "walpad";
/// Show the keyboard this long after a text input gained the focus, if it still has it: a window
/// being activated briefly restores the focus to its last input before the tap moves it on
/// (Firefox: the URL bar, then the page), which would flash the keyboard.
pub const SHOW_DELAY_MS: u64 = 150;
/// Hide the keyboard this long after the last text input lost the focus.
pub const HIDE_DEBOUNCE_MS: u64 = 250;
/// Fallback for apps that never talk XIM: the terminal gets the keyboard this long after it
/// started unless an XIM client connected or focused an input meanwhile.
pub const FALLBACK_MS: u64 = 3000;

/// One input context: (client communication window, input method id, input context id).
pub type IcKey = (u32, u16, u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KbdAction {
    Show,
    Hide,
}

/// Focus -> keyboard state machine. Times are milliseconds on any monotonic clock; actions come
/// out of `tick` (and of `app_started` when there is no XIM server).
///
/// - focus-in: show `SHOW_DELAY_MS` later if an input still has the focus (also after the
///   minimise key: it hides only until the next focus-in);
/// - focus-out / IC destroyed / client gone: hide `HIDE_DEBOUNCE_MS` after the last focused
///   input went away, unless another focus-in came first;
/// - manual show/hide (dock button, core command, minimise key, Home, mouse pad): followed as is;
///   a keyboard opened by hand while no input is focused stays until it is closed by hand or a
///   focus-in / focus-out cycle takes it over;
/// - app started (terminal): fallback show after `FALLBACK_MS` unless XIM spoke meanwhile, at
///   once when the XIM server is not running.
#[derive(Debug, Default, Clone)]
pub struct FocusKbd {
    focused: Vec<IcKey>,
    visible: bool,
    /// the keyboard is up because an input has (or had) the focus: a focus-out hides it
    by_focus: bool,
    show_at: Option<u64>,
    hide_at: Option<u64>,
    fallback_at: Option<u64>,
}

impl FocusKbd {
    pub fn new() -> FocusKbd {
        FocusKbd::default()
    }

    /// What the machine believes (kept in step with the real keyboard by `manual`).
    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn has_focus(&self) -> bool {
        !self.focused.is_empty()
    }

    pub fn focus_in(&mut self, ic: IcKey, now: u64) {
        if !self.focused.contains(&ic) {
            self.focused.push(ic);
        }
        self.hide_at = None;
        self.fallback_at = None;
        self.by_focus = true;
        if !self.visible && self.show_at.is_none() {
            self.show_at = Some(now + SHOW_DELAY_MS);
        }
    }

    /// The input lost the focus, or its context was destroyed.
    pub fn focus_out(&mut self, ic: IcKey, now: u64) {
        self.focused.retain(|k| *k != ic);
        self.arm_hide(now);
    }

    /// An XIM client went away (disconnect, or its window was destroyed: crash, kill).
    pub fn client_gone(&mut self, client_win: u32, now: u64) {
        let before = self.focused.len();
        self.focused.retain(|k| k.0 != client_win);
        if self.focused.len() != before {
            self.arm_hide(now);
        }
    }

    /// An XIM client connected: it will report its own focus, so no fallback is needed.
    pub fn client_connected(&mut self) {
        self.fallback_at = None;
    }

    fn arm_hide(&mut self, now: u64) {
        if !self.focused.is_empty() {
            return;
        }
        self.show_at = None;
        if self.visible && self.by_focus && self.hide_at.is_none() {
            self.hide_at = Some(now + HIDE_DEBOUNCE_MS);
        }
    }

    /// The keyboard was shown or hidden by hand (or by the shell: Home, the mouse pad).
    pub fn manual(&mut self, visible: bool) {
        self.visible = visible;
        self.show_at = None;
        self.hide_at = None;
        self.fallback_at = None;
        // opened by hand while an input has the focus: leaving that input closes it again
        self.by_focus = visible && self.has_focus();
    }

    /// An app that needs keys started (the terminal). `xim_running`: the XIM server is up.
    pub fn app_started(&mut self, now: u64, xim_running: bool) -> Option<KbdAction> {
        if !xim_running {
            return self.fallback_show();
        }
        if !self.visible {
            self.fallback_at = Some(now + FALLBACK_MS);
        }
        None
    }

    fn fallback_show(&mut self) -> Option<KbdAction> {
        self.fallback_at = None;
        if self.visible {
            return None;
        }
        self.visible = true;
        self.by_focus = false;
        Some(KbdAction::Show)
    }

    /// `_NET_ACTIVE_WINDOW` changed (owner report 2026-11-13: the terminal/Kali/Scanner fallback
    /// keyboard, once up, never hid again when switching away - it only ever showed itself on
    /// `app_started`, with nothing watching for the active window leaving). `fallback_app`:
    /// whether the window that just became active is one of the no-XIM apps.
    ///
    /// A keyboard a real XIM focus brought up (`by_focus`) is left alone: its own focus-out
    /// already governs it, and this is purely the fallback apps' own safety net.
    pub fn active_window_changed(&mut self, now: u64, fallback_app: bool) {
        if fallback_app {
            // back in a fallback app: a hide armed by having just left one does not apply any
            // more, visible or not (the not-yet-visible case still needs its own show timer).
            self.hide_at = None;
            if !self.visible && self.fallback_at.is_none() {
                self.fallback_at = Some(now + FALLBACK_MS);
            }
        } else if self.visible && !self.by_focus {
            self.fallback_at = None;
            if self.hide_at.is_none() {
                self.hide_at = Some(now + HIDE_DEBOUNCE_MS);
            }
        }
    }

    pub fn tick(&mut self, now: u64) -> Option<KbdAction> {
        if self.show_at.is_some_and(|t| now >= t) {
            self.show_at = None;
            if self.has_focus() && !self.visible {
                self.visible = true;
                return Some(KbdAction::Show);
            }
        }
        if self.hide_at.is_some_and(|t| now >= t) {
            self.hide_at = None;
            if self.focused.is_empty() && self.visible {
                self.visible = false;
                self.by_focus = false;
                return Some(KbdAction::Hide);
            }
        }
        if self.fallback_at.is_some_and(|t| now >= t) {
            return self.fallback_show();
        }
        None
    }

    pub fn next_deadline(&self) -> Option<u64> {
        [self.show_at, self.hide_at, self.fallback_at]
            .into_iter()
            .flatten()
            .min()
    }
}

/// What the XIM side saw (fed into [`FocusKbd`] by the shell).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImEvent {
    Connected,
    FocusIn(IcKey),
    FocusOut(IcKey),
    Destroyed(IcKey),
    ClientGone(u32),
    /// `_NET_ACTIVE_WINDOW` changed; `true` if the new one is one of `FALLBACK_CLASSES`.
    ActiveWindow(bool),
}
