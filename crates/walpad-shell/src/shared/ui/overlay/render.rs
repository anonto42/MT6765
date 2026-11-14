use super::sheet::PasswordSheet;
use super::types::{AppSheet, SheetPurpose, POWER_ITEMS};
use crate::app::Target;
use walpad_proto::LockKind;
use walpad_ui::components::{keyboard_layout, ButtonKind, ButtonState, KeyboardLayer, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::Rect;

const SHEET_W: i32 = 800;
pub const PASSWORD_SHEET_H: i32 = 524;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordGeom {
    pub sheet: Rect,
    pub field: Rect,
    pub keyboard: Rect,
    pub cancel: Rect,
    pub connect: Rect,
}

pub fn password_geom(frame: &Frame) -> PasswordGeom {
    let w = SHEET_W.min(frame.w);
    let sheet = Rect::new(
        (frame.w - w) / 2,
        frame.h - PASSWORD_SHEET_H,
        w,
        PASSWORD_SHEET_H,
    );
    let content = Rect::new(
        sheet.x + SP_5,
        sheet.y + 68,
        sheet.w - 2 * SP_5,
        sheet.h - 68 - SP_5,
    );
    let field = Rect::new(content.x, content.y, content.w, 56);
    let keyboard = Rect::new(sheet.x, field.bottom() + 34, sheet.w, 274);
    let bw = (content.w - SP_3) / 2;
    let by = keyboard.bottom() + SP_3;
    PasswordGeom {
        sheet,
        field,
        keyboard,
        cancel: Rect::new(content.x, by, bw, BUTTON_H),
        connect: Rect::new(content.x + bw + SP_3, by, bw, BUTTON_H),
    }
}

pub fn render_password(
    p: &mut Painter,
    frame: &Frame,
    s: &PasswordSheet,
    spinner_deg: u16,
) -> Vec<(Rect, Target)> {
    let g = password_geom(frame);
    let mut hits = Vec::new();
    p.sheet(g.sheet, &s.ssid);
    let (placeholder, secret_field, confirm) = match s.purpose {
        SheetPurpose::WifiJoin => ("Password", true, "Connect"),
        SheetPurpose::HotspotName => ("Name", false, "Save"),
        SheetPurpose::HotspotPassword => ("8-63 characters", true, "Save"),
        SheetPurpose::MobileApn => ("e.g. gpinternet", false, "Save"),
        SheetPurpose::SimPin => ("4-8 digits", true, "Unlock"),
        SheetPurpose::VpnDns => ("e.g. 1.1.1.1, 9.9.9.9 (empty: automatic)", false, "Save"),
        SheetPurpose::VpnProxy => ("http://host:port (empty: none)", false, "Save"),
        SheetPurpose::LockCurrent(_) => ("Current secret", true, "Next"),
        SheetPurpose::LockRemove(_) => ("Current secret", true, "Remove lock"),
        SheetPurpose::LockNew(LockKind::Pin) => ("4-12 digits", true, "Next"),
        SheetPurpose::LockNew(_) => ("4-64 characters", true, "Next"),
        SheetPurpose::LockRepeat(_) => ("Type it again", true, "Save"),
        SheetPurpose::PhoneSearch => ("Name or number", false, "Search"),
        SheetPurpose::PhoneContactName => ("Name", false, "Next"),
        SheetPurpose::PhoneContactNumber => ("e.g. 01712345678", false, "Save"),
        SheetPurpose::PhoneCompose => ("Message", false, "Send"),
        SheetPurpose::PhoneNewNumber => ("Number to write to", false, "Next"),
        SheetPurpose::WifiManualSsid => ("Network name (SSID)", false, "Next"),
        SheetPurpose::NetIp => ("e.g. 192.168.0.100 (empty: automatic)", false, "Save"),
        SheetPurpose::NetGateway => ("e.g. 192.168.0.1 (empty: automatic)", false, "Save"),
        SheetPurpose::NetMac => ("AA:BB:CC:DD:EE:FF (empty: hardware)", false, "Save"),
        SheetPurpose::NetHostname => ("e.g. walpad (empty: default)", false, "Save"),
        SheetPurpose::WallpaperPath => ("e.g. /root/wallpaper.png (empty: default)", false, "Save"),
        SheetPurpose::ClockTime => ("YYYY-MM-DD HH:MM", false, "Save"),
    };
    if let Some(eye) = p.text_field(
        g.field,
        &s.secret,
        placeholder,
        secret_field,
        s.shown,
        true,
        s.error.as_deref(),
    ) {
        hits.push((eye, Target::Eye));
    }
    let keys = p.keyboard(g.keyboard, s.layer, s.pressed_key);
    for (i, k) in keys.iter().enumerate() {
        hits.push((k.rect, Target::Key(i)));
    }
    p.button(
        g.cancel,
        "Cancel",
        ButtonKind::Secondary,
        ButtonState::Default,
        None,
    );
    let cs = if s.connecting {
        ButtonState::Loading(spinner_deg)
    } else if s.valid() {
        ButtonState::Default
    } else {
        ButtonState::Disabled
    };
    p.button(g.connect, confirm, ButtonKind::Primary, cs, None);
    hits.push((g.cancel, Target::SheetCancel));
    hits.push((g.connect, Target::SheetConnect));
    hits
}

pub fn app_sheet_geom(frame: &Frame, running: bool) -> (Rect, Vec<(Rect, Target)>) {
    let n = if running { 3 } else { 2 };
    let h = 68 + n * (BUTTON_H + SP_3) + SP_4;
    let w = SHEET_W.min(frame.w);
    let sheet = Rect::new((frame.w - w) / 2, frame.h - h, w, h);
    let mut y = sheet.y + 68;
    let mut btns = Vec::new();
    let mut add = |t: Target, y: &mut i32| {
        btns.push((
            Rect::new(sheet.x + SP_5, *y, sheet.w - 2 * SP_5, BUTTON_H),
            t,
        ));
        *y += BUTTON_H + SP_3;
    };
    if running {
        add(Target::CloseApp, &mut y);
    }
    add(Target::PinToggle, &mut y);
    add(Target::SheetCancel, &mut y);
    (sheet, btns)
}

pub fn render_app_sheet(p: &mut Painter, frame: &Frame, s: &AppSheet) -> Vec<(Rect, Target)> {
    let (sheet, btns) = app_sheet_geom(frame, s.running);
    p.sheet(sheet, &s.name);
    for (r, t) in &btns {
        match t {
            Target::CloseApp => p.button(
                *r,
                "Close app",
                ButtonKind::Danger,
                ButtonState::Default,
                None,
            ),
            Target::PinToggle => p.button(
                *r,
                if s.pinned {
                    "Unpin from the sidebar"
                } else {
                    "Pin to the sidebar"
                },
                ButtonKind::Secondary,
                ButtonState::Default,
                None,
            ),
            _ => p.button(
                *r,
                "Cancel",
                ButtonKind::Secondary,
                ButtonState::Default,
                None,
            ),
        }
    }
    btns
}

pub fn render_forget(p: &mut Painter, frame: &Frame, ssid: &str) -> Vec<(Rect, Target)> {
    let screen = Rect::new(0, 0, frame.w, frame.h);
    let (cancel, confirm) = p.dialog(
        screen,
        "Forget network?",
        &format!("{ssid} will be removed from the saved networks."),
        "Cancel",
        "Forget",
        "Forget",
        true,
        false,
    );
    vec![
        (cancel, Target::DialogCancel),
        (confirm, Target::DialogConfirm),
    ]
}

pub fn render_power_confirm(
    p: &mut Painter,
    frame: &Frame,
    poweroff: bool,
    armed: bool,
) -> Vec<(Rect, Target)> {
    let screen = Rect::new(0, 0, frame.w, frame.h);
    let (title, body, label, again) = if poweroff {
        (
            "Shut down?",
            "The tablet will power off. Press the power button to start it again.",
            "Shut down",
            "Tap again to shut down",
        )
    } else {
        (
            "Restart tablet?",
            "The tablet will restart. Open apps will be closed.",
            "Restart",
            "Tap again to restart",
        )
    };
    let (cancel, confirm) = p.dialog(screen, title, body, "Cancel", label, again, true, armed);
    vec![
        (cancel, Target::DialogCancel),
        (confirm, Target::DialogConfirm),
    ]
}

pub fn render_power_menu(
    p: &mut Painter,
    frame: &Frame,
    focus: usize,
    armed: Option<usize>,
) -> Vec<(Rect, Target)> {
    let screen = Rect::new(0, 0, frame.w, frame.h);
    p.power_menu(screen, &POWER_ITEMS, focus, armed)
        .into_iter()
        .enumerate()
        .map(|(i, r)| (r, Target::PowerItem(i)))
        .collect()
}

/// Key index -> key for the current keyboard (same geometry the renderer uses).
pub fn key_at(
    frame: &Frame,
    layer: KeyboardLayer,
    idx: usize,
) -> Option<walpad_ui::components::Key> {
    keyboard_layout(password_geom(frame).keyboard, layer)
        .get(idx)
        .map(|k| k.key)
}
