//! Clipboard keeper (owner request 2026-11-10: "I'm unable to copy and paste anything"): what an
//! app copied survives the app closing, and the last ten text entries can be pasted again.
//!
//! X selections live in the app that owns them: close the app and the clipboard is gone. The
//! keeper (a pure-Rust `xclip`/`autocutsel`/clipboard-manager in the shell, on its own X
//! connection) watches CLIPBOARD with XFixes; when an app takes it the keeper reads the text
//! (`UTF8_STRING`, else `STRING`), puts it on top of its history and becomes the owner itself, so
//! the text outlives the source app and later pastes are answered by the shell. PRIMARY (what
//! selecting text sets) is only observed: an owner change is reported (the selection toolbar
//! shows), and `copy_primary` copies it into the clipboard on request (the toolbar's Copy in a
//! terminal, where Ctrl+C is an interrupt, not a copy). PRIMARY is never copied on its own: a new
//! selection must not clobber what the user copied before.
//!
//! Text only, newest first, at most `HISTORY_MAX` distinct entries of at most `ENTRY_MAX` bytes;
//! INCR (very large) transfers are ignored. Clipboard text is never logged.
//!
//! Every wait on the connection has a deadline (walpad-x); a source app that never answers costs
//! `FETCH_TIMEOUT` and nothing else.
use std::collections::VecDeque;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, SelectionNotifyEvent,
    SelectionRequestEvent, Window, WindowClass, SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

/// Entries kept.
pub const HISTORY_MAX: usize = 10;
/// Largest entry (bytes): bigger copies are ignored.
pub const ENTRY_MAX: usize = 64 * 1024;
/// A source app that does not answer a conversion within this time is given up on.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(1);

/// The text history, newest first (pure: unit-tested).
#[derive(Debug, Default, Clone)]
pub struct History {
    items: VecDeque<String>,
}

impl History {
    /// Add an entry on top. Empty or blank text, NUL bytes and text over `ENTRY_MAX` are refused
    /// (false); a text already in the list moves to the top; the oldest entry falls off.
    pub fn push(&mut self, text: String) -> bool {
        if text.trim().is_empty() || text.len() > ENTRY_MAX || text.contains('\0') {
            return false;
        }
        self.items.retain(|t| *t != text);
        self.items.push_front(text);
        self.items.truncate(HISTORY_MAX);
        true
    }

    /// Move entry `i` to the top and return it.
    pub fn promote(&mut self, i: usize) -> Option<&str> {
        let t = self.items.remove(i)?;
        self.items.push_front(t);
        self.items.front().map(String::as_str)
    }

    pub fn get(&self, i: usize) -> Option<&str> {
        self.items.get(i).map(String::as_str)
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn clear(&mut self) {
        self.items.clear();
    }
    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.items.iter()
    }
}

/// What happened (the shell reacts: redraw the list, show the selection toolbar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipEvent {
    /// The history changed.
    HistoryChanged,
    /// PRIMARY got a new owner (`true`: text may be selected) or lost it.
    PrimaryChanged(bool),
}

struct Atoms {
    clipboard: Atom,
    primary: Atom,
    utf8: Atom,
    targets: Atom,
    incr: Atom,
    prop: Atom,
}

struct Fetch {
    selection: Atom,
    target: Atom,
    deadline: Instant,
}

pub struct Keeper {
    xc: XConn,
    win: Window,
    a: Atoms,
    history: History,
    /// The text answered to requests while we own CLIPBOARD.
    served: Option<String>,
    fetch: Option<Fetch>,
    /// A selection to read once the running fetch is over.
    queued: Option<Atom>,
    events: Vec<ClipEvent>,
}

