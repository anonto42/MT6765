//! Phone screen behaviour (owner request 2026-11-11, change `phone-dialer`): taps on the dial pad,
//! recents, contacts and messages, the text sheets (search, contact name and number, message),
//! and what the core's data replies and notices change. Numbers and texts only live in the model
//! and in `Secret`s of the commands; nothing here prints them.
use super::*;
use crate::phone_ui::{CallKind, ContactDraft, PhoneSheet, PhoneView};

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

    fn phone_dial(&mut self, number: &str, out: &mut Outcome) {
        if self.model.connected && !number.is_empty() {
            out.cmd(Cmd::PhoneDial(secret(number)));
        }
    }

    fn open_text_sheet(&mut self, purpose: SheetPurpose, title: &str, initial: &str) {
        self.model.overlay = Some(Overlay::Password(PasswordSheet::phone(
            purpose, title, initial,
        )));
    }

    fn open_thread(&mut self, number: String, out: &mut Outcome) {
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

    pub(super) fn on_phone_tap(&mut self, t: PhoneTarget, out: &mut Outcome) {
        let online = self.model.connected;
        match t {
            PhoneTarget::View(v) => {
                self.model.phone.view = v;
                self.model.phone.thread = None;
                self.scroll = 0;
                self.content_hits.clear();
                // fresh data for the list that opens
                if online {
                    out.cmd(match v {
                        PhoneView::Keypad | PhoneView::Recents => Cmd::CallLog,
                        PhoneView::Contacts => Cmd::ContactList,
                        PhoneView::Messages => Cmd::SmsList,
                    });
                }
            }
            PhoneTarget::Digit(c) => {
                if self.model.phone.dial.len() < phone_ui::MAX_DIAL {
                    self.model.phone.dial.push(c);
                }
            }
            PhoneTarget::Backspace => {
                self.model.phone.dial.pop();
            }
            PhoneTarget::Call => {
                let n = self.model.phone.dial.clone();
                if phone_ui::can_call(&self.model.status, online, &n) {
                    self.phone_dial(&n, out);
                    self.model.phone.dial.clear();
                }
            }
            PhoneTarget::Recent(i) => {
                let n = self
                    .model
                    .phone
                    .recents()
                    .get(i)
                    .map(|c| c.number.clone())
                    .unwrap_or_default();
                if !n.is_empty() {
                    // fills the pad: the owner presses Call (no accidental call from a list tap)
                    self.model.phone.dial = n;
                    self.model.phone.view = PhoneView::Keypad;
                    self.scroll = 0;
                    self.content_hits.clear();
                }
            }
            PhoneTarget::ClearLog => {
                self.model.overlay = Some(Overlay::Phone(PhoneSheet::ClearLog))
            }
            PhoneTarget::Search => {
                let cur = self.model.phone.search.clone();
                self.open_text_sheet(SheetPurpose::PhoneSearch, "Search contacts", &cur);
            }
            PhoneTarget::AddContact => {
                self.model.phone.draft = Some(ContactDraft {
                    id: None,
                    name: String::new(),
                    number: String::new(),
                });
                self.open_text_sheet(SheetPurpose::PhoneContactName, "New contact", "");
            }
            PhoneTarget::SimContacts => out.cmd(Cmd::PhoneSimContacts),
            PhoneTarget::UssdDismiss => out.cmd(Cmd::PhoneUssdCancel),
            PhoneTarget::Contact(i) => {
                let id = self
                    .model
                    .phone
                    .filtered_contacts()
                    .get(i)
                    .map(|c| c.id.clone());
                if let Some(id) = id {
                    self.model.overlay = Some(Overlay::Phone(PhoneSheet::Contact(id)));
                }
            }
            PhoneTarget::NewMessage => {
                if self
                    .model
                    .status
                    .as_ref()
                    .is_some_and(|s| s.phone.available)
                {
                    self.open_text_sheet(SheetPurpose::PhoneNewNumber, "New message", "");
                }
            }
            PhoneTarget::Thread(i) => {
                let n = self
                    .model
                    .phone
                    .conversations()
                    .get(i)
                    .map(|c| c.number.clone());
                if let Some(n) = n {
                    self.open_thread(n, out);
                }
            }
            PhoneTarget::ThreadBack => {
                self.model.phone.thread = None;
                self.scroll = 0;
                self.content_hits.clear();
            }
            PhoneTarget::ThreadCall => {
                if let Some(n) = self.model.phone.thread.clone() {
                    if phone_ui::can_call(&self.model.status, online, &n) {
                        self.phone_dial(&n, out);
                    }
                }
            }
            PhoneTarget::Reply => {
                if let Some(n) = self.model.phone.thread.clone() {
                    if self
                        .model
                        .status
                        .as_ref()
                        .is_some_and(|s| s.phone.available)
                    {
                        let title = format!("Message to {}", self.model.phone.label(&n));
                        self.open_text_sheet(SheetPurpose::PhoneCompose, &title, "");
                    }
                }
            }
            PhoneTarget::Bubble(i) => {
                let id = self.model.phone.thread_messages().get(i).map(|m| m.id);
                if let Some(id) = id {
                    self.model.overlay = Some(Overlay::Phone(PhoneSheet::Message(id)));
                }
            }
            PhoneTarget::SheetCall(i) => {
                if let Some(Overlay::Phone(PhoneSheet::Contact(id))) = self.model.overlay.clone() {
                    let n = self
                        .model
                        .phone
                        .contact_by_id(&id)
                        .and_then(|c| c.numbers.get(i).cloned());
                    self.model.overlay = None;
                    if let Some(n) = n {
                        if phone_ui::can_call(&self.model.status, online, &n) {
                            self.phone_dial(&n, out);
                        }
                    }
                }
            }
            PhoneTarget::SheetMessage => {
                if let Some(Overlay::Phone(PhoneSheet::Contact(id))) = self.model.overlay.clone() {
                    let n = self
                        .model
                        .phone
                        .contact_by_id(&id)
                        .and_then(|c| c.numbers.first().cloned());
                    self.model.overlay = None;
                    if let Some(n) = n {
                        self.open_thread(n, out);
                    }
                }
            }
            PhoneTarget::SheetEdit => {
                if let Some(Overlay::Phone(PhoneSheet::Contact(id))) = self.model.overlay.clone() {
                    let c = self.model.phone.contact_by_id(&id).cloned();
                    if let Some(c) = c {
                        self.model.phone.draft = Some(ContactDraft {
                            id: Some(c.id.clone()),
                            name: c.name.clone(),
                            number: c.numbers.first().cloned().unwrap_or_default(),
                        });
                        self.open_text_sheet(
                            SheetPurpose::PhoneContactName,
                            "Edit contact",
                            &c.name,
                        );
                    }
                }
            }
            PhoneTarget::SheetDelete => match self.model.overlay.clone() {
                Some(Overlay::Phone(PhoneSheet::Contact(id))) => {
                    self.model.overlay = Some(Overlay::Phone(PhoneSheet::DeleteContact(id)));
                }
                Some(Overlay::Phone(PhoneSheet::Message(id))) => {
                    self.model.overlay = None;
                    if online {
                        out.cmd(Cmd::SmsDelete(id));
                    }
                }
                _ => {}
            },
            PhoneTarget::SheetResend => {
                if let Some(Overlay::Phone(PhoneSheet::Message(id))) = self.model.overlay.clone() {
                    let m = self.model.phone.sms.iter().find(|m| m.id == id).cloned();
                    self.model.overlay = None;
                    if let (Some(m), true) = (m, online) {
                        out.cmd(Cmd::SmsSend {
                            number: secret(&m.number),
                            text: secret(&m.text),
                        });
                        out.cmd(Cmd::SmsDelete(id));
                    }
                }
            }
            PhoneTarget::SheetClose => self.close_overlay(),
            PhoneTarget::SheetConfirm => match self.model.overlay.take() {
                Some(Overlay::Phone(PhoneSheet::DeleteContact(id))) if online => {
                    out.cmd(Cmd::ContactDelete(id));
                }
                Some(Overlay::Phone(PhoneSheet::ClearLog)) if online => {
                    out.cmd(Cmd::CallLogClear);
                    self.model.phone.calls.clear();
                }
                other => self.model.overlay = other,
            },
        }
    }

    /// The text sheet's confirm button of a Phone purpose.
    pub(super) fn phone_submit(&mut self, out: &mut Outcome) {
        let Some(Overlay::Password(s)) = &mut self.model.overlay else {
            return;
        };
        if !s.valid() {
            s.error = Some(
                match s.purpose {
                    SheetPurpose::PhoneContactName => "Give the contact a name",
                    SheetPurpose::PhoneContactNumber | SheetPurpose::PhoneNewNumber => {
                        "Use 3-20 digits, + at the start is fine"
                    }
                    _ => "Type a message first",
                }
                .into(),
            );
            return;
        }
        let (purpose, text) = (s.purpose, s.secret.clone());
        match purpose {
            SheetPurpose::PhoneSearch => {
                self.model.phone.search = text.trim().to_string();
                self.scroll = 0;
                self.model.overlay = None;
            }
            SheetPurpose::PhoneContactName => {
                let number = match &mut self.model.phone.draft {
                    Some(d) => {
                        d.name = text.trim().to_string();
                        d.number.clone()
                    }
                    None => String::new(),
                };
                self.open_text_sheet(SheetPurpose::PhoneContactNumber, "Phone number", &number);
            }
            SheetPurpose::PhoneContactNumber => {
                let d = self.model.phone.draft.take();
                self.model.overlay = None;
                if let (Some(d), true) = (d, self.model.connected) {
                    out.cmd(Cmd::ContactSave {
                        id: d.id,
                        name: d.name,
                        number: secret(&text),
                    });
                }
            }
            SheetPurpose::PhoneNewNumber => {
                self.model.overlay = None;
                self.open_thread(text.clone(), out);
                let title = format!("Message to {}", self.model.phone.label(&text));
                self.open_text_sheet(SheetPurpose::PhoneCompose, &title, "");
            }
            SheetPurpose::PhoneCompose => {
                let number = self.model.phone.thread.clone();
                self.model.overlay = None;
                if let (Some(n), true) = (number, self.model.connected) {
                    out.cmd(Cmd::SmsSend {
                        number: secret(&n),
                        text: secret(&text),
                    });
                }
            }
            _ => {}
        }
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
