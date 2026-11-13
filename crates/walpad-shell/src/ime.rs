//! Dynamic system keyboard (owner request 2026-11-10): a minimal XIM server registered as
//! `@server=walpad` (apps run with `XMODIFIERS=@im=walpad`, `GTK_IM_MODULE=xim`,
//! `QT_IM_MODULE=xim`). Apps tell an input method when a text input gains and loses the focus
//! (XIM SetICFocus / UnsetICFocus): that is the only thing it is used for. The keyboard shows
//! 150 ms after a focus-in that stays and hides 250 ms after the last focus-out (so neither a
//! window activation nor moving between two fields flickers).
//!
//! The server never touches key input: every input context is told to forward no events
//! (`XIM_SET_EVENT_MASK` 0/0) and only the "nothing" preedit/status styles are offered, so apps
//! keep handling their keys themselves (the system keyboard still types through XTEST).
//!
//! Two layers: [`FocusKbd`], the pure state machine (unit tested), and [`ImServer`], the X side
//! (its own X connection, every wait with a deadline like the other shell windows).
use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::path::Path;
use walpad_x::{TimedStream, XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConnectionExt as _, EventMask,
    PropMode, Window as XWindow, WindowClass, CLIENT_MESSAGE_EVENT,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use xim::x11rb::{HasConnection, X11rbServer};
use xim::{InputStyle, Server, ServerError, ServerHandler, UserInputContext, XimConnections};

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

// ------------------------------------------------------------------------------------ XIM server

/// What the XIM side saw (fed into [`FocusKbd`] by the shell).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImEvent {
    Connected,
    FocusIn(IcKey),
    FocusOut(IcKey),
    Destroyed(IcKey),
    ClientGone(u32),
    /// `_NET_ACTIVE_WINDOW` changed; `true` if the new one is one of [`FALLBACK_CLASSES`].
    ActiveWindow(bool),
}

/// The shell's timed X connection as the xim crate's transport.
pub struct ImConn(XConn);

impl HasConnection for ImConn {
    type Connection = RustConnection<TimedStream>;
    fn conn(&self) -> &Self::Connection {
        &self.0.conn
    }
}

type XServer = X11rbServer<ImConn>;

/// Locales the server accepts (Xlib only opens an input method that lists the client's locale,
/// language or full locale name). The server does no text conversion, so any locale works.
pub const LOCALES: &str =
    "C,POSIX,C.UTF-8,C.utf8,en,en_US,en_US.UTF-8,en_US.utf8,en_GB,en_GB.UTF-8,\
en_IN,en_IN.UTF-8,bn,bn_BD,bn_BD.UTF-8,bn_IN,bn_IN.UTF-8,ar,de,de_DE,de_DE.UTF-8,es,es_ES,\
es_ES.UTF-8,fr,fr_FR,fr_FR.UTF-8,hi,hi_IN,hi_IN.UTF-8,it,it_IT,it_IT.UTF-8,ja,ja_JP,ja_JP.UTF-8,\
ko,ko_KR,ko_KR.UTF-8,nl,nl_NL,nl_NL.UTF-8,pl,pl_PL,pl_PL.UTF-8,pt,pt_BR,pt_BR.UTF-8,pt_PT,\
pt_PT.UTF-8,ru,ru_RU,ru_RU.UTF-8,sv,sv_SE,sv_SE.UTF-8,tr,tr_TR,tr_TR.UTF-8,zh,zh_CN,zh_CN.UTF-8,\
zh_TW,zh_TW.UTF-8";

/// The styles offered: no preedit and no status window, so the client draws nothing for us and
/// keeps its key handling.
pub fn input_styles() -> [InputStyle; 4] {
    [
        InputStyle::PREEDIT_NOTHING | InputStyle::STATUS_NOTHING,
        InputStyle::PREEDIT_NOTHING | InputStyle::STATUS_NONE,
        InputStyle::PREEDIT_NONE | InputStyle::STATUS_NOTHING,
        InputStyle::PREEDIT_NONE | InputStyle::STATUS_NONE,
    ]
}

/// Records focus changes; consumes nothing.
#[derive(Default)]
struct Handler {
    events: Vec<ImEvent>,
}

