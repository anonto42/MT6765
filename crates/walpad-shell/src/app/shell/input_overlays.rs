use super::Shell;
use crate::app::Cmd;
use crate::clipboard::ClipEvent;
use crate::ime::KbdAction;
use crate::seltools::EditOut;

impl Shell {
    pub fn keyboard_visible(&self) -> bool {
        self.kbd.as_ref().is_some_and(|k| k.visible())
    }

    /// Show/hide the system keyboard by hand (dock button, core command, Home, the mouse pad),
    /// move the dock above it, and (`report`) tell the core, so its `keyboard toggle` (hardware
    /// keys, walpad-ctl) knows the real state. Requests that came from the core are not reported
    /// back.
    pub(super) fn set_keyboard(&mut self, on: bool, report: bool) {
        self.focus.manual(on);
        self.show_keyboard(on, report);
    }

    /// What the focus state machine decided (it already knows).
    pub(super) fn apply_focus(&mut self, a: Option<KbdAction>) {
        match a {
            Some(KbdAction::Show) => self.show_keyboard(true, true),
            Some(KbdAction::Hide) => self.show_keyboard(false, true),
            None => {}
        }
    }

    fn show_keyboard(&mut self, on: bool, report: bool) {
        // the keyboard and the mouse pad share the bottom of the screen: one replaces the other
        if on {
            self.set_pad(false);
        }
        let Some(mut k) = self.kbd.take() else {
            return;
        };
        match k.set_visible(on, &mut self.fonts) {
            Ok(()) => self.kbd = Some(k),
            Err(e) => eprintln!("walpad-shell: system keyboard disabled: {}", e.0),
        }
        self.bottom_changed();
        if report {
            self.report_keyboard();
        }
    }

    pub fn mouse_pad_visible(&self) -> bool {
        self.pad.as_ref().is_some_and(|p| p.visible())
    }

    /// Show/hide the mouse pad; showing it puts the keyboard away (and tells the core).
    pub(super) fn set_pad(&mut self, on: bool) {
        if on && self.keyboard_visible() {
            self.set_keyboard(false, true);
        }
        let Some(mut p) = self.pad.take() else {
            return;
        };
        match p.set_visible(on, &mut self.fonts) {
            Ok(()) => self.pad = Some(p),
            Err(e) => eprintln!("walpad-shell: mouse pad disabled: {}", e.0),
        }
        self.bottom_changed();
    }

    /// The keyboard or the mouse pad was shown, hidden or rotated: the dock sits directly above
    /// whichever is up.
    pub(super) fn bottom_changed(&mut self) {
        let kbd_h = match (&self.kbd, &self.pad) {
            (Some(k), _) if k.visible() => k.height(),
            (_, Some(p)) if p.visible() => p.height(),
            _ => 0,
        };
        if let Some(d) = &mut self.dock {
            if let Err(e) = d.set_keyboard_height(kbd_h) {
                eprintln!("walpad-shell: system dock: {}", e.0);
            }
        }
        if let Some(mut e) = self.edit.take() {
            match e.set_bottom(kbd_h, &mut self.fonts) {
                Ok(()) => self.edit = Some(e),
                Err(err) => eprintln!("walpad-shell: selection toolbar disabled: {}", err.0),
            }
        }
    }

    pub(super) fn show_clipboard_list(&mut self) {
        let entries: Vec<String> = self
            .keeper
            .as_ref()
            .map(|k| k.history().iter().cloned().collect())
            .unwrap_or_default();
        if let Some(mut e) = self.edit.take() {
            match e.show_list(&entries, &mut self.fonts) {
                Ok(()) => self.edit = Some(e),
                Err(err) => eprintln!("walpad-shell: selection toolbar disabled: {}", err.0),
            }
        }
    }

    /// Keeper and toolbar traffic; then the toolbar's visibility follows the rule.
    pub(super) fn step_edit(&mut self) {
        let now = self.now_ms();
        let mut history_changed = false;
        if let Some(mut k) = self.keeper.take() {
            match k.poll() {
                Ok(evs) => {
                    for ev in evs {
                        match ev {
                            ClipEvent::PrimaryChanged(true) => self.bar.selection(now),
                            ClipEvent::PrimaryChanged(false) => self.bar.clear_selection(),
                            ClipEvent::HistoryChanged => history_changed = true,
                        }
                    }
                    self.keeper = Some(k);
                }
                Err(e) => eprintln!("walpad-shell: clipboard keeper disabled: {}", e.0),
            }
        }
        let Some(mut e) = self.edit.take() else {
            return;
        };
        let mut ok = true;
        match e.poll(&mut self.fonts) {
            Ok(outs) => {
                for o in outs {
                    match o {
                        EditOut::CopyPrimary => {
                            if let Some(k) = &mut self.keeper {
                                if let Err(err) = k.copy_primary() {
                                    eprintln!("walpad-shell: clipboard: {}", err.0);
                                }
                            }
                        }
                        EditOut::Paste(i) => {
                            let pasted = match &mut self.keeper {
                                Some(k) => k.paste(i).unwrap_or(false),
                                None => false,
                            };
                            if pasted {
                                if let Err(err) = e.send_paste() {
                                    eprintln!("walpad-shell: paste: {}", err.0);
                                }
                            }
                        }
                        EditOut::ClearHistory => {
                            if let Some(k) = &mut self.keeper {
                                if let Err(err) = k.clear() {
                                    eprintln!("walpad-shell: clipboard: {}", err.0);
                                }
                            }
                        }
                        EditOut::Acted => self.bar.clear_selection(),
                    }
                }
            }
            Err(err) => {
                eprintln!("walpad-shell: selection toolbar disabled: {}", err.0);
                ok = false;
            }
        }
        if ok {
            if history_changed {
                let entries: Vec<String> = self
                    .keeper
                    .as_ref()
                    .map(|k| k.history().iter().cloned().collect())
                    .unwrap_or_default();
                ok = e.update_list(&entries, &mut self.fonts).is_ok();
            }
            self.bar.home = e.home();
            self.bar.dock_out = self.dock.as_ref().is_some_and(|d| d.expanded());
            let want = self.bar.visible(now);
            ok = ok && e.set_wanted(want, &mut self.fonts).is_ok();
        }
        if ok {
            self.edit = Some(e);
        } else {
            eprintln!("walpad-shell: selection toolbar disabled");
        }
    }

    pub(super) fn report_keyboard(&mut self) {
        let c = if self.keyboard_visible() {
            Cmd::KeyboardShow
        } else {
            Cmd::KeyboardHide
        };
        let (name, args) = c.request();
        self.core.send(name, args);
    }
}
