//! VPN and DNS screen (owner request 2026-11-10, change vpn-and-dns): tunnel switch and status,
//! kill switch with its keep-or-revert confirmation, profile choice, import from the user folders,
//! DNS servers and HTTP proxy. Model helpers are pure; content coordinates as in mobile.rs.
use crate::app::Target;
use crate::network::switch_hit;
use walpad_proto::{Status, VpnState};
use walpad_ui::components::{Accessory, ButtonKind, ButtonState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

mod model;
#[cfg(test)]
mod tests;

use model::vpn;
pub use model::{
    bytes, card_lines, dns_value, editable, is_on, kind_label, layout, proxy_value, revert_in,
    state_chip, switchable, VpnFile, VpnLayout, MAX_FILES,
};
pub struct View<'a> {
    pub st: &'a Option<Status>,
    pub files: &'a [VpnFile],
    pub online: bool,
    pub pressed: Option<Target>,
}

pub fn render(p: &mut Painter, frame: &Frame, v: &View) -> Vec<(Rect, Target)> {
    let l = layout(frame, v.st, v.files.len());
    let mut hits = Vec::new();
    let pr = |t: Target| v.pressed == Some(t);
    let vs = vpn(v.st);
    p.screen_title("VPN and DNS", l.title);
    let (chip, chip_col) = state_chip(v.st);
    p.outline_box(l.chip, 0, false, false, false);
    p.cv.rect(Rect::new(l.chip.x, l.chip.y, 3, l.chip.h), chip_col);
    p.cv.rect(Rect::new(l.chip.x + 12, l.chip.y + 12, 8, 8), chip_col);
    p.text(
        chip,
        Style::new(Face::Regular, SIZE_SECONDARY, text()),
        Rect::new(l.chip.x + 28, l.chip.y, l.chip.w - 36, l.chip.h),
        Align::Left,
    );

    // status card
    let card = l.card;
    p.card(card, false);
    p.cv.icon(Icon::Lock, card.x + SP_4, card.y + SP_4, 28, text());
    let (title, sub, sub_col, detail) = card_lines(v.st);
    let tx = card.x + SP_4 + 28 + SP_4;
    let tw = card.right() - tx - SP_4;
    p.text(
        &title,
        Style::new(Face::Bold, SIZE_BODY, text()),
        Rect::new(tx, card.y + SP_4, tw, 28),
        Align::Left,
    );
    p.text(
        &sub,
        Style::new(Face::Regular, SIZE_SECONDARY, sub_col),
        Rect::new(tx, card.y + SP_4 + 32, tw, 22),
        Align::Left,
    );
    if let Some(d) = detail {
        p.text(
            &d,
            Style::new(Face::Regular, SIZE_CAPTION, text_secondary()),
            Rect::new(tx, card.y + SP_4 + 60, tw, 20),
            Align::Left,
        );
    }

    // keep-or-revert confirmation of the kill switch
    if let (Some(k), Some(b), Some(secs)) = (l.keep, l.keep_btn, revert_in(v.st)) {
        p.card(k, false);
        p.text(
            "Keep the kill switch?",
            Style::new(Face::Bold, SIZE_BODY, warning()),
            Rect::new(k.x + SP_4, k.y + SP_3, k.w - 140 - 3 * SP_4, 28),
            Align::Left,
        );
        p.text(
            &format!("Everything reverts in {secs} s if you do not confirm"),
            Style::new(Face::Regular, SIZE_SECONDARY, text_secondary()),
            Rect::new(k.x + SP_4, k.y + SP_3 + 32, k.w - 140 - 3 * SP_4, 44),
            Align::Left,
        );
        let st = if !v.online {
            ButtonState::Disabled
        } else if pr(Target::VpnKeep) {
            ButtonState::Pressed
        } else {
            ButtonState::Default
        };
        p.button(b, "Keep", ButtonKind::Primary, st, None);
        hits.push((b, Target::VpnKeep));
    }

    // connection
    p.section_label("Connection", l.conn_label);
    p.row_group(Rect::new(
        l.switch_row.x,
        l.switch_row.y,
        l.switch_row.w,
        3 * ROW_H,
    ));
    let on = is_on(v.st);
    let sw_sub = match vs.map(|s| s.state) {
        Some(VpnState::On) => "Connected",
        Some(VpnState::Starting) => "Connecting...",
        Some(VpnState::Stopping) => "Disconnecting...",
        Some(VpnState::Failed) => "Failed",
        _ => "Off",
    };
    if let Some(t) = p.list_row(
        l.switch_row,
        Some(Icon::Globe),
        "VPN",
        Some(sw_sub),
        &Accessory::Toggle(on),
        pr(Target::VpnSwitch),
        !v.online || !switchable(v.st),
    ) {
        hits.push((switch_hit(t), Target::VpnSwitch));
    }
    hits.push((l.switch_row, Target::VpnSwitch));
    let edit = v.online && editable(v.st);
    let ks = vs.is_some_and(|s| s.kill_switch);
    if let Some(t) = p.list_row(
        l.kill_row,
        Some(Icon::Lock),
        "Kill switch",
        Some("Block traffic outside the tunnel"),
        &Accessory::Toggle(ks),
        pr(Target::VpnKill),
        !edit,
    ) {
        hits.push((switch_hit(t), Target::VpnKill));
    }
    hits.push((l.kill_row, Target::VpnKill));
    let prof = vs
        .and_then(|s| s.profile.clone())
        .unwrap_or_else(|| "None".into());
    p.list_row(
        l.profile_row,
        Some(Icon::Settings),
        "Profile",
        None,
        &Accessory::Value(prof),
        pr(Target::VpnProfile),
        !edit || vs.is_none_or(|s| s.profiles.len() < 2),
    );
    hits.push((l.profile_row, Target::VpnProfile));
    let note = if !edit && on {
        "Turn the VPN off to change the profile or the kill switch"
    } else if ks {
        "With the kill switch, confirm within 60 s after connecting or it reverts"
    } else {
        ""
    };
    p.text(
        note,
        Style::new(Face::Regular, SIZE_CAPTION, text_secondary()),
        Rect::new(l.note.x, l.note.y, l.note.w, 40),
        Align::Left,
    );

    // import from the user folders
    p.section_label("Import from Files", l.import_label);
    if !l.import_rows.is_empty() {
        let r0 = l.import_rows[0];
        p.row_group(Rect::new(
            r0.x,
            r0.y,
            r0.w,
            l.import_rows.len() as i32 * ROW_H,
        ));
    }
    for (i, r) in l.import_rows.iter().enumerate() {
        let f = &v.files[i];
        p.list_row(
            *r,
            Some(Icon::Lock),
            &f.name,
            Some(&f.path),
            &Accessory::Action("Import".into()),
            pr(Target::VpnImport(i)),
            !v.online,
        );
        hits.push((*r, Target::VpnImport(i)));
    }
    if let Some(r) = l.import_note {
        p.text(
            "Put a WireGuard .conf or OpenVPN .ovpn in Downloads or Documents",
            Style::new(Face::Regular, SIZE_CAPTION, text_secondary()),
            Rect::new(r.x, r.y, r.w, 40),
            Align::Left,
        );
    }

    // DNS and proxy
    p.section_label("DNS and proxy", l.net_label);
    p.row_group(Rect::new(l.dns_row.x, l.dns_row.y, l.dns_row.w, 2 * ROW_H));
    p.list_row(
        l.dns_row,
        Some(Icon::Globe),
        "DNS servers",
        None,
        &Accessory::Value(dns_value(v.st)),
        pr(Target::VpnDns),
        !v.online,
    );
    hits.push((l.dns_row, Target::VpnDns));
    p.list_row(
        l.proxy_row,
        Some(Icon::Wifi),
        "HTTP proxy",
        None,
        &Accessory::Value(proxy_value(v.st)),
        pr(Target::VpnProxy),
        !v.online,
    );
    hits.push((l.proxy_row, Target::VpnProxy));
    hits
}