fn key(ic: &UserInputContext<()>) -> IcKey {
    (
        ic.ic.client_win(),
        ic.ic.input_method_id().get(),
        ic.ic.input_context_id().get(),
    )
}

impl ServerHandler<XServer> for Handler {
    type InputStyleArray = [InputStyle; 4];
    type InputContextData = ();

    fn new_ic_data(&mut self, _: &mut XServer, _: InputStyle) -> Result<(), ServerError> {
        Ok(())
    }
    fn input_styles(&self) -> Self::InputStyleArray {
        input_styles()
    }
    fn filter_events(&self) -> u32 {
        0
    }
    fn handle_connect(&mut self, _: &mut XServer) -> Result<(), ServerError> {
        self.events.push(ImEvent::Connected);
        Ok(())
    }
    fn handle_create_ic(
        &mut self,
        server: &mut XServer,
        ic: &mut UserInputContext<()>,
    ) -> Result<(), ServerError> {
        // forward nothing (and nothing synchronously): keys never pass through the server
        server.set_event_mask(&ic.ic, 0, 0)
    }
    fn handle_destroy_ic(
        &mut self,
        _: &mut XServer,
        ic: UserInputContext<()>,
    ) -> Result<(), ServerError> {
        self.events.push(ImEvent::Destroyed(key(&ic)));
        Ok(())
    }
    fn handle_reset_ic(
        &mut self,
        _: &mut XServer,
        _: &mut UserInputContext<()>,
    ) -> Result<String, ServerError> {
        Ok(String::new())
    }
    fn handle_set_focus(
        &mut self,
        _: &mut XServer,
        ic: &mut UserInputContext<()>,
    ) -> Result<(), ServerError> {
        self.events.push(ImEvent::FocusIn(key(ic)));
        Ok(())
    }
    fn handle_unset_focus(
        &mut self,
        _: &mut XServer,
        ic: &mut UserInputContext<()>,
    ) -> Result<(), ServerError> {
        self.events.push(ImEvent::FocusOut(key(ic)));
        Ok(())
    }
    fn handle_set_ic_values(
        &mut self,
        _: &mut XServer,
        _: &mut UserInputContext<()>,
    ) -> Result<(), ServerError> {
        Ok(())
    }
    fn handle_forward_event(
        &mut self,
        _: &mut XServer,
        _: &mut UserInputContext<()>,
        _: &<XServer as Server>::XEvent,
    ) -> Result<bool, ServerError> {
        // never consume: the event goes back to the client unchanged
        Ok(false)
    }
}

struct Atoms {
    xconnect: u32,
    protocol: u32,
    moredata: u32,
    active: u32,
}

/// WM_CLASS of apps that need the keyboard but never talk XIM (st: terminal, kali, scanner - all
/// the same st+fish shell, owner request 2026-11-12/13). Matched against the active window's
/// WM_CLASS to drive the fallback show/hide in [`ImServer::poll`] (`active_window_changed`).
const FALLBACK_CLASSES: &[&str] = &["term-st", "kali-st", "scanner-st"];

pub struct ImServer {
    server: XServer,
    conns: XimConnections<()>,
    handler: Handler,
    atoms: Atoms,
    wait_fd: UnixStream,
    root: XWindow,
    /// client communication window -> our window for that client
    clients: HashMap<u32, u32>,
    /// `_XIM_MOREDATA` chunks per our communication window (Xlib's "only client messages"
    /// transport splits long requests into 20-byte pieces)
    partial: HashMap<u32, Vec<u8>>,
    /// Last `_NET_ACTIVE_WINDOW` seen, to tell a real change from a repeated PropertyNotify.
    active_window: u32,
}

fn ser(e: ServerError) -> XError {
    XError(format!("xim: {e}"))
}

