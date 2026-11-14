//! The Phone screen (owner request 2026-11-11, change `phone-dialer`): dial pad, recent calls,
//! contacts and text messages. Pure model and geometry as in mobile.rs / store.rs; content
//! coordinates start at y = 0 at the top of the scrollable content. Numbers, names and message
//! texts live only in this model and on screen: they are never logged.
//!
//! Text entry (search, contact name and number, message) uses the shell's own text sheet with its
//! on-screen keyboard (`overlay::PasswordSheet`): it does not depend on X focus. It is a Latin
//! keyboard: Bangla messages can be received and shown (given a font with the glyphs) and sent,
//! but not typed here yet (the keyboard-languages work in docs/40-tablet-feature-plan.md).
//! Call audio is experimental until verified on the device: the screen says so under the tabs.
mod geom;
mod messages;
mod parse;
mod render;
mod sheet;
#[cfg(test)]
mod tests;

pub use geom::{keypad_geom, layout_total, thread_geom, KeypadGeom};
pub use parse::{
    call_kind_text, can_call, end_reason_text, fmt_duration, fmt_when, in_call, notice_line,
    parse_calls, parse_contacts, parse_sms, unread, wrap,
};
pub use render::{render, View};
pub use sheet::{paint_badge, render_sheet, sheet_buttons, sheet_geom, SheetButtons};

use walpad_proto::same_number;
use walpad_ui::tokens::ROW_H;

pub const MAX_DIAL: usize = 32;
pub(super) const SEG_H: i32 = 56;
pub(super) const NOTICE_H: i32 = 36;
pub(super) const KEY_H: i32 = 76;
pub(super) const KEY_GAP: i32 = 12;
pub(super) const DISPLAY_H: i32 = 76;
pub(super) const GRID_MAX_W: i32 = 480;
pub(super) const CALL_H: i32 = 64;
pub(super) const LINE_H: i32 = 22;
pub(super) const ROW: i32 = ROW_H;
const MAX_ROWS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhoneView {
    Keypad,
    Recents,
    Contacts,
    Messages,
}

impl PhoneView {
    pub const ALL: [PhoneView; 4] = [
        PhoneView::Keypad,
        PhoneView::Recents,
        PhoneView::Contacts,
        PhoneView::Messages,
    ];
    pub fn label(self) -> &'static str {
        match self {
            PhoneView::Keypad => "Keypad",
            PhoneView::Recents => "Recents",
            PhoneView::Contacts => "Contacts",
            PhoneView::Messages => "Messages",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    In,
    Out,
    Missed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallEntry {
    pub number: String,
    pub name: Option<String>,
    pub ts: u64,
    pub dur_s: u64,
    pub kind: CallKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactEntry {
    pub id: String,
    pub name: String,
    pub numbers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmsEntry {
    pub id: u64,
    pub number: String,
    pub name: Option<String>,
    pub text: String,
    pub ts: u64,
    pub incoming: bool,
    pub read: bool,
    pub state: String,
}

/// A contact being created or changed: the name sheet, then the number sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactDraft {
    pub id: Option<String>,
    pub name: String,
    pub number: String,
}

/// The modal sheets of the Phone screen (`Overlay::Phone`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhoneSheet {
    /// Actions for a contact (by id).
    Contact(String),
    /// Actions for a message (by id).
    Message(u64),
    DeleteContact(String),
    ClearLog,
}

/// Everything a Phone tap can mean (`Target::Phone`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhoneTarget {
    View(PhoneView),
    Digit(char),
    Backspace,
    Call,
    /// Recent call, newest first.
    Recent(usize),
    ClearLog,
    Search,
    AddContact,
    SimContacts,
    UssdDismiss,
    /// Contact in the filtered list.
    Contact(usize),
    NewMessage,
    /// Conversation in the list (newest first).
    Thread(usize),
    ThreadBack,
    ThreadCall,
    Reply,
    /// Message bubble (index in the open conversation, newest first).
    Bubble(usize),
    /// The sheet's buttons.
    SheetCall(usize),
    SheetMessage,
    SheetEdit,
    SheetDelete,
    SheetResend,
    SheetClose,
    SheetConfirm,
}

pub struct PhoneModel {
    pub view: PhoneView,
    pub dial: String,
    pub calls: Vec<CallEntry>,
    pub contacts: Vec<ContactEntry>,
    pub sms: Vec<SmsEntry>,
    pub search: String,
    /// The open conversation (a number), None = the conversation list.
    pub thread: Option<String>,
    pub draft: Option<ContactDraft>,
    /// Time zone for the times shown; the system's, UTC in tests.
    pub tz: jiff::tz::TimeZone,
    /// "Now" in epoch milliseconds for relative dates; None = the clock.
    pub now_ms: Option<u64>,
}

impl Default for PhoneModel {
    fn default() -> Self {
        PhoneModel {
            view: PhoneView::Keypad,
            dial: String::new(),
            calls: Vec::new(),
            contacts: Vec::new(),
            sms: Vec::new(),
            search: String::new(),
            thread: None,
            draft: None,
            tz: jiff::tz::TimeZone::system(),
            now_ms: None,
        }
    }
}

impl PhoneModel {
    pub fn clock_ms(&self) -> u64 {
        self.now_ms
            .unwrap_or_else(|| u64::try_from(jiff::Timestamp::now().as_millisecond()).unwrap_or(0))
    }

