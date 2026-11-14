//! Settings subpage row builders (Input, Security, System, Clock, TopBar, Maintenance, About, Logs).

use super::pages_power_net::{network_rows, power_rows};
use super::{fmt_bytes, fmt_uptime, telnet_scope, Ctx, Page, Row};
use crate::app::Target;
use crate::topbar::StatItem;
use walpad_ui::components::Tone;

pub(super) fn opt(v: Option<String>) -> (String, Tone) {
    v.map(|s| (s, Tone::Info))
        .unwrap_or_else(|| ("Unknown".into(), Tone::Warning))
}

pub(super) fn info(label: &str, (val, tone): (String, Tone)) -> Row {
    Row::Info(label.into(), val, tone)
}

pub fn page_rows(page: Page, c: &Ctx) -> Vec<Row> {
    match page {
        // Sound and other dedicated screens are built by the app
        Page::Display
        | Page::Mobile
        | Page::Bluetooth
        | Page::Gps
        | Page::Sound
        | Page::Apps
        | Page::Vpn => Vec::new(),
        Page::Input => input_rows(c),
        Page::Power => power_rows(c),
        Page::Network => network_rows(c),
        Page::Security => security_rows(c),
        Page::System => system_rows(c),
        Page::Clock => vec![
            Row::Toggle("24-hour clock", c.clock_24h, Target::ClockToggle),
            Row::Action {
                label: "Set date & time manually",
                target: Target::ClockSetTime,
                danger: false,
            },
        ],
        Page::TopBar => {
            let it = c.status.as_ref().map(|s| s.stats.items).unwrap_or_default();
            StatItem::ALL
                .into_iter()
                .map(|i| Row::Toggle(i.name(), i.on(&it), Target::Stat(i)))
                .collect()
        }
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

fn input_rows(c: &Ctx) -> Vec<Row> {
    let mut r = vec![
        Row::Toggle(
            "Touch scroll & gestures",
            c.devd.touch_gestures,
            Target::TouchGesturesToggle,
        ),
        Row::Toggle(
            "Two-finger pinch zoom",
            c.devd.multitouch,
            Target::MultitouchToggle,
        ),
    ];
    for (i, (_, label, def_act)) in crate::config::KEY_SLOTS.iter().enumerate() {
        let val = c
            .sys
            .gestures
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, a)| a.clone())
            .unwrap_or_else(|| crate::config::pretty_action(def_act));
        r.push(Row::Choice((*label).into(), val, Target::KeyGesture(i)));
    }
    r
}

fn security_rows(c: &Ctx) -> Vec<Row> {
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

fn system_rows(c: &Ctx) -> Vec<Row> {
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
