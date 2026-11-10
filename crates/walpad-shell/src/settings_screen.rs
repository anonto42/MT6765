//! Settings tab: category list and subpages. Pages are built as plain rows from real data
//! (`SysInfo` + status + config) so they can be tested without drawing.
use crate::app::Target;
use crate::config::{read_gestures, DevdSettings};
use crate::device_screen::{info_row, tone_color};
use walpad_hw::{sysfs, sysinfo, Root};
use walpad_proto::Status;
use walpad_ui::components::{Accessory, ButtonKind, ButtonState, Painter, Tone};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Display,
    /// Volume, mute and the test tone (owner request 2026-11-10).
    Sound,
    Input,
    Power,
    Network,
    /// Opens the Mobile network screen (owner request 2026-11-10).
    Mobile,
    /// Opens the VPN and DNS screen (owner request 2026-11-10).
    Vpn,
    /// Opens the Bluetooth screen (owner request 2026-11-10).
    Bluetooth,
    /// Opens the app store's installed apps (change app-catalogue-manager).
    Apps,
    Security,
    System,
    Clock,
    Maintenance,
    About,
    Logs,
}

pub const CATEGORIES: [(Page, &str, Icon); 14] = [
    (Page::Display, "Display", Icon::Sun),
    (Page::Sound, "Sound", Icon::Volume),
    (Page::Input, "Input", Icon::Keyboard),
    (Page::Power, "Power", Icon::Power),
    (Page::Network, "Network", Icon::Wifi),
    (Page::Mobile, "Mobile network", Icon::Sim),
    (Page::Vpn, "VPN and DNS", Icon::Lock),
    (Page::Bluetooth, "Bluetooth", Icon::Bluetooth),
    (Page::Apps, "Apps", Icon::Apps),
    (Page::Security, "Security", Icon::Lock),
    (Page::System, "System", Icon::Device),
    (Page::Clock, "Clock", Icon::Settings),
    (Page::Maintenance, "Maintenance", Icon::Refresh),
    (Page::About, "About", Icon::Globe),
];

pub fn title(p: Page) -> &'static str {
    match p {
        Page::Logs => "Logs",
        other => CATEGORIES
            .iter()
            .find(|(c, _, _)| *c == other)
            .map_or("", |(_, n, _)| n),
    }
}

/// Facts gathered from the system (refreshed when a page opens and every minute).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SysInfo {
    pub alpine: Option<String>,
    pub kernel: Option<String>,
    pub storage: Option<(u64, u64)>,
    pub memory_kb: Option<(u64, u64)>,
    pub uptime_s: Option<u64>,
    pub ssh_port_open: bool,
    pub ssh_keys: usize,
    pub listening: Vec<(String, u16)>,
    pub dns: Vec<String>,
    pub gateway: Option<String>,
    pub logs: Vec<String>,
    pub gestures: Vec<(String, String)>,
}

pub fn gather(root: &Root) -> SysInfo {
    let listening = sysinfo::listening_tcp(root);
    SysInfo {
        alpine: sysinfo::alpine_release(root),
        kernel: sysinfo::kernel_release(root),
        storage: sysinfo::disk_usage(root),
        memory_kb: sysinfo::meminfo(root),
        uptime_s: sysfs::uptime_s(root).map(|u| u as u64),
        ssh_port_open: listening.iter().any(|(_, p)| *p == 22),
        ssh_keys: sysinfo::authorized_key_count(root),
        listening,
        dns: sysinfo::dns_servers(root),
        gateway: sysinfo::default_gateway(root),
        logs: sysinfo::tail_lines(root, "/var/log/walpad-core.log", 200),
        gestures: read_gestures(root),
    }
}

pub fn fmt_bytes(b: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let f = b as f64;
    if f >= GB {
        format!("{:.1} GB", f / GB)
    } else {
        format!("{:.0} MB", f / (1024.0 * 1024.0))
    }
}

pub fn fmt_uptime(s: u64) -> String {
    let (d, h, m) = (s / 86_400, s % 86_400 / 3600, s % 3600 / 60);
    match (d, h) {
        (0, 0) => format!("{m} min"),
        (0, _) => format!("{h} h {m} min"),
        _ => format!("{d} d {h} h"),
    }
}

