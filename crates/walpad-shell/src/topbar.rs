//! Live system figures in the header row (owner request 2026-11-11, change `top-bar-stats`): CPU,
//! RAM, storage and network throughput as small chips (label and value, a mini bar under the
//! value) in one translucent pill between the clock and the Wi-Fi/battery pill. Which chips show
//! is the owner's choice (Settings > Top bar items). The layout is pure (text widths in, rects
//! out) and degrades before anything could overlap: the date goes first, then the long texts
//! become short ones, then the last chips drop out.
use walpad_proto::{StatsItems, StatsStatus};
use walpad_ui::components::Painter;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Fonts, Rect, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatItem {
    Cpu,
    Ram,
    Rom,
    Net,
}

impl StatItem {
    pub const ALL: [StatItem; 4] = [StatItem::Cpu, StatItem::Ram, StatItem::Rom, StatItem::Net];

    pub fn name(self) -> &'static str {
        match self {
            StatItem::Cpu => "CPU use",
            StatItem::Ram => "Memory (RAM)",
            StatItem::Rom => "Storage (ROM)",
            StatItem::Net => "Network speed",
        }
    }

    /// The `stats_config` argument name.
    pub fn key(self) -> &'static str {
        match self {
            StatItem::Cpu => "cpu",
            StatItem::Ram => "ram",
            StatItem::Rom => "rom",
            StatItem::Net => "net",
        }
    }

    pub fn on(self, it: &StatsItems) -> bool {
        match self {
            StatItem::Cpu => it.cpu,
            StatItem::Ram => it.ram,
            StatItem::Rom => it.rom,
            StatItem::Net => it.net,
        }
    }
}

/// Height of the chip pill (the same as the quick-settings pill).
const PILL_H: i32 = 40;
const PAD: i32 = 18;
const GAP: i32 = SP_4;
const BAR_H: i32 = 3;
const TEXT_PX: f32 = SIZE_LABEL;
/// The clock keeps its date only with this much room (see `Painter::clock`).
const DATE_MIN: i32 = 81;

/// One figure as drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Chip {
    pub item: StatItem,
    pub text: String,
    /// 0..=1 for the mini bar (none for the network).
    pub frac: Option<f32>,
    /// Near the limit: the bar is drawn in the warning colour.
    pub warn: bool,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    /// The date still fits beside the clock.
    pub date: bool,
    /// The pill behind the chips (None: no chips).
    pub pill: Option<Rect>,
    pub chips: Vec<Chip>,
}

/// Bytes with one decimal below 10 and none above: 1.2G, 54G, 512M.
fn fmt_gb(bytes: u64) -> String {
    const G: f64 = 1024.0 * 1024.0 * 1024.0;
    const M: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= G {
        let g = b / G;
        if g < 10.0 {
            format!("{g:.1}G")
        } else {
            format!("{g:.0}G")
        }
    } else {
        format!("{:.0}M", b / M)
    }
}

/// A rate as 512B, 1.5K, 15K, 1.2M (per second, the unit is not drawn: the arrows say enough).
pub fn fmt_rate(bps: u64) -> String {
    const K: f64 = 1024.0;
    let b = bps as f64;
    if bps < 1000 {
        format!("{bps}B")
    } else if b < 10.0 * K {
        format!("{:.1}K", b / K)
    } else if b < 1000.0 * K {
        format!("{:.0}K", b / K)
    } else if b < 10.0 * K * K {
        format!("{:.1}M", b / (K * K))
    } else if b < 1000.0 * K * K {
        format!("{:.0}M", b / (K * K))
    } else {
        format!("{:.1}G", b / (K * K * K))
    }
}

fn pct(used: Option<u64>, total: Option<u64>) -> Option<u32> {
    let (u, t) = (used?, total.filter(|t| *t > 0)?);
    Some(((u.min(t) * 100 + t / 2) / t) as u32)
}

/// What a chip says and how full its bar is. `short`: the percentage instead of used/total.
pub fn chip_content(item: StatItem, st: &StatsStatus, short: bool) -> (String, Option<f32>) {
    match item {
        StatItem::Cpu => match st.cpu_pct {
            Some(p) => (format!("CPU {p}%"), Some(f32::from(p) / 100.0)),
            None => ("CPU --".into(), None),
        },
        StatItem::Ram => {
            let (u, t) = (
                st.ram_used_kb.map(|k| k * 1024),
                st.ram_total_kb.map(|k| k * 1024),
            );
            match (pct(u, t), u, t) {
                (Some(p), Some(u), Some(t)) => (
                    if short {
                        format!("RAM {p}%")
                    } else {
                        format!("RAM {}/{}", fmt_gb(u).trim_end_matches('G'), fmt_gb(t))
                    },
                    Some(p as f32 / 100.0),
                ),
                _ => ("RAM --".into(), None),
            }
        }
        StatItem::Rom => match (
            pct(st.rom_used_b, st.rom_total_b),
            st.rom_used_b,
            st.rom_total_b,
        ) {
            (Some(p), Some(u), Some(t)) => (
                if short {
                    format!("ROM {p}%")
                } else {
                    format!("ROM {}/{}", fmt_gb(u).trim_end_matches('G'), fmt_gb(t))
                },
                Some(p as f32 / 100.0),
            ),
            _ => ("ROM --".into(), None),
        },
        StatItem::Net => {
            let r = |v: Option<u64>| v.map_or("--".to_string(), fmt_rate);
            (
                format!(
                    "\u{2193}{} \u{2191}{}",
                    r(st.net_down_bps),
                    r(st.net_up_bps)
                ),
                None,
            )
        }
    }
}