impl Keeper {
    pub fn open(socket: &Path) -> Result<Keeper, XError> {
        let xc = walpad_x::connect(socket)?;
        xc.conn.xfixes_query_version(4, 0)?.reply()?;
        let intern = |n: &str| -> Result<Atom, XError> {
            Ok(xc.conn.intern_atom(false, n.as_bytes())?.reply()?.atom)
        };
        let a = Atoms {
            clipboard: intern("CLIPBOARD")?,
            primary: AtomEnum::PRIMARY.into(),
            utf8: intern("UTF8_STRING")?,
            targets: intern("TARGETS")?,
            incr: intern("INCR")?,
            prop: intern("WALPAD_CLIPBOARD")?,
        };
        let win = xc.conn.generate_id()?;
        xc.conn.create_window(
            0,
            win,
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
        let mask = SelectionEventMask::SET_SELECTION_OWNER
            | SelectionEventMask::SELECTION_WINDOW_DESTROY
            | SelectionEventMask::SELECTION_CLIENT_CLOSE;
        for sel in [a.clipboard, a.primary] {
            xc.conn.xfixes_select_selection_input(win, sel, mask)?;
        }
        xc.conn.flush()?;
        let mut k = Keeper {
            xc,
            win,
            a,
            history: History::default(),
            served: None,
            fetch: None,
            queued: None,
            events: Vec::new(),
        };
        // an app may already own the clipboard (the shell restarted): keep it
        k.start_fetch_if_owned()?;
        Ok(k)
    }

    pub fn wait_fd(&self) -> &UnixStream {
        &self.xc.wait_fd
    }
    pub fn history(&self) -> &History {
        &self.history
    }
    /// When `poll` needs to run again without X traffic (a fetch deadline).
    pub fn next_deadline(&self) -> Option<Instant> {
        self.fetch.as_ref().map(|f| f.deadline)
    }

    fn start_fetch_if_owned(&mut self) -> Result<(), XError> {
        let o = self
            .xc
            .conn
            .get_selection_owner(self.a.clipboard)?
            .reply()?;
        if o.owner != 0 && o.owner != self.win {
            self.start_fetch(self.a.clipboard)?;
        }
        Ok(())
    }

    fn start_fetch(&mut self, selection: Atom) -> Result<(), XError> {
        if self.fetch.is_some() {
            self.queued = Some(selection);
            return Ok(());
        }
        self.convert(selection, self.a.utf8)
    }

    fn convert(&mut self, selection: Atom, target: Atom) -> Result<(), XError> {
        self.xc.conn.convert_selection(
            self.win,
            selection,
            target,
            self.a.prop,
            x11rb::CURRENT_TIME,
        )?;
        self.xc.conn.flush()?;
        self.fetch = Some(Fetch {
            selection,
            target,
            deadline: Instant::now() + FETCH_TIMEOUT,
        });
        Ok(())
    }

    fn fetch_done(&mut self) -> Result<(), XError> {
        self.fetch = None;
        if let Some(sel) = self.queued.take() {
            self.start_fetch(sel)?;
        }
        Ok(())
    }

    /// Become the CLIPBOARD owner with `text` (already on top of the history).
    fn own(&mut self, text: String) -> Result<(), XError> {
        self.served = Some(text);
        self.xc
            .conn
            .set_selection_owner(self.win, self.a.clipboard, x11rb::CURRENT_TIME)?;
        self.xc.conn.flush()?;
        Ok(())
    }

    /// Paste support: entry `i` becomes the clipboard (and the top of the history). The caller
    /// then sends the paste key.
    pub fn paste(&mut self, i: usize) -> Result<bool, XError> {
        let Some(text) = self.history.promote(i).map(str::to_string) else {
            return Ok(false);
        };
        self.own(text)?;
        self.events.push(ClipEvent::HistoryChanged);
        Ok(true)
    }

    /// Copy what is selected (PRIMARY) into the clipboard and the history. Answers nothing: the
    /// result arrives as a `HistoryChanged` event.
    pub fn copy_primary(&mut self) -> Result<(), XError> {
        let o = self.xc.conn.get_selection_owner(self.a.primary)?.reply()?;
        if o.owner != 0 && o.owner != self.win {
            self.start_fetch(self.a.primary)?;
        }
        Ok(())
    }

    /// Forget everything, including what we serve.
    pub fn clear(&mut self) -> Result<(), XError> {
        self.history.clear();
        if self.served.take().is_some() {
            self.xc
                .conn
                .set_selection_owner(x11rb::NONE, self.a.clipboard, x11rb::CURRENT_TIME)?;
            self.xc.conn.flush()?;
        }
        self.events.push(ClipEvent::HistoryChanged);
        Ok(())
    }

    fn on_fetched(&mut self, e: SelectionNotifyEvent) -> Result<(), XError> {
        let Some(f) = self.fetch.take() else {
            return Ok(());
        };
        if e.selection != f.selection {
            // an answer to an older request: keep waiting for ours
            self.fetch = Some(f);
            return Ok(());
        }
        if e.property == x11rb::NONE {
            if f.target == self.a.utf8 {
                // no UTF-8 on offer: plain Latin-1
                return self.convert(f.selection, AtomEnum::STRING.into());
            }
            return self.fetch_done();
        }
        let p = self
            .xc
            .conn
            .get_property(
                true,
                self.win,
                self.a.prop,
                AtomEnum::ANY,
                0,
                (ENTRY_MAX / 4 + 1) as u32,
            )?
            .reply()?;
        // INCR (huge) and over-long values are ignored
        let text = if p.type_ == self.a.incr || p.bytes_after > 0 || p.format != 8 {
            None
        } else if p.type_ == self.a.utf8 {
            String::from_utf8(p.value).ok()
        } else if p.type_ == u32::from(AtomEnum::STRING) {
            Some(p.value.iter().map(|b| *b as char).collect())
        } else {
            None
        };
        if let Some(t) = text {
            if self.history.push(t) {
                if let Some(top) = self.history.get(0).map(str::to_string) {
                    self.own(top)?;
                }
                self.events.push(ClipEvent::HistoryChanged);
            }
        }
        self.fetch_done()
    }

    /// Answer a request for the CLIPBOARD we own.
    fn serve(&mut self, e: SelectionRequestEvent) -> Result<(), XError> {
        let prop = if e.property == x11rb::NONE {
            e.target
        } else {
            e.property
        };
        let c = &self.xc.conn;
        let mut ok = false;
        if let (true, Some(text)) = (e.selection == self.a.clipboard, self.served.as_ref()) {
            let string: Atom = AtomEnum::STRING.into();
            if e.target == self.a.targets {
                let t = [self.a.targets, self.a.utf8, string];
                c.change_property32(PropMode::REPLACE, e.requestor, prop, AtomEnum::ATOM, &t)?;
                ok = true;
            } else if e.target == self.a.utf8 {
                c.change_property8(
                    PropMode::REPLACE,
                    e.requestor,
                    prop,
                    self.a.utf8,
                    text.as_bytes(),
                )?;
                ok = true;
            } else if e.target == string {
                let latin1: Vec<u8> = text
                    .chars()
                    .map(|ch| u8::try_from(ch as u32).unwrap_or(b'?'))
                    .collect();
                c.change_property8(PropMode::REPLACE, e.requestor, prop, string, &latin1)?;
                ok = true;
            }
        }
        let reply = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: e.time,
            requestor: e.requestor,
            selection: e.selection,
            target: e.target,
            property: if ok { prop } else { x11rb::NONE },
        };
        c.send_event(false, e.requestor, EventMask::NO_EVENT, reply)?;
        c.flush()?;
        Ok(())
    }