impl ImServer {
    /// Connect, create the server window, own `@server=walpad` and list it in `XIM_SERVERS`.
    pub fn open(socket: &Path) -> Result<ImServer, XError> {
        let xc = walpad_x::connect(socket)?;
        let wait_fd = xc.wait_fd.try_clone()?;
        let root = xc.root;
        let intern = |n: &str| -> Result<u32, XError> {
            Ok(xc.conn.intern_atom(false, n.as_bytes())?.reply()?.atom)
        };
        let atoms = Atoms {
            xconnect: intern("_XIM_XCONNECT")?,
            protocol: intern("_XIM_PROTOCOL")?,
            moredata: intern("_XIM_MOREDATA")?,
            active: intern("_NET_ACTIVE_WINDOW")?,
        };
        // watch root for _NET_ACTIVE_WINDOW changes (the fallback show/hide for apps that never
        // talk XIM at all - st: terminal, kali, scanner - follows which window is active)
        xc.conn.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        let server = X11rbServer::init(ImConn(xc), 0, IM_NAME, LOCALES).map_err(ser)?;
        Ok(ImServer {
            server,
            conns: XimConnections::new(),
            handler: Handler::default(),
            atoms,
            wait_fd,
            root,
            clients: HashMap::new(),
            partial: HashMap::new(),
            active_window: 0,
        })
    }

    /// WM_CLASS of a window (both components), empty if it has none / is gone.
    fn class_of(&self, w: u32) -> Vec<String> {
        self.conn()
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

    /// The current `_NET_ACTIVE_WINDOW` (0 if none / unreadable).
    fn read_active(&self) -> u32 {
        self.conn()
            .get_property(false, self.root, self.atoms.active, AtomEnum::WINDOW, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().and_then(|mut v| v.next()))
            .unwrap_or(0)
    }

    pub fn wait_fd(&self) -> &UnixStream {
        &self.wait_fd
    }

    fn conn(&self) -> &RustConnection<TimedStream> {
        self.server.conn()
    }

    /// `_XIM_XCONNECT` from a client: a communication window of ours for it, the reply, and a
    /// watch on its window so a client that dies without disconnecting is noticed.
    fn accept(&mut self, client_win: u32) -> Result<(), XError> {
        let c = self.conn();
        let com = c.generate_id()?;
        c.create_window(
            0,
            com,
            self.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &Default::default(),
        )?;
        c.change_window_attributes(
            client_win,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::STRUCTURE_NOTIFY),
        )?;
        // transport version 0.0 (client messages + properties), as the xim crate does
        c.send_event(
            false,
            client_win,
            EventMask::NO_EVENT,
            ClientMessageEvent {
                response_type: CLIENT_MESSAGE_EVENT,
                format: 32,
                sequence: 0,
                window: client_win,
                type_: self.atoms.xconnect,
                data: [com, 0, 0, 0, 0].into(),
            },
        )?;
        c.flush()?;
        self.conns.new_connection(com, client_win);
        self.clients.insert(client_win, com);
        Ok(())
    }

    /// The client's window is gone: drop its connection and its input contexts.
    fn drop_client(&mut self, client_win: u32) {
        let Some(com) = self.clients.remove(&client_win) else {
            return;
        };
        if let Some(conn) = self.conns.get_connection(com) {
            if let Err(e) = conn.disconnect(&mut self.server, &mut self.handler) {
                eprintln!("walpad-shell: xim: {e}");
            }
        }
        self.conns.remove_connection(com);
        self.partial.remove(&com);
        let _ = self.conn().destroy_window(com);
        self.handler.events.push(ImEvent::ClientGone(client_win));
    }