    pub fn contact_name(&self, number: &str) -> Option<&str> {
        self.contacts
            .iter()
            .find(|c| c.numbers.iter().any(|n| same_number(n, number)))
            .map(|c| c.name.as_str())
    }

    pub fn contact_by_id(&self, id: &str) -> Option<&ContactEntry> {
        self.contacts.iter().find(|c| c.id == id)
    }

    /// The contacts matching the search text (name or digits of a number), sorted by name.
    pub fn filtered_contacts(&self) -> Vec<&ContactEntry> {
        let q = self.search.trim().to_lowercase();
        let qd: String = q.chars().filter(char::is_ascii_digit).collect();
        let mut v: Vec<&ContactEntry> = self
            .contacts
            .iter()
            .filter(|c| {
                q.is_empty()
                    || c.name.to_lowercase().contains(&q)
                    || (!qd.is_empty()
                        && c.numbers.iter().any(|n| {
                            n.chars()
                                .filter(char::is_ascii_digit)
                                .collect::<String>()
                                .contains(&qd)
                        }))
            })
            .collect();
        v.sort_by_key(|c| c.name.to_lowercase());
        v
    }

    /// Recent calls, newest first.
    pub fn recents(&self) -> Vec<&CallEntry> {
        self.calls.iter().rev().take(MAX_ROWS).collect()
    }

    /// Conversations, newest first.
    pub fn conversations(&self) -> Vec<Conversation> {
        let mut out: Vec<Conversation> = Vec::new();
        for m in &self.sms {
            match out.iter_mut().find(|c| same_number(&c.number, &m.number)) {
                Some(c) => {
                    c.count += 1;
                    c.unread += u32::from(m.incoming && !m.read);
                    if m.ts >= c.last_ts {
                        c.last_ts = m.ts;
                        c.last_text = m.text.clone();
                        c.last_incoming = m.incoming;
                        c.number = m.number.clone();
                    }
                }
                None => out.push(Conversation {
                    number: m.number.clone(),
                    last_ts: m.ts,
                    last_text: m.text.clone(),
                    last_incoming: m.incoming,
                    unread: u32::from(m.incoming && !m.read),
                    count: 1,
                }),
            }
        }
        out.sort_by_key(|c| std::cmp::Reverse(c.last_ts));
        out.truncate(MAX_ROWS);
        out
    }

    /// The messages of the open conversation, newest first.
    pub fn thread_messages(&self) -> Vec<&SmsEntry> {
        let Some(t) = &self.thread else {
            return Vec::new();
        };
        let mut v: Vec<&SmsEntry> = self
            .sms
            .iter()
            .filter(|m| same_number(&m.number, t))
            .collect();
        v.sort_by_key(|m| std::cmp::Reverse((m.ts, m.id)));
        v.truncate(MAX_ROWS);
        v
    }

    /// Name for a number: the contact, else the number itself.
    pub fn label(&self, number: &str) -> String {
        self.contact_name(number).unwrap_or(number).to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    pub number: String,
    pub last_ts: u64,
    pub last_text: String,
    pub last_incoming: bool,
    pub unread: u32,
    pub count: usize,
}
