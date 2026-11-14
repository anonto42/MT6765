use super::*;
use std::path::Path;

fn fonts() -> Fonts {
    Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts")).unwrap()
}

fn st(items: StatsItems) -> StatsStatus {
    StatsStatus {
        items,
        cpu_pct: Some(100),
        ram_used_kb: Some(1_258_291),
        ram_total_kb: Some(2_097_152),
        rom_used_b: Some(12 * 1024 * 1024 * 1024),
        rom_total_b: Some(54 * 1024 * 1024 * 1024),
        net_down_bps: Some(1_258_291),
        net_up_bps: Some(30 * 1024),
    }
}
const ALL: StatsItems = StatsItems {
    cpu: true,
    ram: true,
    rom: true,
    net: true,
};

#[test]
fn rates_and_sizes_are_compact() {
    assert_eq!(fmt_rate(0), "0B");
    assert_eq!(fmt_rate(999), "999B");
    assert_eq!(fmt_rate(1500), "1.5K");
    assert_eq!(fmt_rate(30 * 1024), "30K");
    assert_eq!(fmt_rate(1_258_291), "1.2M");
    assert_eq!(fmt_rate(50 * 1024 * 1024), "50M");
    assert_eq!(fmt_gb(2 * 1024 * 1024 * 1024), "2.0G");
    assert_eq!(fmt_gb(54 * 1024 * 1024 * 1024), "54G");
    assert_eq!(fmt_gb(300 * 1024 * 1024), "300M");
}

#[test]
fn chip_texts_and_unavailable_figures() {
    let s = st(ALL);
    assert_eq!(chip_content(StatItem::Cpu, &s, false).0, "CPU 100%");
    assert_eq!(chip_content(StatItem::Ram, &s, false).0, "RAM 1.2/2.0G");
    assert_eq!(chip_content(StatItem::Ram, &s, true).0, "RAM 60%");
    assert_eq!(chip_content(StatItem::Rom, &s, false).0, "ROM 12/54G");
    assert_eq!(
        chip_content(StatItem::Net, &s, false).0,
        "\u{2193}1.2M \u{2191}30K"
    );
    let none = StatsStatus {
        cpu_pct: None,
        net_up_bps: None,
        ram_total_kb: None,
        ..s
    };
    assert_eq!(
        chip_content(StatItem::Cpu, &none, false),
        ("CPU --".into(), None)
    );
    assert_eq!(chip_content(StatItem::Ram, &none, false).0, "RAM --");
    assert_eq!(
        chip_content(StatItem::Net, &none, false).0,
        "\u{2193}1.2M \u{2191}--"
    );
}

#[test]
fn nothing_is_drawn_before_the_first_sample_or_with_everything_off() {
    let mut f = fonts();
    let h = Rect::new(96, 0, 704, 64);
    for s in [
        StatsStatus::default(),
        st(StatsItems {
            cpu: false,
            ram: false,
            rom: false,
            net: false,
        }),
    ] {
        let l = layout(&mut f, &s, h, 200, 150, 500);
        assert!(l.pill.is_none() && l.chips.is_empty());
    }
}

/// The header never overlaps: the chips stay between the clock (and its date while shown) and
/// the pill, in both orientations, for every subset of items and every pill width.
#[test]
fn chips_never_overlap_the_clock_or_the_pill() {
    let mut f = fonts();
    for (w, tag) in [(800, "portrait"), (1280, "landscape")] {
        let h = Rect::new(96, 0, w - 96, 64);
        for mask in 1u8..16 {
            let items = StatsItems {
                cpu: mask & 1 != 0,
                ram: mask & 2 != 0,
                rom: mask & 4 != 0,
                net: mask & 8 != 0,
            };
            // the narrowest pill and the widest one (cell bars, Bluetooth and GPS marks)
            for pill_w in [150, 250, 330] {
                for (clock_end, date_w) in [(h.x + SP_5 + 62, 150), (h.x + SP_5 + 96, 200)] {
                    let right = h.right() - SP_4 - pill_w - SP_3;
                    let l = layout(&mut f, &st(items), h, clock_end, date_w, right);
                    let msg = format!("{tag} {mask} pill {pill_w} {l:?}");
                    if let Some(p) = l.pill {
                        assert!(p.right() <= right, "{msg}");
                        let date_end = clock_end + SP_3 + date_w.max(DATE_MIN);
                        let floor = if l.date { date_end } else { clock_end };
                        assert!(p.x >= floor, "{msg}");
                        assert!(p.y >= h.y && p.bottom() <= h.bottom(), "{msg}");
                        let mut prev = p.x;
                        for c in &l.chips {
                            assert!(c.rect.x >= prev && c.rect.right() <= p.right(), "{msg}");
                            prev = c.rect.right();
                        }
                        assert!(!l.chips.is_empty());
                    } else {
                        assert!(l.chips.is_empty(), "{msg}");
                    }
                }
            }
        }
    }
    // plenty of room: the default pair keeps the date and both chips
    let h = Rect::new(96, 0, 1184, 64);
    let l = layout(&mut f, &st(StatsItems::default()), h, 200, 150, 900);
    assert!(l.date && l.chips.len() == 2);
    // tight portrait with every item: shorter, never overlapping (checked above); the
    // default pair always fits next to the date in landscape
    let l = layout(&mut f, &st(ALL), h, 200, 150, 900);
    assert!(l.date && l.chips.len() == 4);
}

#[test]
fn degrades_in_order_date_then_short_text_then_chips() {
    let mut f = fonts();
    let h = Rect::new(96, 0, 704, 64);
    let s = st(ALL);
    let (clock_end, date_w) = (96 + SP_5 + 62, 150);
    // 0 date kept, 1 date dropped, 2 short texts, 3 fewer chips, 4 none
    let rank = |l: &Layout| {
        let short = l
            .chips
            .iter()
            .any(|c| c.text.starts_with("RAM") && c.text.contains('%'));
        match (l.date, l.chips.len(), short) {
            (true, 4, false) => 0,
            (false, 4, false) => 1,
            (false, 4, true) => 2,
            (false, 1..=3, _) => 3,
            _ => 4,
        }
    };
    let mut ranks = Vec::new();
    for right in (250..900).rev() {
        ranks.push(rank(&layout(&mut f, &s, h, clock_end, date_w, right)));
    }
    // as the room shrinks it only ever gives up more
    assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "{ranks:?}");
    assert_eq!((ranks[0], *ranks.last().unwrap()), (0, 4));
    assert!(ranks.contains(&1) && ranks.contains(&2));
}