    /// Handle everything the server has sent (non-blocking). Protocol errors of one client are
    /// logged and never stop the server; only a dead X connection is an error.
    pub fn poll(&mut self) -> Result<Vec<ImEvent>, XError> {
        while let Some(ev) = self.conn().poll_for_event()? {
            match ev {
                Event::ClientMessage(m) if m.type_ == self.atoms.xconnect => {
                    let client_win = m.data.as_data32()[0];
                    if let Err(e) = self.accept(client_win) {
                        eprintln!("walpad-shell: xim: connect: {}", e.0);
                    }
                }
                Event::ClientMessage(m) if m.type_ == self.atoms.moredata && m.format == 8 => {
                    self.partial
                        .entry(m.window)
                        .or_default()
                        .extend_from_slice(&m.data.as_data8());
                }
                Event::ClientMessage(m)
                    if m.type_ == self.atoms.protocol
                        && m.format == 8
                        && self.partial.contains_key(&m.window) =>
                {
                    // the last piece of a split request: hand the whole request to the parser
                    // the way a long request normally arrives (in a property of our window)
                    let mut data = self.partial.remove(&m.window).unwrap_or_default();
                    data.extend_from_slice(&m.data.as_data8());
                    let r = self.reassembled(m.window, &data);
                    if let Err(e) = r {
                        eprintln!("walpad-shell: xim: {}", e.0);
                    }
                }
                Event::DestroyNotify(d) if self.clients.contains_key(&d.window) => {
                    self.drop_client(d.window);
                }
                Event::PropertyNotify(e) if e.atom == self.atoms.active => {
                    let w = self.read_active();
                    if w != self.active_window {
                        self.active_window = w;
                        let class = self.class_of(w);
                        let fallback = FALLBACK_CLASSES
                            .iter()
                            .any(|f| class.iter().any(|c| c.eq_ignore_ascii_case(f)));
                        self.handler.events.push(ImEvent::ActiveWindow(fallback));
                    }
                }
                // a client window vanished between its request and our answer: not fatal
                Event::Error(e) => eprintln!("walpad-shell: xim: X error {:?}", e.error_kind),
                ev => {
                    if let Err(e) =
                        self.server
                            .filter_event(&ev, &mut self.conns, &mut self.handler)
                    {
                        eprintln!("walpad-shell: xim: {e}");
                    }
                }
            }
        }
        Ok(std::mem::take(&mut self.handler.events))
    }

