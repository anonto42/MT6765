//! VPN and DNS screen (owner request 2026-11-10, change vpn-and-dns): tunnel switch and status,
//! kill switch with its keep-or-revert confirmation, profile choice, import from the user folders,
//! DNS servers and HTTP proxy. Model helpers are pure; content coordinates as in mobile.rs.
use crate::app::Target;
use crate::network::switch_hit;
use walpad_proto::{Status, VpnKind, VpnState, VpnStatus};
use walpad_ui::components::{Accessory, ButtonKind, ButtonState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Icon, Rect, Style};

const TITLE_H: i32 = 40;
const CARD_H: i32 = 112;
const LABEL_H: i32 = 24;
const KEEP_H: i32 = 96;
const NOTE_H: i32 = 52;
/// Import candidates shown at most.
pub const MAX_FILES: usize = 5;

/// An importable file the core found (name, path).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VpnFile {
    pub name: String,
    pub path: String,
}

fn vpn(st: &Option<Status>) -> Option<&VpnStatus> {
    st.as_ref().map(|s| &s.vpn)
}

/// The tunnel switch shows "on" while it runs or starts.
pub fn is_on(st: &Option<Status>) -> bool {
    vpn(st).is_some_and(|v| matches!(v.state, VpnState::On | VpnState::Starting))
}

/// The switch works with a selected profile (or to turn a running tunnel off), not mid-change.
pub fn switchable(st: &Option<Status>) -> bool {
    vpn(st).is_some_and(|v| {
        !matches!(v.state, VpnState::Stopping) && (v.profile.is_some() || is_on(st))
    })
}

/// Profile and kill switch cannot change while the tunnel runs.
pub fn editable(st: &Option<Status>) -> bool {
    vpn(st).is_some_and(|v| matches!(v.state, VpnState::Off | VpnState::Failed))
}

/// Seconds left to confirm the kill switch (None: nothing to confirm).
pub fn revert_in(st: &Option<Status>) -> Option<u32> {
    vpn(st).and_then(|v| v.revert_in_s)
}

pub fn state_chip(st: &Option<Status>) -> (&'static str, Color) {
    match vpn(st).map(|v| v.state) {
        None => ("Unknown", text_disabled()),
        Some(VpnState::Off) => ("Off", text_secondary()),
        Some(VpnState::Starting) => ("Connecting", warning()),
        Some(VpnState::On) => ("Connected", success()),
        Some(VpnState::Stopping) => ("Stopping", warning()),
        Some(VpnState::Failed) => ("Failed", danger()),
    }
}

pub fn kind_label(k: VpnKind) -> &'static str {
    match k {
        VpnKind::Wireguard => "WireGuard",
        VpnKind::Openvpn => "OpenVPN",
    }
}

