//! Phone screen tap handlers and text-sheet submission for `App`.

use super::{App, Cmd, Outcome};
use crate::overlay::{Overlay, SheetPurpose};
use crate::phone_ui::{self, ContactDraft, PhoneSheet, PhoneTarget, PhoneView};
use walpad_proto::Secret;

fn secret(s: &str) -> Secret {
    Secret::new(s)
}

impl App {
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
}