    fn reassembled(&mut self, com: u32, data: &[u8]) -> Result<(), XError> {
        let c = self.conn();
        let prop = c
            .intern_atom(false, b"_WALPAD_XIM_REASSEMBLED")?
            .reply()?
            .atom;
        c.change_property(
            PropMode::REPLACE,
            com,
            prop,
            AtomEnum::STRING,
            8,
            data.len() as u32,
            data,
        )?;
        let ev = Event::ClientMessage(ClientMessageEvent {
            response_type: CLIENT_MESSAGE_EVENT,
            format: 32,
            sequence: 0,
            window: com,
            type_: self.atoms.protocol,
            data: [data.len() as u32, prop, 0, 0, 0].into(),
        });
        self.server
            .filter_event(&ev, &mut self.conns, &mut self.handler)
            .map_err(ser)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: IcKey = (100, 1, 1);
    const B: IcKey = (100, 1, 2);
    const C: IcKey = (200, 1, 1);

    /// Focus-in at `now` and run the show delay: what comes out.
    fn focus(m: &mut FocusKbd, ic: IcKey, now: u64) -> Option<KbdAction> {
        m.focus_in(ic, now);
        m.tick(now + SHOW_DELAY_MS)
    }

    #[test]
    fn focus_in_shows_and_focus_out_hides_after_the_debounce() {
        let mut m = FocusKbd::new();
        m.focus_in(A, 0);
        assert_eq!(m.next_deadline(), Some(SHOW_DELAY_MS));
        assert_eq!(m.tick(SHOW_DELAY_MS - 1), None, "show delay");
        assert_eq!(m.tick(SHOW_DELAY_MS), Some(KbdAction::Show));
        assert!(m.visible());
        assert_eq!(focus(&mut m, A, 500), None, "already up");
        m.focus_out(A, 1000);
        assert_eq!(m.next_deadline(), Some(1000 + HIDE_DEBOUNCE_MS));
        assert_eq!(m.tick(1100), None, "debounced");
        assert!(m.visible());
        assert_eq!(m.tick(1250), Some(KbdAction::Hide));
        assert!(!m.visible());
        assert_eq!(m.next_deadline(), None);
        assert_eq!(m.tick(5000), None);
    }

    #[test]
    fn a_focus_that_does_not_stay_never_shows_the_keyboard() {
        // window activation: the last input gets the focus back for a moment, the tap moves on
        let mut m = FocusKbd::new();
        m.focus_in(A, 0);
        m.focus_out(A, 80);
        assert_eq!(m.next_deadline(), None);
        assert_eq!(m.tick(1000), None);
        assert!(!m.visible());
        // repeated focus-ins do not push the delay back
        m.focus_in(A, 2000);
        m.focus_in(B, 2100);
        assert_eq!(m.tick(2000 + SHOW_DELAY_MS), Some(KbdAction::Show));
    }

    #[test]
    fn moving_between_two_fields_does_not_flicker() {
        let mut m = FocusKbd::new();
        focus(&mut m, A, 0);
        // out of A, into B within the debounce
        m.focus_out(A, 1000);
        assert_eq!(focus(&mut m, B, 1050), None);
        assert_eq!(m.tick(2000), None);
        assert!(m.visible());
        // the other order (B in before A out) never even arms the timer
        m.focus_in(A, 3000);
        m.focus_out(B, 3000);
        assert_eq!(m.next_deadline(), None);
        assert_eq!(m.tick(9000), None);
        assert!(m.visible());
    }

    #[test]
    fn destroyed_context_and_vanished_client_hide() {
        let mut m = FocusKbd::new();
        focus(&mut m, C, 0);
        m.focus_out(C, 1000); // destroy is reported like a focus-out
        assert_eq!(m.tick(1000 + HIDE_DEBOUNCE_MS), Some(KbdAction::Hide));
        focus(&mut m, A, 2000);
        m.focus_in(C, 2000);
        m.client_gone(100, 2010);
        assert_eq!(m.tick(3000), None, "C still has the focus");
        m.client_gone(200, 3000);
        assert_eq!(m.tick(3000 + HIDE_DEBOUNCE_MS), Some(KbdAction::Hide));
        // a client without focused inputs going away changes nothing
        m.client_gone(300, 4000);
        assert_eq!(m.next_deadline(), None);
    }

    #[test]
    fn minimise_hides_until_the_next_focus_in() {
        let mut m = FocusKbd::new();
        focus(&mut m, A, 0);
        m.manual(false); // the minimise key while A has the focus
        assert!(!m.visible());
        assert_eq!(m.tick(10_000), None, "stays down");
        m.focus_out(A, 10_000);
        assert_eq!(m.tick(20_000), None);
        assert_eq!(
            focus(&mut m, B, 20_000),
            Some(KbdAction::Show),
            "next focus-in"
        );
        // minimise, then the focus moves on within the debounce: the new field shows it again
        m.manual(false);
        m.focus_out(B, 30_000);
        assert_eq!(focus(&mut m, A, 30_100), Some(KbdAction::Show));
        // minimise right after a focus-in: the pending show is dropped
        let mut m = FocusKbd::new();
        m.focus_in(A, 0);
        m.manual(false);
        assert_eq!(m.tick(SHOW_DELAY_MS), None);
    }

    #[test]
    fn manual_keyboard_without_focus_stays_and_with_focus_follows_it() {
        let mut m = FocusKbd::new();
        // the dock button with nothing focused: stays up whatever happens elsewhere
        m.manual(true);
        m.focus_out(A, 0);
        assert_eq!(m.tick(1000), None);
        assert!(m.visible());
        // a focus-in takes it over: leaving the input then closes it
        assert_eq!(focus(&mut m, A, 1000), None);
        m.focus_out(A, 2000);
        assert_eq!(m.tick(2000 + HIDE_DEBOUNCE_MS), Some(KbdAction::Hide));
        // opened by hand while a field has the focus: leaving the field closes it
        focus(&mut m, B, 3000);
        m.manual(false);
        m.manual(true);
        m.focus_out(B, 5000);
        assert_eq!(m.tick(5000 + HIDE_DEBOUNCE_MS), Some(KbdAction::Hide));
        // a manual hide cancels a pending hide timer (nothing left to do)
        focus(&mut m, A, 6000);
        m.focus_out(A, 7000);
        m.manual(false);
        assert_eq!(m.next_deadline(), None);
    }

    #[test]
    fn terminal_fallback_only_without_xim() {
        // no XIM server: the terminal gets the keyboard at once
        let mut m = FocusKbd::new();
        assert_eq!(m.app_started(0, false), Some(KbdAction::Show));
        assert_eq!(m.app_started(0, false), None, "already up");
        // XIM running, but the app never connects: shown after the fallback delay
        let mut m = FocusKbd::new();
        assert_eq!(m.app_started(1000, true), None);
        assert_eq!(m.next_deadline(), Some(1000 + FALLBACK_MS));
        assert_eq!(m.tick(1000 + FALLBACK_MS - 1), None);
        assert_eq!(m.tick(1000 + FALLBACK_MS), Some(KbdAction::Show));
        // the fallback keyboard does not hide on a stray focus-out (it was not opened by focus)
        m.focus_out(A, 9000);
        assert_eq!(m.tick(20_000), None);
        assert!(m.visible());
        // XIM client connects (st does at start-up): its focus decides, no fallback
        let mut m = FocusKbd::new();
        m.app_started(0, true);
        m.client_connected();
        assert_eq!(m.tick(FALLBACK_MS * 2), None);
        assert!(!m.visible());
        // a focus-in within the delay cancels it as well
        let mut m = FocusKbd::new();
        m.app_started(0, true);
        assert_eq!(focus(&mut m, A, 10), Some(KbdAction::Show));
        m.focus_out(A, 500);
        assert_eq!(m.tick(500 + HIDE_DEBOUNCE_MS), Some(KbdAction::Hide));
        assert_eq!(m.tick(FALLBACK_MS * 2), None, "no late fallback");
        // a manual hide cancels it
        let mut m = FocusKbd::new();
        m.app_started(0, true);
        m.manual(false);
        assert_eq!(m.tick(FALLBACK_MS * 2), None);
    }

    #[test]
    fn active_window_change_shows_and_hides_the_fallback_keyboard() {
        // owner report 2026-11-13: once up, the fallback keyboard never hid on switching away -
        // nothing watched for the active window leaving, only app_started (first launch) raised
        // it. Becoming active is the same trigger `app_started` would have been (covers
        // switching back to an *already-running* terminal/Kali/Scanner too, which never sends a
        // fresh "started" event at all).
        let mut m = FocusKbd::new();
        m.active_window_changed(0, true);
        assert_eq!(m.next_deadline(), Some(FALLBACK_MS));
        assert_eq!(m.tick(FALLBACK_MS - 1), None);
        assert_eq!(m.tick(FALLBACK_MS), Some(KbdAction::Show));
        // switching to a non-fallback window hides it (after the usual debounce)
        m.active_window_changed(FALLBACK_MS, false);
        assert_eq!(m.tick(FALLBACK_MS + HIDE_DEBOUNCE_MS - 1), None);
        assert_eq!(
            m.tick(FALLBACK_MS + HIDE_DEBOUNCE_MS),
            Some(KbdAction::Hide)
        );
        // switching straight back to a fallback app before the hide lands cancels it (no flicker)
        let mut m = FocusKbd::new();
        m.active_window_changed(0, true);
        m.tick(FALLBACK_MS);
        assert!(m.visible());
        m.active_window_changed(FALLBACK_MS + 10, false);
        m.active_window_changed(FALLBACK_MS + 20, true);
        assert_eq!(
            m.tick(FALLBACK_MS + 20 + HIDE_DEBOUNCE_MS),
            None,
            "still up, never hid"
        );
        assert!(m.visible());
        // a keyboard a real focus-in brought up is left alone: active-window changes do not
        // touch it, only a real focus-out does (same contract as before this change)
        let mut m = FocusKbd::new();
        focus(&mut m, A, 0);
        m.tick(SHOW_DELAY_MS);
        assert!(m.visible());
        m.active_window_changed(SHOW_DELAY_MS + 10, false);
        assert_eq!(
            m.next_deadline(),
            None,
            "no hide armed - by_focus governs it, not this"
        );
        assert!(m.visible());
    }

    #[test]
    fn the_server_offers_only_styles_without_preedit_or_status() {
        for s in input_styles() {
            assert!(s.intersects(InputStyle::PREEDIT_NOTHING | InputStyle::PREEDIT_NONE));
            assert!(s.intersects(InputStyle::STATUS_NOTHING | InputStyle::STATUS_NONE));
            assert!(!s.intersects(
                InputStyle::PREEDIT_CALLBACKS
                    | InputStyle::PREEDIT_POSITION
                    | InputStyle::PREEDIT_AREA
                    | InputStyle::STATUS_CALLBACKS
                    | InputStyle::STATUS_AREA
            ));
        }
        assert!(LOCALES.split(',').any(|l| l == "C"));
        assert!(LOCALES.split(',').any(|l| l == "en_US.UTF-8"));
        assert!(!LOCALES.contains(' '));
    }
}
