//! JSON payload parsers, time/duration/text formatting, and status helpers for [`super::PhoneModel`].
use super::{CallEntry, CallKind, ContactEntry, SmsEntry, MAX_DIAL};
use serde_json::Value;
use walpad_proto::{CallState, PhoneStatus, Status};
use walpad_ui::tokens::*;
use walpad_ui::Color;

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

pub(super) fn phone(st: &Option<Status>) -> Option<&PhoneStatus> {
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
        return ("System service not responding".into(), danger());
    }
    match phone(st) {
        None => ("Waiting for the system".into(), text_disabled()),
        Some(p) if !p.available => (
            p.reason
                .clone()
                .unwrap_or_else(|| "Phone unavailable".into()),
            warning(),
        ),
        Some(p) if p.audio == "experimental" => (
            "Call audio is experimental: it may be silent".into(),
            text_secondary(),
        ),
        Some(_) => (String::new(), text_secondary()),
    }
}