/// The chips are drawn only once the core has sampled something (before that there is nothing to
/// show, and a header full of dashes would only flicker at start-up).
pub fn has_figures(st: &StatsStatus) -> bool {
    st.items.any()
        && (st.cpu_pct.is_some()
            || st.ram_used_kb.is_some()
            || st.rom_used_b.is_some()
            || st.net_down_bps.is_some()
            || st.net_up_bps.is_some())
}

fn width(fonts: &mut Fonts, s: &str) -> i32 {
    fonts.measure(s, Face::Regular, TEXT_PX).ceil() as i32
}

/// Lay the chips out right-aligned against `right` (the quick-settings pill's left edge minus its
/// gap), vertically centred on the header row `h`. `clock_end`: where the time (and AM/PM) ends;
/// `date_w`: the date's width. Everything stays right of `clock_end` and, while `date` is true,
/// right of the date as well.
pub fn layout(
    fonts: &mut Fonts,
    st: &StatsStatus,
    h: Rect,
    clock_end: i32,
    date_w: i32,
    right: i32,
) -> Layout {
    if !has_figures(st) {
        return Layout {
            date: true,
            ..Layout::default()
        };
    }
    let items: Vec<StatItem> = StatItem::ALL
        .into_iter()
        .filter(|i| i.on(&st.items))
        .collect();
    let cy = h.y + h.h / 2;
    // (date kept, short texts)
    for (date, short) in [(true, false), (false, false), (false, true)] {
        let left = clock_end + SP_3 + if date { date_w.max(DATE_MIN) + SP_3 } else { 0 };
        let texts: Vec<(StatItem, String, Option<f32>)> = items
            .iter()
            .map(|i| {
                let (t, f) = chip_content(*i, st, short);
                (*i, t, f)
            })
            .collect();
        let widths: Vec<i32> = texts.iter().map(|(_, t, _)| width(fonts, t)).collect();
        // drop trailing chips only at the last tier
        let max_drop = if short { texts.len() } else { 0 };
        for keep in (texts.len() - max_drop.min(texts.len() - 1)..=texts.len()).rev() {
            let total: i32 = widths[..keep].iter().sum::<i32>() + GAP * (keep as i32 - 1) + 2 * PAD;
            if right - total < left {
                continue;
            }
            let mut x = right - total + PAD;
            let chips = texts[..keep]
                .iter()
                .zip(&widths)
                .map(|((item, text, frac), w)| {
                    let rect = Rect::new(x, cy - PILL_H / 2, *w, PILL_H);
                    x += w + GAP;
                    Chip {
                        item: *item,
                        text: text.clone(),
                        frac: *frac,
                        warn: frac.is_some_and(|f| f >= 0.9),
                        rect,
                    }
                })
                .collect();
            return Layout {
                date,
                pill: Some(Rect::new(right - total, cy - PILL_H / 2, total, PILL_H)),
                chips,
            };
        }
    }
    // not even one chip fits: nothing is drawn and the clock lays itself out as before
    Layout {
        date: true,
        ..Layout::default()
    }
}

/// Draw the pill and its chips.
pub fn paint(p: &mut Painter, l: &Layout) {
    let Some(pill) = l.pill else { return };
    p.cv.rrect(pill, PILL_H / 2, SURFACE);
    p.cv.rrect_stroke(pill, PILL_H / 2, 1.0, DIVIDER);
    let cy = pill.y + pill.h / 2;
    for c in &l.chips {
        p.text(
            &c.text,
            Style::new(Face::Regular, TEXT_PX, TEXT),
            Rect::new(c.rect.x, cy - 15, c.rect.w + 2, 16),
            Align::Left,
        );
        if let Some(f) = c.frac {
            let bar = Rect::new(c.rect.x, cy + 6, c.rect.w, BAR_H);
            p.cv.rrect(bar, 1, DIVIDER);
            let fw = ((c.rect.w as f32 * f.clamp(0.0, 1.0)).round() as i32).max(1);
            p.cv.rrect(
                Rect::new(bar.x, bar.y, fw, BAR_H),
                1,
                if c.warn { WARNING } else { TEXT },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fonts() -> Fonts {
        Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"))
            .unwrap()
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
}