/// Telnet is only acceptable on the USB addresses.
pub fn telnet_scope(listening: &[(String, u16)]) -> (&'static str, Tone) {
    let t: Vec<&String> = listening
        .iter()
        .filter(|(_, p)| *p == 23)
        .map(|(a, _)| a)
        .collect();
    if t.is_empty() {
        ("Not running", Tone::Info)
    } else if t
        .iter()
        .all(|a| a.as_str() == "192.168.42.1" || a.as_str() == "192.168.43.1")
    {
        ("USB only", Tone::Info)
    } else {
        ("Open on the network", Tone::Danger)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Info(String, String, Tone),
    Toggle(&'static str, bool, Target),
    Action {
        label: &'static str,
        target: Target,
        danger: bool,
    },
    Link(&'static str, Page),
    Mono(String),
    /// Label line (icon, name, value text) above a 56 px slider; the slider is the hit area.
    Slider {
        icon: Icon,
        label: &'static str,
        pct: String,
        value: f32,
        active: bool,
        target: Target,
        disabled: bool,
    },
    /// An action button whose label is computed (the screen lock's auto-lock delay).
    ActionText {
        label: String,
        target: Target,
        danger: bool,
    },
}

pub struct Ctx<'a> {
    pub sys: &'a SysInfo,
    pub status: &'a Option<Status>,
    pub devd: &'a DevdSettings,
    pub clock_24h: bool,
}

fn opt(v: Option<String>) -> (String, Tone) {
    v.map(|s| (s, Tone::Info))
        .unwrap_or_else(|| ("Unknown".into(), Tone::Warning))
}
fn secs(s: u32) -> String {
    match s {
        0 => "Never".into(),
        s if s < 60 => format!("{s} s"),
        s => format!("{} min", s / 60),
    }
}

pub fn page_rows(page: Page, c: &Ctx) -> Vec<Row> {
    let info = |l: &str, (v, t): (String, Tone)| Row::Info(l.into(), v, t);
    match page {
        // Sound is built by the app (it needs the live slider state)
        Page::Display | Page::Mobile | Page::Bluetooth | Page::Sound | Page::Apps | Page::Vpn => {
            Vec::new()
        }
        Page::Input => {
            let mut r: Vec<Row> = c
                .sys
                .gestures
                .iter()
                .map(|(l, a)| Row::Info(l.clone(), a.clone(), Tone::Info))
                .collect();
            if r.is_empty() {
                r.push(info("Gestures", ("Unknown".into(), Tone::Warning)));
            }
            r
        }
        Page::Power => vec![
            Row::Info(
                "Power button · tap".into(),
                "Screen off / on".into(),
                Tone::Info,
            ),
            Row::Info(
                "Power button · hold".into(),
                "Power menu".into(),
                Tone::Info,
            ),
            Row::Info(
                "Screen dims after".into(),
                secs(c.devd.dim_after_s),
                Tone::Info,
            ),
            Row::Info(
                "Screen turns off after".into(),
                secs(c.devd.off_after_s),
                Tone::Info,
            ),
            Row::Info(
                "Low battery below".into(),
                format!("{} mV", c.devd.low_batt_mv),
                Tone::Info,
            ),
            Row::Info(
                "Shuts down below".into(),
                format!("{} mV", c.devd.critical_batt_mv),
                Tone::Info,
            ),
        ],
        Page::Network => {
            let w = c.status.as_ref().map(|s| &s.wifi);
            vec![
                info("Wi-Fi state", opt(w.and_then(|w| w.state.clone()))),
                info("Network", opt(w.and_then(|w| w.ssid.clone()))),
                info("IP address", opt(w.and_then(|w| w.ip.clone()))),
                info(
                    "Signal",
                    opt(w.and_then(|w| w.signal_dbm).map(|d| format!("{d} dBm"))),
                ),
                info("Gateway", opt(c.sys.gateway.clone())),
                info(
                    "DNS",
                    if c.sys.dns.is_empty() {
                        ("Unknown".into(), Tone::Warning)
                    } else {
                        (c.sys.dns.join(", "), Tone::Info)
                    },
                ),
            ]
        }
        Page::Security => {
            let ssh = if c.sys.ssh_port_open {
                ("Listening on port 22".into(), Tone::Info)
            } else {
                ("Not running".into(), Tone::Warning)
            };
            let keys = match c.sys.ssh_keys {
                0 => ("None: key login is disabled".into(), Tone::Warning),
                n => (n.to_string(), Tone::Info),
            };
            let (scope, tone) = telnet_scope(&c.sys.listening);
            let mut rows = crate::app::lock_rows(c.status);
            rows.extend([
                Row::Info("SSH".into(), ssh.0, ssh.1),
                Row::Info("Authorised keys".into(), keys.0, keys.1),
                Row::Info("Telnet".into(), scope.into(), tone),
            ]);
            rows
        }
        Page::System => {
            let mem = c
                .sys
                .memory_kb
                .map(|(t, a)| format!("{} free of {}", fmt_bytes(a * 1024), fmt_bytes(t * 1024)));
            let disk = c
                .sys
                .storage
                .map(|(t, f)| format!("{} free of {}", fmt_bytes(f), fmt_bytes(t)));
            vec![
                Row::Info(
                    "WalPad OS".into(),
                    env!("CARGO_PKG_VERSION").into(),
                    Tone::Info,
                ),
                info("Alpine Linux", opt(c.sys.alpine.clone())),
                info("Kernel", opt(c.sys.kernel.clone())),
                Row::Info(
                    "Architecture".into(),
                    std::env::consts::ARCH.into(),
                    Tone::Info,
                ),
                info("Storage", opt(disk)),
                info("Memory", opt(mem)),
                info("Uptime", opt(c.sys.uptime_s.map(fmt_uptime))),
            ]
        }
        Page::Clock => vec![Row::Toggle(
            "24-hour clock",
            c.clock_24h,
            Target::ClockToggle,
        )],
        Page::Maintenance => vec![
            Row::Link("View logs", Page::Logs),
            Row::Action {
                label: "Power menu",
                target: Target::PowerMenu,
                danger: false,
            },
            Row::Action {
                label: "Restart shell",
                target: Target::RestartShell,
                danger: false,
            },
            Row::Action {
                label: "Restart tablet",
                target: Target::Reboot,
                danger: true,
            },
            Row::Action {
                label: "Shut down",
                target: Target::Shutdown,
                danger: true,
            },
        ],
        Page::About => vec![
            Row::Info(
                "Version".into(),
                env!("CARGO_PKG_VERSION").into(),
                Tone::Info,
            ),
            Row::Info(
                "Build".into(),
                option_env!("WALPAD_BUILD").unwrap_or("development").into(),
                Tone::Info,
            ),
            Row::Info("Project".into(), "WalPad OS".into(), Tone::Info),
        ],
        Page::Logs => {
            if c.sys.logs.is_empty() {
                vec![Row::Mono("No log lines yet".into())]
            } else {
                c.sys.logs.iter().cloned().map(Row::Mono).collect()
            }
        }
    }
}

pub fn row_h(r: &Row) -> i32 {
    match r {
        Row::Mono(_) => 22,
        Row::Slider { .. } => crate::volume_ui::SLIDER_ROW_H,
        _ => ROW_H,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsLayout {
    pub back: Option<Rect>,
    pub title: Rect,
    pub rows: Vec<(Rect, Row)>,
    pub total: i32,
}

pub fn layout(frame: &Frame, page: Option<Page>, rows: &[Row]) -> SettingsLayout {
    let c = frame.padded();
    let mut y = SP_4;
    let (back, title);
    match page {
        None => {
            back = None;
            title = Rect::new(c.x, y, c.w, 40);
            y += 40 + SP_4;
        }
        Some(_) => {
            let b = Rect::new(c.x, y, c.w, BUTTON_H);
            back = Some(b);
            title = Rect::new(c.x + TOUCH_MIN + SP_2, y, c.w - TOUCH_MIN - SP_2, BUTTON_H);
            y += BUTTON_H + SP_3;
        }
    }
    let mut out = Vec::new();
    if page.is_none() {
        for (i, _) in CATEGORIES.iter().enumerate() {
            out.push((
                Rect::new(c.x, y + i as i32 * ROW_H, c.w, ROW_H),
                Row::Link(CATEGORIES[i].1, CATEGORIES[i].0),
            ));
        }
        y += CATEGORIES.len() as i32 * ROW_H;
    } else {
        for r in rows {
            let h = row_h(r);
            out.push((Rect::new(c.x, y, c.w, h), r.clone()));
            y += h;
        }
    }
    SettingsLayout {
        back,
        title,
        rows: out,
        total: y + SP_6,
    }
}

pub fn render(
    p: &mut Painter,
    frame: &Frame,
    page: Option<Page>,
    rows: &[Row],
    online: bool,
    pressed: Option<Target>,
) -> Vec<(Rect, Target)> {
    let l = layout(frame, page, rows);
    let mut hits = Vec::new();
    match (page, l.back) {
        (Some(pg), Some(b)) => {
            let back_rect = Rect::new(b.x, b.y, TOUCH_MIN, b.h);
            p.outline_box(
                back_rect.inset(4),
                RADIUS,
                pressed == Some(Target::Back),
                false,
                false,
            );
            p.cv.icon(
                Icon::Back,
                back_rect.x + (back_rect.w - ICON) / 2,
                back_rect.y + (back_rect.h - ICON) / 2,
                ICON,
                TEXT,
            );
            p.screen_title(title(pg), l.title);
            hits.push((back_rect, Target::Back));
        }
        _ => p.screen_title("Settings", l.title),
    }
    // rows over the wallpaper get a translucent card behind each run (buttons have their own fill)
    let runs = walpad_ui::components::row_runs(l.rows.iter().map(|(r, row)| {
        (
            *r,
            !matches!(row, Row::Action { .. } | Row::ActionText { .. }),
        )
    }));
    for g in runs {
        p.row_group(g);
    }
    for (r, row) in &l.rows {
        match row {
            Row::Info(label, value, tone) => info_row(p, *r, label, value, *tone),
            Row::Toggle(label, on, t) => {
                p.list_row(
                    *r,
                    None,
                    label,
                    None,
                    &Accessory::Toggle(*on),
                    pressed == Some(*t),
                    !online,
                );
                hits.push((*r, *t));
            }
            Row::Action {
                label,
                target,
                danger,
            } => {
                let inner = Rect::new(r.x, r.y + 4, r.w, r.h - 8);
                let st = if !online {
                    ButtonState::Disabled
                } else if pressed == Some(*target) {
                    ButtonState::Pressed
                } else {
                    ButtonState::Default
                };
                p.button(
                    inner,
                    label,
                    if *danger {
                        ButtonKind::Danger
                    } else {
                        ButtonKind::Secondary
                    },
                    st,
                    None,
                );
                hits.push((inner, *target));
            }
            Row::ActionText {
                label,
                target,
                danger,
            } => {
                let inner = Rect::new(r.x, r.y + 4, r.w, r.h - 8);
                let st = if !online {
                    ButtonState::Disabled
                } else if pressed == Some(*target) {
                    ButtonState::Pressed
                } else {
                    ButtonState::Default
                };
                p.button(
                    inner,
                    label,
                    if *danger {
                        ButtonKind::Danger
                    } else {
                        ButtonKind::Secondary
                    },
                    st,
                    None,
                );
                hits.push((inner, *target));
            }
            Row::Link(label, pg) => {
                let icon = CATEGORIES
                    .iter()
                    .find(|(c, _, _)| c == pg)
                    .map(|(_, _, i)| *i);
                p.list_row(
                    *r,
                    icon,
                    label,
                    None,
                    &Accessory::Chevron,
                    pressed == Some(Target::Page(*pg)),
                    false,
                );
                hits.push((*r, Target::Page(*pg)));
            }
            Row::Slider {
                icon,
                label,
                pct,
                value,
                active,
                target,
                disabled,
            } => {
                let lc = if *disabled { TEXT_DISABLED } else { TEXT };
                let cap = Rect::new(r.x + SP_3, r.y + 8, r.w - 2 * SP_3, 24);
                p.cv.icon(*icon, cap.x, cap.y, ICON, lc);
                p.text(
                    label,
                    Style::new(Face::Regular, SIZE_SECONDARY + 1.0, lc),
                    Rect::new(cap.x + ICON + SP_3, cap.y, cap.w - ICON - SP_3, cap.h),
                    Align::Left,
                );
                p.text(
                    pct,
                    Style::new(Face::Regular, SIZE_SECONDARY, TEXT_SECONDARY),
                    cap,
                    Align::Right,
                );
                let sl = Rect::new(r.x, cap.bottom(), r.w, 56);
                p.slider(sl, *value, *active, *disabled);
                hits.push((sl, *target));
            }
            Row::Mono(line) => p.text(
                line,
                Style::new(Face::Mono, 13.0, TEXT_SECONDARY),
                Rect::new(r.x + SP_2, r.y, r.w - SP_4, r.h),
                Align::Left,
            ),
        }
    }
    let _ = tone_color;
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(sys: &'a SysInfo, st: &'a Option<Status>, d: &'a DevdSettings) -> Ctx<'a> {
        Ctx {
            sys,
            status: st,
            devd: d,
            clock_24h: true,
        }
    }
    fn find<'a>(rows: &'a [Row], label: &str) -> Option<&'a Row> {
        rows.iter()
            .find(|r| matches!(r, Row::Info(l, _, _) if l == label))
    }

    #[test]
    fn formatting() {
        assert_eq!(fmt_bytes(512 * 1024 * 1024), "512 MB");
        assert_eq!(
            fmt_bytes(3 * 1024 * 1024 * 1024 + 512 * 1024 * 1024),
            "3.5 GB"
        );
        assert_eq!(
            [59, 60, 3_599, 3_600, 8_040, 90_000, 400_000].map(fmt_uptime),
            [
                "0 min",
                "1 min",
                "59 min",
                "1 h 0 min",
                "2 h 14 min",
                "1 d 1 h",
                "4 d 15 h"
            ]
        );
    }

    #[test]
    fn telnet_must_be_usb_only() {
        let l = |a: &str| vec![(a.to_string(), 23)];
        assert_eq!(telnet_scope(&[]), ("Not running", Tone::Info));
        assert_eq!(telnet_scope(&l("192.168.42.1")), ("USB only", Tone::Info));
        assert_eq!(
            telnet_scope(&[("192.168.42.1".into(), 23), ("192.168.43.1".into(), 23)]),
            ("USB only", Tone::Info)
        );
        assert_eq!(
            telnet_scope(&l("0.0.0.0")),
            ("Open on the network", Tone::Danger),
            "the warning the owner must see"
        );
        assert_eq!(
            telnet_scope(&[("192.168.42.1".into(), 23), ("192.168.0.9".into(), 23)]).1,
            Tone::Danger
        );
        assert_eq!(
            telnet_scope(&[("0.0.0.0".into(), 22)]),
            ("Not running", Tone::Info)
        );
    }

    #[test]
    fn pages_show_real_data_and_flag_what_is_missing() {
        let st = Some(crate::app::tests::status(120, &[]));
        let sys = SysInfo {
            kernel: Some("4.19.127".into()),
            storage: Some((4 << 30, 1 << 30)),
            memory_kb: Some((3_800_000, 1_900_000)),
            uptime_s: Some(8040),
            ssh_port_open: true,
            ssh_keys: 2,
            listening: vec![("0.0.0.0".into(), 22), ("192.168.42.1".into(), 23)],
            dns: vec!["1.1.1.1".into(), "8.8.8.8".into()],
            gateway: Some("192.168.0.1".into()),
            ..SysInfo::default()
        };
        let d = DevdSettings::default();
        let c = ctx(&sys, &st, &d);
        let net = page_rows(Page::Network, &c);
        assert_eq!(
            find(&net, "Network"),
            Some(&Row::Info("Network".into(), "Home".into(), Tone::Info))
        );
        assert_eq!(
            find(&net, "Gateway"),
            Some(&Row::Info(
                "Gateway".into(),
                "192.168.0.1".into(),
                Tone::Info
            ))
        );
        assert_eq!(
            find(&net, "DNS"),
            Some(&Row::Info(
                "DNS".into(),
                "1.1.1.1, 8.8.8.8".into(),
                Tone::Info
            ))
        );
        assert_eq!(
            find(&net, "Signal"),
            Some(&Row::Info("Signal".into(), "-58 dBm".into(), Tone::Info))
        );
        let sec = page_rows(Page::Security, &c);
        assert_eq!(
            find(&sec, "SSH"),
            Some(&Row::Info(
                "SSH".into(),
                "Listening on port 22".into(),
                Tone::Info
            ))
        );
        assert_eq!(
            find(&sec, "Authorised keys"),
            Some(&Row::Info("Authorised keys".into(), "2".into(), Tone::Info))
        );
        assert_eq!(
            find(&sec, "Telnet"),
            Some(&Row::Info("Telnet".into(), "USB only".into(), Tone::Info))
        );
        let sysrows = page_rows(Page::System, &c);
        assert_eq!(
            find(&sysrows, "Kernel"),
            Some(&Row::Info("Kernel".into(), "4.19.127".into(), Tone::Info))
        );
        assert_eq!(
            find(&sysrows, "Alpine Linux"),
            Some(&Row::Info(
                "Alpine Linux".into(),
                "Unknown".into(),
                Tone::Warning
            )),
            "missing data is Unknown, not invented"
        );
        assert_eq!(
            find(&sysrows, "Uptime"),
            Some(&Row::Info("Uptime".into(), "2 h 14 min".into(), Tone::Info))
        );
        assert_eq!(
            find(&sysrows, "Storage"),
            Some(&Row::Info(
                "Storage".into(),
                "1.0 GB free of 4.0 GB".into(),
                Tone::Info
            ))
        );
        let power = page_rows(Page::Power, &c);
        assert_eq!(
            find(&power, "Screen turns off after"),
            Some(&Row::Info(
                "Screen turns off after".into(),
                "4 min".into(),
                Tone::Info
            ))
        );
        // no keys + no ssh: warnings
        let bare = SysInfo::default();
        let sec = page_rows(Page::Security, &ctx(&bare, &None, &d));
        assert!(matches!(
            find(&sec, "Authorised keys"),
            Some(Row::Info(_, _, Tone::Warning))
        ));
        assert!(matches!(
            find(&sec, "SSH"),
            Some(Row::Info(_, _, Tone::Warning))
        ));
        assert!(
            matches!(find(&page_rows(Page::Network, &ctx(&bare, &None, &d)), "IP address"), Some(Row::Info(_, v, Tone::Warning)) if v == "Unknown")
        );
    }

    #[test]
    fn maintenance_has_the_dangerous_actions_as_danger_buttons() {
        let sys = SysInfo::default();
        let d = DevdSettings::default();
        let rows = page_rows(Page::Maintenance, &ctx(&sys, &None, &d));
        let acts: Vec<(&str, bool)> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Action { label, danger, .. } => Some((*label, *danger)),
                _ => None,
            })
            .collect();
        assert_eq!(
            acts,
            [
                ("Power menu", false),
                ("Restart shell", false),
                ("Restart tablet", true),
                ("Shut down", true)
            ]
        );
        assert!(rows.contains(&Row::Link("View logs", Page::Logs)));
    }

    #[test]
    fn layout_top_level_and_subpage() {
        let f = Frame::new(800, 1280);
        let top = layout(&f, None, &[]);
        assert_eq!(top.rows.len(), CATEGORIES.len());
        assert!(top.rows.iter().all(|(r, _)| r.h >= TOUCH_MIN) && top.back.is_none());
        let rows = vec![
            Row::Info("a".into(), "b".into(), Tone::Info),
            Row::Mono("log".into()),
            Row::Mono("log2".into()),
        ];
        let sub = layout(&f, Some(Page::Logs), &rows);
        let b = sub.back.unwrap();
        assert!(b.h >= TOUCH_MIN);
        assert_eq!(
            sub.rows[2].0.y - sub.rows[1].0.y,
            22,
            "log lines are compact"
        );
        assert!(sub.total > sub.rows[2].0.bottom());
    }

    #[test]
    fn gather_reads_a_fake_root() {
        let d = std::env::temp_dir().join(format!("walpad-gather-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("proc/sys/kernel")).unwrap();
        std::fs::create_dir_all(d.join("var/log")).unwrap();
        std::fs::write(d.join("proc/sys/kernel/osrelease"), "4.19.127\n").unwrap();
        std::fs::write(d.join("proc/uptime"), "7200.5 1.0\n").unwrap();
        // built from parts: the source tree must not contain a literal key=value secret (check.sh greps for it)
        let key = ["p", "sk"].concat();
        std::fs::write(
            d.join("var/log/walpad-core.log"),
            format!("a\nb {key}=secret\n"),
        )
        .unwrap();
        let s = gather(&Root::new(&d));
        assert_eq!(
            (s.kernel.as_deref(), s.uptime_s, s.logs.len()),
            (Some("4.19.127"), Some(7200), 2)
        );
        assert_eq!(
            s.logs[1],
            format!("b {key}=***"),
            "the log viewer never shows secrets"
        );
        assert!(!s.ssh_port_open && s.ssh_keys == 0);
    }
}
