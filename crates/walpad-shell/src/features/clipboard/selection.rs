use super::{ClipEvent, Keeper, ENTRY_MAX};
use std::time::Instant;
use walpad_x::XError;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, EventMask, PropMode, SelectionNotifyEvent,
    SelectionRequestEvent, SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

impl Keeper {
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