/// "1.2 MB" style size.
pub fn bytes(n: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let (mut v, mut i) = (n as f64, 0);
    while v >= 1024.0 && i < 3 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

/// Status card: (title, subtitle, subtitle colour, detail line).
pub fn card_lines(st: &Option<Status>) -> (String, String, Color, Option<String>) {
    let Some(v) = vpn(st) else {
        return (
            "Unknown".into(),
            "No status from the system".into(),
            text_disabled(),
            None,
        );
    };
    let name = v.profile.clone().unwrap_or_else(|| "No profile".into());
    let kind = v.kind.map(kind_label);
    let title = match kind {
        Some(k) if v.profile.is_some() => format!("{name} · {k}"),
        _ => name,
    };
    let (sub, col) = match v.state {
        VpnState::Off if v.profile.is_none() => {
            ("Import a profile to start".to_string(), text_secondary())
        }
        VpnState::Off => ("Not connected".into(), text_secondary()),
        VpnState::Starting => ("Connecting...".into(), warning()),
        VpnState::On => ("Connected".into(), success()),
        VpnState::Stopping => ("Disconnecting...".into(), warning()),
        VpnState::Failed => (
            format!("Failed: {}", v.reason.as_deref().unwrap_or("unknown error")),
            danger(),
        ),
    };
    let detail = (v.state == VpnState::On).then(|| {
        let mut parts = Vec::new();
        if let Some(h) = v.handshake_age_s {
            parts.push(format!("Handshake {h} s ago"));
        }
        if let (Some(rx), Some(tx)) = (v.rx_bytes, v.tx_bytes) {
            parts.push(format!("down {} · up {}", bytes(rx), bytes(tx)));
        }
        parts.join(" · ")
    });
    (title, sub, col, detail.filter(|d| !d.is_empty()))
}

/// Row values for DNS and proxy.
pub fn dns_value(st: &Option<Status>) -> String {
    match vpn(st) {
        Some(v) if !v.dns.is_empty() => v.dns.join(", "),
        _ => "Automatic".into(),
    }
}

pub fn proxy_value(st: &Option<Status>) -> String {
    vpn(st)
        .and_then(|v| v.proxy.clone())
        .unwrap_or_else(|| "None".into())
}

#[derive(Debug, Clone, PartialEq)]
pub struct VpnLayout {
    pub title: Rect,
    pub chip: Rect,
    pub card: Rect,
    pub keep: Option<Rect>,
    pub keep_btn: Option<Rect>,
    pub conn_label: Rect,
    pub switch_row: Rect,
    pub kill_row: Rect,
    pub profile_row: Rect,
    pub note: Rect,
    pub import_label: Rect,
    pub import_rows: Vec<Rect>,
    pub import_note: Option<Rect>,
    pub net_label: Rect,
    pub dns_row: Rect,
    pub proxy_row: Rect,
    pub total: i32,
}

pub fn layout(frame: &Frame, st: &Option<Status>, files: usize) -> VpnLayout {
    let c = frame.padded();
    let mut y = SP_4;
    let chip = Rect::new(c.right() - 140, y + 4, 140, 32);
    let title = Rect::new(c.x, y, chip.x - SP_3 - c.x, TITLE_H);
    y += TITLE_H + SP_4;
    let card = Rect::new(c.x, y, c.w, CARD_H);
    y += CARD_H + SP_3;
    let (keep, keep_btn) = if revert_in(st).is_some() {
        let k = Rect::new(c.x, y, c.w, KEEP_H);
        let b = Rect::new(
            k.right() - SP_4 - 140,
            k.y + (KEEP_H - BUTTON_H) / 2,
            140,
            BUTTON_H,
        );
        y += KEEP_H + SP_3;
        (Some(k), Some(b))
    } else {
        (None, None)
    };
    y += SP_3;
    let conn_label = Rect::new(c.x, y, c.w, LABEL_H);
    y += LABEL_H + SP_2;
    let switch_row = Rect::new(c.x, y, c.w, ROW_H);
    let kill_row = Rect::new(c.x, y + ROW_H, c.w, ROW_H);
    let profile_row = Rect::new(c.x, y + 2 * ROW_H, c.w, ROW_H);
    y += 3 * ROW_H + SP_3;
    let note = Rect::new(c.x + SP_1, y, c.w - SP_1, NOTE_H);
    y += NOTE_H + SP_4;
    let import_label = Rect::new(c.x, y, c.w, LABEL_H);
    y += LABEL_H + SP_2;
    let n = files.min(MAX_FILES);
    let import_rows: Vec<Rect> = (0..n)
        .map(|i| Rect::new(c.x, y + i as i32 * ROW_H, c.w, ROW_H))
        .collect();
    y += n as i32 * ROW_H;
    let import_note = (n == 0).then(|| {
        let r = Rect::new(c.x + SP_1, y, c.w - SP_1, NOTE_H);
        y += NOTE_H;
        r
    });
    y += SP_6;
    let net_label = Rect::new(c.x, y, c.w, LABEL_H);
    y += LABEL_H + SP_2;
    let dns_row = Rect::new(c.x, y, c.w, ROW_H);
    let proxy_row = Rect::new(c.x, y + ROW_H, c.w, ROW_H);
    y += 2 * ROW_H;
    VpnLayout {
        title,
        chip,
        card,
        keep,
        keep_btn,
        conn_label,
        switch_row,
        kill_row,
        profile_row,
        note,
        import_label,
        import_rows,
        import_note,
        net_label,
        dns_row,
        proxy_row,
        total: y + SP_6,
    }
}

/// Everything the screen draws from.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn st(f: impl FnOnce(&mut VpnStatus)) -> Option<Status> {
        let mut s = crate::app::tests::status(120, &[]);
        f(&mut s.vpn);
        Some(s)
    }
    fn home(v: &mut VpnStatus) {
        v.profile = Some("home".into());
        v.kind = Some(VpnKind::Wireguard);
        v.profiles = vec![
            walpad_proto::VpnProfile {
                name: "home".into(),
                kind: VpnKind::Wireguard,
            },
            walpad_proto::VpnProfile {
                name: "work".into(),
                kind: VpnKind::Openvpn,
            },
        ];
    }

    #[test]
    fn lines_for_every_state() {
        assert_eq!(card_lines(&None).0, "Unknown");
        let none = st(|_| {});
        assert_eq!(card_lines(&none).1, "Import a profile to start");
        assert!(!switchable(&none));
        let off = st(home);
        assert_eq!(card_lines(&off).0, "home · WireGuard");
        assert!(switchable(&off) && editable(&off) && !is_on(&off));
        let on = st(|v| {
            home(v);
            v.state = VpnState::On;
            v.handshake_age_s = Some(12);
            v.rx_bytes = Some(2048);
            v.tx_bytes = Some(10);
        });
        assert!(is_on(&on) && !editable(&on));
        assert_eq!(
            card_lines(&on).3.as_deref(),
            Some("Handshake 12 s ago · down 2.0 KB · up 10 B")
        );
        let failed = st(|v| {
            home(v);
            v.state = VpnState::Failed;
            v.reason = Some("no handshake".into());
        });
        assert_eq!(card_lines(&failed).1, "Failed: no handshake");
        assert!(editable(&failed));
        assert_eq!(state_chip(&failed).0, "Failed");
        let mid = st(|v| {
            home(v);
            v.state = VpnState::Stopping;
        });
        assert!(!switchable(&mid));
        assert_eq!(dns_value(&off), "Automatic");
        assert_eq!(proxy_value(&off), "None");
        let set = st(|v| {
            v.dns = vec!["1.1.1.1".into(), "9.9.9.9".into()];
            v.proxy = Some("http://10.0.0.2:3128".into());
        });
        assert_eq!(dns_value(&set), "1.1.1.1, 9.9.9.9");
        assert_eq!(proxy_value(&set), "http://10.0.0.2:3128");
        assert_eq!(bytes(1_500_000), "1.4 MB");
    }

    #[test]
    fn layout_rows_are_touchable_and_do_not_overlap() {
        for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
            for (s, n) in [
                (st(home), 0),
                (st(home), 3),
                (
                    st(|v| {
                        home(v);
                        v.state = VpnState::On;
                        v.revert_in_s = Some(40);
                    }),
                    9,
                ),
            ] {
                let l = layout(&f, &s, n);
                let mut rs = vec![
                    l.title,
                    l.chip,
                    l.card,
                    l.conn_label,
                    l.switch_row,
                    l.kill_row,
                    l.profile_row,
                    l.note,
                    l.import_label,
                    l.net_label,
                    l.dns_row,
                    l.proxy_row,
                ];
                rs.extend(l.keep);
                rs.extend(l.import_note);
                rs.extend(l.import_rows.iter().copied());
                for r in [
                    l.switch_row,
                    l.kill_row,
                    l.profile_row,
                    l.dns_row,
                    l.proxy_row,
                ] {
                    assert!(r.h >= TOUCH_MIN);
                }
                if let Some(b) = l.keep_btn {
                    assert!(b.h >= TOUCH_MIN);
                }
                for (i, a) in rs.iter().enumerate() {
                    for b in &rs[i + 1..] {
                        let o = a.x < b.right()
                            && b.x < a.right()
                            && a.y < b.bottom()
                            && b.y < a.bottom();
                        assert!(!o, "{a:?} overlaps {b:?}");
                    }
                }
                assert!(l.import_rows.len() <= MAX_FILES);
                assert!(l.total > l.proxy_row.bottom());
            }
        }
        assert!(layout(&Frame::new(800, 1280), &st(home), 0).keep.is_none());
    }
}
