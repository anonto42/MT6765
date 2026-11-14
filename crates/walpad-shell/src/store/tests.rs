use super::layout::{layout, Item};
use super::*;
use serde_json::json;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::TOUCH_MIN;

pub fn sample() -> StoreModel {
    let mut m = StoreModel::default();
    m.apply_catalogue(&json!({
        "entries": [
            {"id":"browser","name":"Browser","category":"Built in","size_kb":224000,"installed":true,"system":true,"roles":["browser"]},
            {"id":"mousepad","name":"Mousepad","category":"Documents","summary":"Simple text editor","size_kb":700,"installed":true,"system":false,"roles":[]},
            {"id":"evince","name":"Evince","category":"Documents","summary":"PDF viewer","size_kb":15000,"installed":true,"system":false,"roles":["pdf"]},
            {"id":"zathura","name":"Zathura","category":"Documents","summary":"Light PDF viewer","size_kb":1500,"installed":false,"system":false,"roles":["pdf"]},
            {"id":"mpv","name":"mpv","category":"Media","summary":"Video player","size_kb":37600,"installed":false,"system":false,"roles":["video"]}
        ],
        "free_kb": 5242880, "reserve_kb": 2097152, "busy": null,
        "defaults": {"browser":"browser","pdf":"evince","video":null}
    }));
    m
}

#[test]
fn sizes_are_readable() {
    assert_eq!(fmt_size(700), "700 KB");
    assert_eq!(fmt_size(1536), "1.5 MB");
    assert_eq!(fmt_size(37600), "37 MB");
    assert_eq!(fmt_size(3 * 1024 * 1024), "3.0 GB");
}

#[test]
fn catalogue_reply_is_parsed() {
    let m = sample();
    assert!(m.loaded && m.entries.len() == 5 && m.busy.is_none());
    assert_eq!(m.free_kb, Some(5242880));
    assert_eq!(m.defaults[6].as_deref(), Some("evince"), "pdf");
    assert_eq!(m.defaults[3], None, "video has none");
    assert_eq!(m.name_of("mpv"), "mpv");
}

#[test]
fn space_rule_matches_the_core() {
    let mut m = sample();
    let big = StoreEntry {
        size_kb: 3 * 1024 * 1024,
        ..m.entries[4].clone()
    };
    m.free_kb = Some(4 * 1024 * 1024);
    let msg = m.space_problem(&big).unwrap();
    assert!(
        msg.contains("Not enough free space") && msg.contains("3.0 GB") && msg.contains("4.0 GB"),
        "{msg}"
    );
    m.free_kb = Some(6 * 1024 * 1024);
    assert!(m.space_problem(&big).is_none());
    assert!(m.space_problem(&m.entries[4].clone()).is_none());
}

#[test]
fn layouts_do_not_overlap_and_targets_are_touchable() {
    for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
        for view in [
            StoreView::Installed,
            StoreView::Browse,
            StoreView::Role(6),
            StoreView::Role(3),
        ] {
            let mut m = sample();
            m.view = view;
            m.error = Some("Not enough free space".into());
            m.busy = Some(Busy {
                op: "install".into(),
                id: "mpv".into(),
                percent: 40,
                text: "Installing".into(),
            });
            let l = layout(&f, &m);
            for (i, (a, ia)) in l.items.iter().enumerate() {
                if matches!(
                    ia,
                    Item::Row {
                        target: Some(_),
                        ..
                    } | Item::Button(..)
                ) {
                    assert!(a.h >= TOUCH_MIN, "{a:?}");
                }
                for (b, _) in &l.items[i + 1..] {
                    assert!(a.bottom() <= b.y, "{a:?} overlaps {b:?}");
                }
            }
            assert!(l.total > l.items.last().map_or(0, |(r, _)| r.bottom()));
        }
    }
}
