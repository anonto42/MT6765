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
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Instant;
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, Window, WindowClass,
};

/// Entries kept.
mod history;
mod selection;
#[cfg(test)]
mod tests;

pub use history::{ClipEvent, History, ENTRY_MAX, FETCH_TIMEOUT, HISTORY_MAX};

struct Atoms {
    clipboard: Atom,
    primary: Atom,
    utf8: Atom,
    targets: Atom,
    incr: Atom,
    prop: Atom,
}

pub(super) struct Fetch {
    selection: Atom,
    target: Atom,
    deadline: Instant,
}

pub struct Keeper {
    pub(super) xc: XConn,
    pub(super) win: Window,
    a: Atoms,
    pub(super) history: History,
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

    /// Put `text` on the clipboard and on top of the history (the GPS screen's Copy buttons).
    /// Longer than an entry may be: ignored.
    pub fn set_text(&mut self, text: &str) -> Result<(), XError> {
        if text.is_empty() || text.len() > ENTRY_MAX {
            return Ok(());
        }
        self.history.push(text.to_string());
        self.own(text.to_string())?;
        self.events.push(ClipEvent::HistoryChanged);
        Ok(())
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
}
