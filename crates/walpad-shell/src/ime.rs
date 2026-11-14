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
mod focus;
mod handler;
#[cfg(test)]
mod tests;

pub use focus::{
    FocusKbd, IcKey, ImEvent, KbdAction, FALLBACK_MS, HIDE_DEBOUNCE_MS, IM_NAME, SHOW_DELAY_MS,
};
pub use handler::{input_styles, ImConn, LOCALES};
use handler::{Handler, XServer};
use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::path::Path;
use walpad_x::{TimedStream, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConnectionExt as _, EventMask,
    PropMode, Window as XWindow, WindowClass, CLIENT_MESSAGE_EVENT,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use xim::x11rb::{HasConnection, X11rbServer};
use xim::{ServerError, XimConnections};

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
