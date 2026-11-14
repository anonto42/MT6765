//! Phone screen behaviour (owner request 2026-11-11, change `phone-dialer`):
//! core data replies, notices, thread opening, and missed-call summary. Numbers and texts only
//! live in the model and in `Secret`s of the commands; nothing here prints them.

use super::model::friendly;
use super::{App, Cmd, Outcome, Purpose};
use crate::core_client::ReplyError;
use crate::overlay::{Overlay, PasswordSheet, SheetPurpose};
use crate::phone_ui::{self, CallKind, PhoneView};
use serde_json::Value;
use walpad_proto::Secret;
use walpad_ui::components::{Tab, Tone};

fn secret(s: &str) -> Secret {
    Secret::new(s)
}

impl App {
    /// The Phone screen opens (or the core came back while it is open): fetch what it shows.
    pub(super) fn open_phone(&mut self, out: &mut Outcome) {
        if self.model.connected {
            out.cmd(Cmd::CallLog);
            out.cmd(Cmd::ContactList);
            out.cmd(Cmd::SmsList);
        }
    }

    /// A list the core sent.
    pub(super) fn on_phone_data(&mut self, purpose: Purpose, data: &Value) {
        let m = &mut self.model.phone;
        match purpose {
            Purpose::PhoneSms => m.sms = phone_ui::parse_sms(data),
            Purpose::PhoneContacts => m.contacts = phone_ui::parse_contacts(data),
            Purpose::PhoneCalls => m.calls = phone_ui::parse_calls(data),
            _ => {}
        }
    }

    /// A send, delete or save was answered: refresh the lists, or say why it failed.
    pub(super) fn on_phone_action_reply(
        &mut self,
        ok: bool,
        _data: &Value,
        error: Option<&ReplyError>,
        out: &mut Outcome,
    ) {
        if ok {
            self.open_phone(out);
        } else if let Some(e) = error {
            if e.code != "busy" {
                self.toast(friendly(e), Tone::Danger);
            }
        }
    }

    /// Notices of the phone module: toasts and list refreshes. The call screen itself is the
    /// shell loop's call window (it shows over every app); this is for the main window.
    pub(super) fn on_phone_notice(&mut self, v: &Value, out: &mut Outcome) {
        out.dirty = true;
        let who = |this: &Self| -> String {
            let number = v["number"].as_str().unwrap_or("");
            v["name"]
                .as_str()
                .map(String::from)
                .or_else(|| this.model.phone.contact_name(number).map(String::from))
                .unwrap_or_else(|| {
                    if number.is_empty() {
                        "unknown number".into()
                    } else {
                        number.to_string()
                    }
                })
        };
        match v["kind"].as_str() {
            Some("sms_received") => {
                let text = format!("New message from {}", who(self));
                self.toast(text, Tone::Info);
                if self.active == Tab::Phone {
                    out.cmd(Cmd::SmsList);
                }
            }
            Some("sms_sent") => {
                if v["ok"] != true {
                    let why = v["error"].as_str().unwrap_or("unknown error").to_string();
                    self.toast(format!("Message not sent: {why}"), Tone::Danger);
                }
                if self.active == Tab::Phone {
                    out.cmd(Cmd::SmsList);
                }
            }
            Some("call_ended") => {
                if v["reason"] == "missed" {
                    let text = format!("Missed call from {}", who(self));
                    self.toast(text, Tone::Warning);
                }
                if self.active == Tab::Phone {
                    out.cmd(Cmd::CallLog);
                }
            }
            Some("incoming_call") => {
                // the call window covers everything; this is the fallback when it cannot open
                let text = format!("Incoming call from {}", who(self));
                self.toast(text, Tone::Info);
            }
            Some("phone_error") => {
                let what = v["what"].as_str().unwrap_or("request");
                self.toast(format!("Phone: {what} failed"), Tone::Danger);
            }
            _ => {}
        }
    }

    pub(super) fn phone_dial(&mut self, number: &str, out: &mut Outcome) {
        if self.model.connected && !number.is_empty() {
            out.cmd(Cmd::PhoneDial(secret(number)));
        }
    }

    pub(super) fn open_text_sheet(&mut self, purpose: SheetPurpose, title: &str, initial: &str) {
        self.model.overlay = Some(Overlay::Password(PasswordSheet::phone(
            purpose, title, initial,
        )));
    }

    pub(super) fn open_thread(&mut self, number: String, out: &mut Outcome) {
        // reading the conversation marks it read (the unread badge follows with the next status)
        for m in self
            .model
            .phone
            .sms
            .iter_mut()
            .filter(|m| walpad_proto::same_number(&m.number, &number))
        {
            m.read = true;
        }
        if self.model.connected {
            out.cmd(Cmd::SmsRead(secret(&number)));
        }
        self.model.phone.thread = Some(number);
        self.model.phone.view = PhoneView::Messages;
        self.scroll = 0;
        self.content_hits.clear();
    }

    /// Missed calls for the badge-less summary (used by tests and the Phone tab title).
    pub fn missed_calls(&self) -> usize {
        self.model
            .phone
            .calls
            .iter()
            .filter(|c| c.kind == CallKind::Missed)
            .count()
    }
}
