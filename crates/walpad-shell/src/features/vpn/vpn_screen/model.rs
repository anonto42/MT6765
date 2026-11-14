use walpad_proto::{Status, VpnKind, VpnState, VpnStatus};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Color, Rect};

pub(super) const TITLE_H: i32 = 40;
pub(super) const CARD_H: i32 = 112;
pub(super) const LABEL_H: i32 = 24;
pub(super) const KEEP_H: i32 = 96;
pub(super) const NOTE_H: i32 = 52;
/// Import candidates shown at most.
pub const MAX_FILES: usize = 5;

/// An importable file the core found (name, path).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VpnFile {
    pub name: String,
    pub path: String,
}

pub(super) fn vpn(st: &Option<Status>) -> Option<&VpnStatus> {
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
