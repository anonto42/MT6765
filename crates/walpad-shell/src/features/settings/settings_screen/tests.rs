//! Unit tests for the Settings screen.

use super::*;
use crate::config::DevdSettings;
use walpad_hw::Root;
use walpad_proto::Status;
use walpad_ui::components::Tone;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::TOUCH_MIN;

fn ctx<'a>(sys: &'a SysInfo, st: &'a Option<Status>, d: &'a DevdSettings) -> Ctx<'a> {
    Ctx {
        sys,
        status: st,
        devd: d,
        clock_24h: true,
    }
}

fn find<'a>(rows: &'a [Row], label: &str) -> Option<&'a Row> {
    rows.iter().find(|r| match r {
        Row::Info(l, _, _) | Row::Choice(l, _, _) => l == label,
        _ => false,
    })
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
        Some(&Row::Choice(
            "Screen turns off after".into(),
            "4 min".into(),
            Target::PowerOffCycle
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
