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
use crate::app::Target;
use serde_json::Value;
use walpad_proto::{same_number, CallState, PhoneStatus, Status};
use walpad_ui::components::{Accessory, ButtonKind, ButtonState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Icon, Rect, Style};

pub const MAX_DIAL: usize = 32;
const SEG_H: i32 = 56;
const NOTICE_H: i32 = 36;
const KEY_H: i32 = 76;
const KEY_GAP: i32 = 12;
const DISPLAY_H: i32 = 76;
const GRID_MAX_W: i32 = 480;
const CALL_H: i32 = 64;
const LINE_H: i32 = 22;
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

// ---------------------------------------------------------------------------- JSON -> model
fn s(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or("").to_string()
}

pub fn parse_calls(data: &Value) -> Vec<CallEntry> {
    data["calls"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|c| CallEntry {
                    number: s(c, "number"),
                    name: c["name"].as_str().map(String::from),
                    ts: c["ts"].as_u64().unwrap_or(0),
                    dur_s: c["dur_s"].as_u64().unwrap_or(0),
                    kind: match c["kind"].as_str() {
                        Some("in") => CallKind::In,
                        Some("missed") => CallKind::Missed,
                        _ => CallKind::Out,
                    },
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_contacts(data: &Value) -> Vec<ContactEntry> {
    data["contacts"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    Some(ContactEntry {
                        id: c["id"].as_str()?.to_string(),
                        name: s(c, "name"),
                        numbers: c["numbers"]
                            .as_array()?
                            .iter()
                            .filter_map(|n| n.as_str().map(String::from))
                            .collect(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_sms(data: &Value) -> Vec<SmsEntry> {
    data["messages"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    Some(SmsEntry {
                        id: m["id"].as_u64()?,
                        number: s(m, "number"),
                        name: m["name"].as_str().map(String::from),
                        text: s(m, "text"),
                        ts: m["ts"].as_u64().unwrap_or(0),
                        incoming: m["incoming"] == true,
                        read: m["read"] != false,
                        state: s(m, "state"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------- formatting
/// "14:05" today, "Mon 09 Nov" within the year, "09 Nov 2025" older.
pub fn fmt_when(ts_ms: u64, now_ms: u64, tz: &jiff::tz::TimeZone) -> String {
    let (Ok(t), Ok(n)) = (
        jiff::Timestamp::from_millisecond(ts_ms as i64),
        jiff::Timestamp::from_millisecond(now_ms as i64),
    ) else {
        return String::new();
    };
    let (t, n) = (t.to_zoned(tz.clone()), n.to_zoned(tz.clone()));
    if t.date() == n.date() {
        t.strftime("%H:%M").to_string()
    } else if t.year() == n.year() {
        t.strftime("%a %d %b").to_string()
    } else {
        t.strftime("%d %b %Y").to_string()
    }
}

/// `m:ss`, or `h:mm:ss` from an hour on.
pub fn fmt_duration(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

/// Word-wrap `text` to lines of at most `max` characters (a single word longer than that is cut).
pub fn wrap(text: &str, max: usize) -> Vec<String> {
    let max = max.max(4);
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut cur = String::new();
        for word in para.split(' ') {
            let mut w: String = word.to_string();
            while w.chars().count() > max {
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                }
                let head: String = w.chars().take(max).collect();
                w = w.chars().skip(max).collect();
                lines.push(head);
            }
            let need = cur.chars().count() + usize::from(!cur.is_empty()) + w.chars().count();
            if need > max && !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(&w);
        }
        lines.push(cur);
    }
    lines
}

pub fn call_kind_text(k: CallKind) -> &'static str {
    match k {
        CallKind::In => "Incoming",
        CallKind::Out => "Outgoing",
        CallKind::Missed => "Missed",
    }
}

/// Why a call ended, in words.
pub fn end_reason_text(r: Option<&str>) -> &'static str {
    match r {
        Some("busy") => "Busy",
        Some("no_answer") => "No answer",
        Some("rejected") => "Declined",
        Some("missed") => "Missed",
        Some("failed") => "Call failed",
        Some("hangup") => "Call ended",
        _ => "Call ended",
    }
}

// ---------------------------------------------------------------------------- geometry
fn phone(st: &Option<Status>) -> Option<&PhoneStatus> {
    st.as_ref().map(|s| &s.phone)
}

/// A call is on (the call window covers the screen): the keypad cannot start another.
pub fn in_call(st: &Option<Status>) -> bool {
    phone(st).is_some_and(|p| !matches!(p.call, CallState::Idle))
}

pub fn can_call(st: &Option<Status>, online: bool, number: &str) -> bool {
    online
        && !number.is_empty()
        && number.len() <= MAX_DIAL
        && phone(st).is_some_and(|p| p.available && p.call == CallState::Idle)
}

/// The unread count for the badges.
pub fn unread(st: &Option<Status>) -> u32 {
    phone(st).map_or(0, |p| p.unread)
}

/// What the line under the tabs says: (text, colour).
pub fn notice_line(st: &Option<Status>, online: bool) -> (String, Color) {
    if !online {
        return ("System service not responding".into(), DANGER);
    }
    match phone(st) {
        None => ("Waiting for the system".into(), TEXT_DISABLED),
        Some(p) if !p.available => (
            p.reason
                .clone()
                .unwrap_or_else(|| "Phone unavailable".into()),
            WARNING,
        ),
        // the honest part: signalling works, call sound is not verified on this device
        Some(_) => (
            "Call audio is experimental: it may be silent".into(),
            TEXT_SECONDARY,
        ),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeypadGeom {
    pub display: Rect,
    pub backspace: Rect,
    pub keys: Vec<(Rect, char)>,
    pub call: Rect,
    pub bottom: i32,
}

/// The dial pad: 3x4 keys, then `+` and the Call button.
pub fn keypad_geom(frame: &Frame, y0: i32) -> KeypadGeom {
    let c = frame.padded();
    let gw = c.w.min(GRID_MAX_W);
    let gx = c.x + (c.w - gw) / 2;
    let kw = (gw - 2 * KEY_GAP) / 3;
    let display = Rect::new(gx, y0, gw, DISPLAY_H);
    let backspace = Rect::new(display.right() - 72, display.y, 72, DISPLAY_H);
    let mut keys = Vec::new();
    let chars = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '*', '0', '#'];
    let top = display.bottom() + KEY_GAP;
    for (i, ch) in chars.iter().enumerate() {
        let (col, row) = ((i % 3) as i32, (i / 3) as i32);
        keys.push((
            Rect::new(
                gx + col * (kw + KEY_GAP),
                top + row * (KEY_H + KEY_GAP),
                kw,
                KEY_H,
            ),
            *ch,
        ));
    }
    let ry = top + 4 * (KEY_H + KEY_GAP);
    keys.push((Rect::new(gx, ry, kw, CALL_H), '+'));
    let call = Rect::new(gx + kw + KEY_GAP, ry, 2 * kw + KEY_GAP, CALL_H);
    KeypadGeom {
        display,
        backspace,
        keys,
        call,
        bottom: call.bottom(),
    }
}

fn body_top() -> i32 {
    SP_4 + SEG_H + NOTICE_H
}

/// Bubble geometry of the open conversation: (rect, wrapped lines) per message, newest first.
pub fn thread_geom(frame: &Frame, y0: i32, msgs: &[&SmsEntry]) -> (Vec<(Rect, Vec<String>)>, i32) {
    let c = frame.padded();
    let bw = ((c.w as f32 * 0.78) as i32).min(520);
    let chars = ((bw - 2 * SP_3) as f32 / (SIZE_BODY * 0.56)) as usize;
    let mut y = y0;
    let mut out = Vec::new();
    for m in msgs {
        let lines = wrap(&m.text, chars);
        let h = lines.len() as i32 * LINE_H + 2 * SP_3;
        let x = if m.incoming { c.x } else { c.right() - bw };
        out.push((Rect::new(x, y, bw, h), lines));
        y += h + 20 + SP_3; // the time line under the bubble
    }
    (out, y)
}

const ROW: i32 = ROW_H;

/// Total content height of the screen for the current view.
pub fn layout_total(frame: &Frame, m: &PhoneModel, st: &Option<Status>) -> i32 {
    let y0 = body_top();
    let rows = |n: usize, extra: i32| y0 + n as i32 * ROW + extra + SP_6;
    match m.view {
        PhoneView::Keypad => keypad_geom(frame, y0).bottom + SP_6,
        PhoneView::Recents => {
            let n = m.recents().len();
            rows(n.max(5), if n > 0 { SP_3 + BUTTON_H } else { 0 })
        }
        PhoneView::Contacts => {
            let n = m.filtered_contacts().len();
            rows(n.max(4), SEG_H + SP_3)
        }
        PhoneView::Messages => match &m.thread {
            None => {
                let n = m.conversations().len();
                rows(n.max(4), BUTTON_H + SP_3)
            }
            Some(_) => {
                let msgs = m.thread_messages();
                let top = y0 + ROW + SP_2 + BUTTON_H + SP_4;
                let (_, bottom) = thread_geom(frame, top, &msgs);
                let _ = st;
                bottom.max(top + 200) + SP_6
            }
        },
    }
}

// ---------------------------------------------------------------------------- rendering
pub struct View<'a> {
    pub st: &'a Option<Status>,
    pub m: &'a PhoneModel,
    pub online: bool,
    pub pressed: Option<Target>,
}

fn pt(t: PhoneTarget) -> Target {
    Target::Phone(t)
}

/// A key of the dial pad: sharp box, big digit.
fn key_box(p: &mut Painter, r: Rect, label: &str, pressed: bool, disabled: bool) {
    p.outline_box(r, 0, pressed, false, disabled);
    p.text(
        label,
        Style::new(
            Face::Regular,
            34.0,
            if disabled { TEXT_DISABLED } else { TEXT },
        ),
        r,
        Align::Center,
    );
}

pub fn render(p: &mut Painter, frame: &Frame, v: &View) -> Vec<(Rect, Target)> {
    let c = frame.padded();
    let mut hits: Vec<(Rect, Target)> = Vec::new();
    let pr = |t: PhoneTarget| v.pressed == Some(pt(t));
    let m = v.m;
    // tabs
    let seg = Rect::new(c.x, SP_4, c.w, SEG_H);
    let unread = unread(v.st);
    let msg_label = if unread > 0 {
        format!("Messages ({unread})")
    } else {
        "Messages".to_string()
    };
    let labels = ["Keypad", "Recents", "Contacts", msg_label.as_str()];
    let selected = PhoneView::ALL
        .iter()
        .position(|x| *x == m.view)
        .unwrap_or(0);
    let pressed_seg = PhoneView::ALL
        .iter()
        .position(|x| v.pressed == Some(pt(PhoneTarget::View(*x))));
    let rects = p.segmented(seg, &labels, selected, pressed_seg, false);
    for (r, view) in rects.iter().zip(PhoneView::ALL) {
        hits.push((*r, pt(PhoneTarget::View(view))));
    }
    let (note, note_col) = notice_line(v.st, v.online);
    p.text(
        &note,
        Style::new(Face::Regular, SIZE_CAPTION, note_col),
        Rect::new(c.x + SP_1, seg.bottom() + SP_1, c.w - SP_2, NOTICE_H - SP_2),
        Align::Left,
    );
    let y0 = body_top();
    match m.view {
        PhoneView::Keypad => {
            let g = keypad_geom(frame, y0);
            p.outline_box(g.display, 0, false, false, false);
            let shown = if m.dial.is_empty() {
                "Enter a number".to_string()
            } else {
                m.dial.clone()
            };
            let col = if m.dial.is_empty() {
                TEXT_DISABLED
            } else {
                TEXT
            };
            let size = if m.dial.chars().count() > 16 {
                26.0
            } else {
                34.0
            };
            p.text(
                &shown,
                Style::new(Face::Regular, size, col),
                Rect::new(
                    g.display.x + SP_4,
                    g.display.y,
                    g.display.w - SP_4 - 72,
                    g.display.h,
                ),
                Align::Left,
            );
            if !m.dial.is_empty() {
                let b = g.backspace;
                p.cv.icon(
                    Icon::Backspace,
                    b.x + (b.w - ICON) / 2,
                    b.y + (b.h - ICON) / 2,
                    ICON,
                    if pr(PhoneTarget::Backspace) {
                        TEXT
                    } else {
                        TEXT_SECONDARY
                    },
                );
                hits.push((b, pt(PhoneTarget::Backspace)));
            }
            let full = m.dial.len() >= MAX_DIAL;
            for (r, ch) in &g.keys {
                let t = PhoneTarget::Digit(*ch);
                key_box(p, *r, &ch.to_string(), pr(t), full);
                hits.push((*r, pt(t)));
            }
            let ok = can_call(v.st, v.online, &m.dial);
            let state = if !ok {
                ButtonState::Disabled
            } else if pr(PhoneTarget::Call) {
                ButtonState::Pressed
            } else {
                ButtonState::Default
            };
            p.button(
                g.call,
                "Call",
                ButtonKind::Primary,
                state,
                Some(Icon::Phone),
            );
            hits.push((g.call, pt(PhoneTarget::Call)));
        }
        PhoneView::Recents => {
            let recents = m.recents();
            if recents.is_empty() {
                p.empty_state(
                    Rect::new(c.x, y0, c.w, 5 * ROW),
                    Icon::Phone,
                    "No calls yet",
                    "Calls you make and receive show up here",
                );
            } else {
                p.row_group(Rect::new(c.x, y0, c.w, recents.len() as i32 * ROW));
                let now = m.clock_ms();
                for (i, e) in recents.iter().enumerate() {
                    let r = Rect::new(c.x, y0 + i as i32 * ROW, c.w, ROW);
                    let title = e
                        .name
                        .clone()
                        .or_else(|| m.contact_name(&e.number).map(String::from))
                        .unwrap_or_else(|| {
                            if e.number.is_empty() {
                                "Unknown".into()
                            } else {
                                e.number.clone()
                            }
                        });
                    let dur = if e.dur_s > 0 {
                        format!(" · {}", fmt_duration(e.dur_s))
                    } else {
                        String::new()
                    };
                    let sub = format!("{}{dur}", call_kind_text(e.kind));
                    let when = fmt_when(e.ts, now, &m.tz);
                    let t = PhoneTarget::Recent(i);
                    p.list_row(
                        r,
                        Some(Icon::Phone),
                        &title,
                        Some(&sub),
                        &Accessory::Value(when),
                        pr(t),
                        false,
                    );
                    if e.kind == CallKind::Missed {
                        p.cv.rect(Rect::new(r.x + SP_4 + ICON - 4, r.y + 10, 8, 8), DANGER);
                    }
                    hits.push((r, pt(t)));
                }
                let by = y0 + recents.len() as i32 * ROW + SP_3;
                let b = Rect::new(c.x, by, c.w.min(320), BUTTON_H);
                p.button(
                    b,
                    "Clear call log",
                    ButtonKind::Secondary,
                    if pr(PhoneTarget::ClearLog) {
                        ButtonState::Pressed
                    } else {
                        ButtonState::Default
                    },
                    None,
                );
                hits.push((b, pt(PhoneTarget::ClearLog)));
            }
        }
        PhoneView::Contacts => {
            let search = Rect::new(c.x, y0, c.w - 120 - SP_3, SEG_H);
            let add = Rect::new(c.right() - 120, y0, 120, SEG_H);
            p.outline_box(search, 0, pr(PhoneTarget::Search), false, false);
            let (txt, col) = if m.search.is_empty() {
                ("Search contacts".to_string(), TEXT_DISABLED)
            } else {
                (m.search.clone(), TEXT)
            };
            p.text(
                &txt,
                Style::new(Face::Regular, SIZE_BODY, col),
                Rect::new(search.x + SP_4, search.y, search.w - 2 * SP_4, search.h),
                Align::Left,
            );
            hits.push((search, pt(PhoneTarget::Search)));
            p.button(
                add,
                "Add",
                ButtonKind::Primary,
                if pr(PhoneTarget::AddContact) {
                    ButtonState::Pressed
                } else {
                    ButtonState::Default
                },
                Some(Icon::Person),
            );
            hits.push((add, pt(PhoneTarget::AddContact)));
            let list = m.filtered_contacts();
            let ly = y0 + SEG_H + SP_3;
            if list.is_empty() {
                let (title, hint) = if m.contacts.is_empty() {
                    ("No contacts", "Tap Add to save one")
                } else {
                    ("No match", "Try another name or number")
                };
                p.empty_state(Rect::new(c.x, ly, c.w, 4 * ROW), Icon::Person, title, hint);
            } else {
                p.row_group(Rect::new(c.x, ly, c.w, list.len() as i32 * ROW));
                for (i, e) in list.iter().enumerate() {
                    let r = Rect::new(c.x, ly + i as i32 * ROW, c.w, ROW);
                    let sub = match e.numbers.len() {
                        0 => String::new(),
                        1 => e.numbers[0].clone(),
                        n => format!("{} (+{} more)", e.numbers[0], n - 1),
                    };
                    let t = PhoneTarget::Contact(i);
                    p.list_row(
                        r,
                        Some(Icon::Person),
                        &e.name,
                        Some(&sub),
                        &Accessory::Chevron,
                        pr(t),
                        false,
                    );
                    hits.push((r, pt(t)));
                }
            }
        }
        PhoneView::Messages => match &m.thread {
            None => render_conversations(p, frame, v, y0, &mut hits),
            Some(num) => render_thread(p, frame, v, y0, num, &mut hits),
        },
    }
    hits
}

fn render_conversations(
    p: &mut Painter,
    frame: &Frame,
    v: &View,
    y0: i32,
    hits: &mut Vec<(Rect, Target)>,
) {
    let c = frame.padded();
    let m = v.m;
    let pr = |t: PhoneTarget| v.pressed == Some(pt(t));
    let ok = v.online && phone(v.st).is_some_and(|ph| ph.available);
    let nb = Rect::new(c.x, y0, c.w, BUTTON_H);
    let state = if !ok {
        ButtonState::Disabled
    } else if pr(PhoneTarget::NewMessage) {
        ButtonState::Pressed
    } else {
        ButtonState::Default
    };
    p.button(
        nb,
        "New message",
        ButtonKind::Primary,
        state,
        Some(Icon::Message),
    );
    hits.push((nb, pt(PhoneTarget::NewMessage)));
    let convs = m.conversations();
    let ly = y0 + BUTTON_H + SP_3;
    if convs.is_empty() {
        p.empty_state(
            Rect::new(c.x, ly, c.w, 4 * ROW),
            Icon::Message,
            "No messages",
            "Text messages you send and receive show up here",
        );
        return;
    }
    p.row_group(Rect::new(c.x, ly, c.w, convs.len() as i32 * ROW));
    let now = m.clock_ms();
    for (i, cv) in convs.iter().enumerate() {
        let r = Rect::new(c.x, ly + i as i32 * ROW, c.w, ROW);
        let title = m.label(&cv.number);
        let first = cv.last_text.lines().next().unwrap_or("");
        let sub = if cv.last_incoming {
            first.to_string()
        } else {
            format!("You: {first}")
        };
        let t = PhoneTarget::Thread(i);
        p.list_row(
            r,
            Some(Icon::Message),
            &title,
            Some(&sub),
            &Accessory::Value(fmt_when(cv.last_ts, now, &m.tz)),
            pr(t),
            false,
        );
        if cv.unread > 0 {
            p.cv.rect(Rect::new(r.x + SP_4 + ICON - 5, r.y + 9, 10, 10), TEXT);
        }
        hits.push((r, pt(t)));
    }
}

fn render_thread(
    p: &mut Painter,
    frame: &Frame,
    v: &View,
    y0: i32,
    num: &str,
    hits: &mut Vec<(Rect, Target)>,
) {
    let c = frame.padded();
    let m = v.m;
    let pr = |t: PhoneTarget| v.pressed == Some(pt(t));
    let back = Rect::new(c.x, y0, 64, ROW);
    p.cv.icon(
        Icon::Back,
        back.x + (back.w - ICON) / 2,
        back.y + (back.h - ICON) / 2,
        ICON,
        if pr(PhoneTarget::ThreadBack) {
            TEXT
        } else {
            TEXT_SECONDARY
        },
    );
    hits.push((back, pt(PhoneTarget::ThreadBack)));
    let call = Rect::new(c.right() - 112, y0 + 4, 112, ROW - 8);
    let ok = can_call(v.st, v.online, num);
    p.button(
        call,
        "Call",
        ButtonKind::Secondary,
        if !ok {
            ButtonState::Disabled
        } else if pr(PhoneTarget::ThreadCall) {
            ButtonState::Pressed
        } else {
            ButtonState::Default
        },
        Some(Icon::Phone),
    );
    hits.push((call, pt(PhoneTarget::ThreadCall)));
    let title = m.label(num);
    p.text(
        &title,
        Style::new(Face::Bold, SIZE_BODY, TEXT),
        Rect::new(back.right(), y0, call.x - back.right() - SP_2, ROW),
        Align::Left,
    );
    let reply = Rect::new(c.x, y0 + ROW + SP_2, c.w, BUTTON_H);
    let can_send = v.online && phone(v.st).is_some_and(|ph| ph.available);
    p.button(
        reply,
        "Write a message",
        ButtonKind::Primary,
        if !can_send {
            ButtonState::Disabled
        } else if pr(PhoneTarget::Reply) {
            ButtonState::Pressed
        } else {
            ButtonState::Default
        },
        Some(Icon::Message),
    );
    hits.push((reply, pt(PhoneTarget::Reply)));
    let msgs = m.thread_messages();
    let top = reply.bottom() + SP_4;
    let (geom, _) = thread_geom(frame, top, &msgs);
    if msgs.is_empty() {
        p.empty_state(
            Rect::new(c.x, top, c.w, 200),
            Icon::Message,
            "No messages",
            "Write the first one",
        );
    }
    let now = m.clock_ms();
    for (i, ((r, lines), msg)) in geom.iter().zip(&msgs).enumerate() {
        let t = PhoneTarget::Bubble(i);
        p.cv.rect(
            *r,
            if msg.incoming {
                SURFACE
            } else {
                SURFACE_STRONG
            },
        );
        if pr(t) {
            p.cv.rect(*r, PRESSED_TINT);
        }
        if msg.incoming {
            p.cv.rect(Rect::new(r.x, r.y, 3, r.h), OUTLINE);
        } else {
            p.cv.rect(Rect::new(r.right() - 3, r.y, 3, r.h), TEXT);
        }
        for (k, l) in lines.iter().enumerate() {
            p.text(
                l,
                Style::new(Face::Regular, SIZE_BODY, TEXT),
                Rect::new(
                    r.x + SP_3,
                    r.y + SP_3 + k as i32 * LINE_H,
                    r.w - 2 * SP_3,
                    LINE_H,
                ),
                Align::Left,
            );
        }
        let state = match (msg.incoming, msg.state.as_str()) {
            (false, "sending") => " · Sending...",
            (false, "failed") => " · Not sent",
            (false, "sent") => " · Sent",
            _ => "",
        };
        let col = if state.contains("Not sent") {
            DANGER
        } else {
            TEXT_SECONDARY
        };
        p.text(
            &format!("{}{state}", fmt_when(msg.ts, now, &m.tz)),
            Style::new(Face::Regular, SIZE_CAPTION, col),
            Rect::new(r.x + SP_1, r.bottom() + 2, r.w, 18),
            if msg.incoming {
                Align::Left
            } else {
                Align::Right
            },
        );
        hits.push((*r, pt(t)));
    }
}

// ---------------------------------------------------------------------------- sheets
const SHEET_W: i32 = 800;

/// The stacked buttons of a bottom sheet: (target, label, kind, icon).
pub type SheetButtons = Vec<(PhoneTarget, String, ButtonKind, Option<Icon>)>;

pub fn sheet_buttons(sheet: &PhoneSheet, m: &PhoneModel) -> (String, SheetButtons) {
    let mut b: Vec<(PhoneTarget, String, ButtonKind, Option<Icon>)> = Vec::new();
    let title = match sheet {
        PhoneSheet::Contact(id) => {
            let Some(c) = m.contact_by_id(id) else {
                return (String::new(), Vec::new());
            };
            for (i, n) in c.numbers.iter().enumerate() {
                b.push((
                    PhoneTarget::SheetCall(i),
                    format!("Call {n}"),
                    ButtonKind::Primary,
                    Some(Icon::Phone),
                ));
            }
            b.push((
                PhoneTarget::SheetMessage,
                "Message".into(),
                ButtonKind::Secondary,
                Some(Icon::Message),
            ));
            b.push((
                PhoneTarget::SheetEdit,
                "Edit".into(),
                ButtonKind::Secondary,
                None,
            ));
            b.push((
                PhoneTarget::SheetDelete,
                "Delete contact".into(),
                ButtonKind::Danger,
                None,
            ));
            c.name.clone()
        }
        PhoneSheet::Message(id) => {
            let failed = m
                .sms
                .iter()
                .any(|x| x.id == *id && !x.incoming && x.state == "failed");
            if failed {
                b.push((
                    PhoneTarget::SheetResend,
                    "Send again".into(),
                    ButtonKind::Primary,
                    None,
                ));
            }
            b.push((
                PhoneTarget::SheetDelete,
                "Delete message".into(),
                ButtonKind::Danger,
                None,
            ));
            "Message".into()
        }
        PhoneSheet::DeleteContact(_) | PhoneSheet::ClearLog => String::new(),
    };
    b.push((
        PhoneTarget::SheetClose,
        "Cancel".into(),
        ButtonKind::Secondary,
        None,
    ));
    (title, b)
}

/// Geometry of the stacked-button sheet: the sheet rect and the button rects.
pub fn sheet_geom(frame: &Frame, n: usize) -> (Rect, Vec<Rect>) {
    let h = 68 + n as i32 * (BUTTON_H + SP_3) + SP_4;
    let w = SHEET_W.min(frame.w);
    let sheet = Rect::new((frame.w - w) / 2, frame.h - h, w, h);
    let mut y = sheet.y + 68;
    let rects = (0..n)
        .map(|_| {
            let r = Rect::new(sheet.x + SP_5, y, sheet.w - 2 * SP_5, BUTTON_H);
            y += BUTTON_H + SP_3;
            r
        })
        .collect();
    (sheet, rects)
}

pub fn render_sheet(
    p: &mut Painter,
    frame: &Frame,
    sheet: &PhoneSheet,
    m: &PhoneModel,
) -> Vec<(Rect, Target)> {
    match sheet {
        PhoneSheet::DeleteContact(id) => {
            let name = m
                .contact_by_id(id)
                .map_or("this contact", |c| c.name.as_str());
            let (cancel, confirm) = p.dialog(
                frame.screen(),
                "Delete contact?",
                &format!("{name} will be removed from your contacts."),
                "Cancel",
                "Delete",
                "Delete",
                true,
                false,
            );
            vec![
                (cancel, pt(PhoneTarget::SheetClose)),
                (confirm, pt(PhoneTarget::SheetConfirm)),
            ]
        }
        PhoneSheet::ClearLog => {
            let (cancel, confirm) = p.dialog(
                frame.screen(),
                "Clear call log?",
                "All recent calls will be removed.",
                "Cancel",
                "Clear",
                "Clear",
                true,
                false,
            );
            vec![
                (cancel, pt(PhoneTarget::SheetClose)),
                (confirm, pt(PhoneTarget::SheetConfirm)),
            ]
        }
        _ => {
            let (title, buttons) = sheet_buttons(sheet, m);
            let (rect, rects) = sheet_geom(frame, buttons.len());
            p.sheet(rect, &title);
            let mut hits = vec![(rect, Target::PanelBody)];
            for ((t, label, kind, icon), r) in buttons.iter().zip(rects) {
                p.button(r, label, *kind, ButtonState::Default, *icon);
                hits.push((r, pt(*t)));
            }
            hits
        }
    }
}

// ---------------------------------------------------------------------------- badge
/// A small unread count on the top-right corner of an icon box.
pub fn paint_badge(p: &mut Painter, icon_box: Rect, count: u32) {
    if count == 0 {
        return;
    }
    let text = if count > 9 {
        "9+".to_string()
    } else {
        count.to_string()
    };
    let r = Rect::new(icon_box.right() - 14, icon_box.y - 6, 22, 22);
    p.cv.circle(
        (r.x + 11) as f32,
        (r.y + 11) as f32,
        11.0,
        walpad_ui::tokens::DANGER,
    );
    p.text(
        &text,
        Style::new(Face::Bold, SIZE_LABEL, walpad_ui::tokens::BG),
        r,
        Align::Center,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sms(id: u64, n: &str, t: &str, ts: u64, inc: bool, read: bool) -> SmsEntry {
        SmsEntry {
            id,
            number: n.into(),
            name: None,
            text: t.into(),
            ts,
            incoming: inc,
            read,
            state: if inc { "".into() } else { "sent".into() },
        }
    }

    fn model() -> PhoneModel {
        PhoneModel {
            tz: jiff::tz::TimeZone::UTC,
            now_ms: Some(1_605_000_000_000),
            ..Default::default()
        }
    }

    #[test]
    fn wrap_breaks_words_and_long_runs() {
        assert_eq!(
            wrap("hello brave new world", 11),
            ["hello brave", "new world"]
        );
        assert_eq!(wrap("a\nb", 10), ["a", "b"]);
        assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(wrap("", 10), [""]);
        // Bangla counts characters, not bytes
        assert_eq!(wrap("আমি ভালো আছি", 8), ["আমি ভালো", "আছি"]);
    }

    #[test]
    fn times_and_durations_read_naturally() {
        let tz = jiff::tz::TimeZone::UTC;
        let now = 1_605_000_000_000; // 2020-11-10 09:20 UTC
        assert_eq!(fmt_when(now - 60_000, now, &tz), "09:19");
        assert_eq!(fmt_when(now - 86_400_000, now, &tz), "Mon 09 Nov");
        assert_eq!(fmt_when(now - 400 * 86_400_000, now, &tz), "07 Oct 2019");
        assert_eq!(fmt_duration(5), "0:05");
        assert_eq!(fmt_duration(754), "12:34");
        assert_eq!(fmt_duration(3725), "1:02:05");
    }

    #[test]
    fn conversations_group_national_and_international_numbers() {
        let mut m = model();
        m.sms = vec![
            sms(1, "01711234567", "hi", 10, true, true),
            sms(2, "+8801711234567", "hello back", 20, false, true),
            sms(3, "01555000111", "other", 15, true, false),
            sms(4, "+8801711234567", "again", 30, true, false),
        ];
        let c = m.conversations();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].count, 3);
        assert_eq!(c[0].unread, 1);
        assert_eq!(c[0].last_text, "again");
        assert_eq!(c[1].number, "01555000111");
        m.thread = Some("01711234567".into());
        let t = m.thread_messages();
        assert_eq!(
            t.iter().map(|x| x.id).collect::<Vec<_>>(),
            [4, 2, 1],
            "newest first"
        );
    }

    #[test]
    fn contacts_filter_by_name_or_number() {
        let mut m = model();
        m.contacts = vec![
            ContactEntry {
                id: "c1".into(),
                name: "Zara".into(),
                numbers: vec!["01711234567".into()],
            },
            ContactEntry {
                id: "c2".into(),
                name: "amir".into(),
                numbers: vec!["+8801555000111".into()],
            },
        ];
        assert_eq!(m.filtered_contacts()[0].name, "amir");
        m.search = "ZAR".into();
        assert_eq!(m.filtered_contacts().len(), 1);
        m.search = "555".into();
        assert_eq!(m.filtered_contacts()[0].id, "c2");
        m.search = "zzz".into();
        assert!(m.filtered_contacts().is_empty());
        assert_eq!(m.contact_name("01555000111"), Some("amir"));
        assert_eq!(m.label("01999999999"), "01999999999");
    }

    #[test]
    fn keypad_keys_are_big_and_do_not_overlap_in_both_orientations() {
        for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
            let g = keypad_geom(&f, body_top());
            assert_eq!(g.keys.len(), 13, "12 keys and +");
            let mut rs: Vec<Rect> = g.keys.iter().map(|k| k.0).collect();
            rs.push(g.call);
            rs.push(g.display);
            for r in &rs {
                assert!(r.h >= TOUCH_MIN && r.w >= TOUCH_MIN, "{r:?}");
                assert!(r.x >= f.padded().x && r.right() <= f.padded().right());
            }
            for (i, a) in rs.iter().enumerate() {
                for b in &rs[i + 1..] {
                    let o =
                        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom();
                    assert!(!o, "{a:?} overlaps {b:?}");
                }
            }
            assert!(g.backspace.h >= TOUCH_MIN);
            let chars: String = g.keys.iter().map(|k| k.1).collect();
            assert_eq!(chars, "123456789*0#+");
        }
    }

    #[test]
    fn call_button_needs_a_usable_line_and_a_number() {
        let mut st = crate::app::tests::status(120, &[]);
        st.phone.available = true;
        let st = Some(st);
        assert!(can_call(&st, true, "999"));
        assert!(!can_call(&st, true, ""));
        assert!(!can_call(&st, false, "999"));
        assert!(!can_call(&None, true, "999"));
        let mut busy = st.clone();
        busy.as_mut().unwrap().phone.call = CallState::Active;
        assert!(!can_call(&busy, true, "999"));
        let mut off = st.clone();
        off.as_mut().unwrap().phone.available = false;
        off.as_mut().unwrap().phone.reason = Some("No usable SIM card".into());
        assert!(!can_call(&off, true, "999"));
        assert_eq!(notice_line(&off, true).0, "No usable SIM card");
        assert!(notice_line(&st, true).0.contains("experimental"));
        assert_eq!(notice_line(&st, false).1, DANGER);
    }

    #[test]
    fn json_parsers_are_tolerant() {
        let calls = parse_calls(&serde_json::json!({"calls": [
            {"number": "999", "ts": 5, "dur_s": 7, "kind": "missed", "name": null},
            {"number": "1", "kind": "weird"}]}));
        assert_eq!(calls[0].kind, CallKind::Missed);
        assert_eq!(calls[1].kind, CallKind::Out);
        assert!(parse_calls(&Value::Null).is_empty());
        let c = parse_contacts(&serde_json::json!({"contacts": [
            {"id": "c1", "name": "A", "numbers": ["012"]}, {"name": "no id"}]}));
        assert_eq!(c.len(), 1);
        let s = parse_sms(&serde_json::json!({"messages": [
            {"id": 3, "number": "1", "text": "t", "ts": 1, "incoming": true, "read": false, "state": ""}]}));
        assert!(s[0].incoming && !s[0].read);
    }

    #[test]
    fn thread_bubbles_stack_without_overlap_and_total_covers_them() {
        let mut m = model();
        m.view = PhoneView::Messages;
        m.thread = Some("01711234567".into());
        m.sms = (1..=6)
            .map(|i| {
                sms(
                    i,
                    "01711234567",
                    &"word ".repeat(i as usize * 12),
                    i * 10,
                    i % 2 == 0,
                    true,
                )
            })
            .collect();
        for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
            let msgs = m.thread_messages();
            let (g, bottom) = thread_geom(&f, 300, &msgs);
            for w in g.windows(2) {
                assert!(w[0].0.bottom() < w[1].0.y);
            }
            assert!(g
                .iter()
                .all(|(r, _)| r.right() <= f.padded().right() && r.x >= f.padded().x));
            let st = Some(crate::app::tests::status(120, &[]));
            assert!(layout_total(&f, &m, &st) >= bottom - 300);
        }
    }

    #[test]
    fn sheets_have_a_cancel_and_stack_inside_the_screen() {
        let mut m = model();
        m.contacts = vec![ContactEntry {
            id: "c1".into(),
            name: "Zara".into(),
            numbers: vec!["01711234567".into(), "01811234567".into()],
        }];
        let (title, b) = sheet_buttons(&PhoneSheet::Contact("c1".into()), &m);
        assert_eq!(title, "Zara");
        let kinds: Vec<PhoneTarget> = b.iter().map(|x| x.0).collect();
        assert_eq!(kinds[0], PhoneTarget::SheetCall(0));
        assert_eq!(kinds[1], PhoneTarget::SheetCall(1));
        assert_eq!(*kinds.last().unwrap(), PhoneTarget::SheetClose);
        for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
            let (sheet, rs) = sheet_geom(&f, b.len());
            assert!(sheet.y >= 0 && sheet.bottom() <= f.h);
            assert!(rs
                .iter()
                .all(|r| r.h >= TOUCH_MIN && r.bottom() <= sheet.bottom()));
        }
        m.sms = vec![
            SmsEntry {
                state: "failed".into(),
                ..sms(9, "01711234567", "x", 1, false, true)
            },
            sms(8, "01711234567", "y", 1, false, true),
        ];
        let (_, failed) = sheet_buttons(&PhoneSheet::Message(9), &m);
        assert_eq!(failed[0].0, PhoneTarget::SheetResend);
        let (_, sent) = sheet_buttons(&PhoneSheet::Message(8), &m);
        assert_eq!(sent[0].0, PhoneTarget::SheetDelete);
    }
}