    /// Handle everything the server has sent (non-blocking).
    pub fn poll(&mut self) -> Result<Vec<ClipEvent>, XError> {
        while let Some(ev) = self.xc.conn.poll_for_event()? {
            match ev {
                Event::XfixesSelectionNotify(e) => {
                    if e.owner == self.win {
                        continue; // our own takeover
                    }
                    if e.selection == self.a.clipboard {
                        if e.owner != x11rb::NONE {
                            self.start_fetch(self.a.clipboard)?;
                        }
                    } else if e.selection == self.a.primary {
                        self.events
                            .push(ClipEvent::PrimaryChanged(e.owner != x11rb::NONE));
                    }
                }
                Event::SelectionNotify(e) if e.requestor == self.win => self.on_fetched(e)?,
                Event::SelectionRequest(e) if e.owner == self.win => self.serve(e)?,
                Event::Error(e) => return Err(XError(format!("clipboard X error: {e:?}"))),
                _ => {}
            }
        }
        if self
            .fetch
            .as_ref()
            .is_some_and(|f| Instant::now() >= f.deadline)
        {
            self.fetch_done()?;
        }
        Ok(std::mem::take(&mut self.events))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_newest_first_deduplicated_and_capped() {
        let mut h = History::default();
        for i in 0..12 {
            assert!(h.push(format!("entry {i}")));
        }
        assert_eq!(h.len(), HISTORY_MAX);
        assert_eq!(h.get(0), Some("entry 11"));
        assert_eq!(h.get(9), Some("entry 2"));
        // a repeat moves to the top, no duplicate
        assert!(h.push("entry 5".into()));
        assert_eq!(h.len(), HISTORY_MAX);
        assert_eq!(h.get(0), Some("entry 5"));
        assert_eq!(h.iter().filter(|t| *t == "entry 5").count(), 1);
    }

    #[test]
    fn history_refuses_blank_huge_and_binary_text() {
        let mut h = History::default();
        assert!(!h.push(String::new()));
        assert!(!h.push(" \n\t".into()));
        assert!(!h.push("a\0b".into()));
        assert!(!h.push("x".repeat(ENTRY_MAX + 1)));
        assert!(h.push("x".repeat(ENTRY_MAX)));
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn promote_moves_an_entry_to_the_top() {
        let mut h = History::default();
        for t in ["a", "b", "c"] {
            h.push(t.into());
        }
        assert_eq!(h.promote(2), Some("a"));
        assert_eq!(h.iter().cloned().collect::<Vec<_>>(), ["a", "c", "b"]);
        assert_eq!(h.promote(9), None);
        h.clear();
        assert!(h.is_empty());
    }
}
