//! Settings > Top bar items and the header stats (owner request 2026-11-11).
use super::*;
use crate::topbar::StatItem;
use walpad_proto::{StatsItems, StatsStatus};

fn set_stats(app: &mut App, st: StatsStatus) -> Outcome {
    let mut s = app.model.status.clone().unwrap();
    s.stats = st;
    let o = app.on_core(&CoreEvent::Status(Box::new(s)), 1500);
    render(app);
    o
}

fn toggles(app: &App) -> Vec<(&'static str, bool, StatItem)> {
    app.settings_rows()
        .iter()
        .filter_map(|r| match r {
            settings_screen::Row::Toggle(l, on, Target::Stat(i)) => Some((*l, *on, *i)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_page_lists_four_switches_that_follow_the_status() {
    let mut app = online();
    go(&mut app, Tab::Settings);
    tap(&mut app, Target::Page(Page::TopBar));
    assert_eq!(app.model.settings_page, Some(Page::TopBar));
    // defaults: CPU and RAM on, storage and network off
    assert_eq!(
        toggles(&app),
        vec![
            ("CPU use", true, StatItem::Cpu),
            ("Memory (RAM)", true, StatItem::Ram),
            ("Storage (ROM)", false, StatItem::Rom),
            ("Network speed", false, StatItem::Net),
        ]
    );
    set_stats(
        &mut app,
        StatsStatus {
            items: StatsItems {
                cpu: false,
                ram: true,
                rom: true,
                net: true,
            },
            ..Default::default()
        },
    );
    let on: Vec<bool> = toggles(&app).iter().map(|t| t.1).collect();
    assert_eq!(on, [false, true, true, true]);
}

#[test]
fn a_switch_sends_stats_config_with_the_opposite_value() {
    let mut app = online();
    go(&mut app, Tab::Settings);
    tap(&mut app, Target::Page(Page::TopBar));
    let out = tap(&mut app, Target::Stat(StatItem::Net));
    assert_eq!(out.cmds, vec![Cmd::Stats(StatItem::Net, true)]);
    assert_eq!(
        out.cmds[0].request(),
        ("stats_config", json!({"net": true}))
    );
    let out = tap(&mut app, Target::Stat(StatItem::Cpu));
    assert_eq!(out.cmds, vec![Cmd::Stats(StatItem::Cpu, false)]);
    assert_eq!(out.cmds[0].purpose(), Purpose::Setting);
}

#[test]
fn a_stats_only_change_redraws_the_header() {
    let mut app = online();
    let cpu = |p| StatsStatus {
        cpu_pct: Some(p),
        ..Default::default()
    };
    assert!(set_stats(&mut app, cpu(10)).dirty);
    assert!(set_stats(&mut app, cpu(11)).dirty);
    // the same figures again: nothing to redraw
    assert!(!set_stats(&mut app, cpu(11)).dirty);
}
