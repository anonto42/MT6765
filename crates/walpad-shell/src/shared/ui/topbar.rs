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
    // fill only, 0 px radius, no border (owner request 2026-11-12)
    p.cv.rect(pill, surface());
    let cy = pill.y + pill.h / 2;
    for c in &l.chips {
        p.text(
            &c.text,
            Style::new(Face::Regular, TEXT_PX, text()),
            Rect::new(c.rect.x, cy - 15, c.rect.w + 2, 16),
            Align::Left,
        );
        if let Some(f) = c.frac {
            let bar = Rect::new(c.rect.x, cy + 6, c.rect.w, BAR_H);
            p.cv.rect(bar, divider());
            let fw = ((c.rect.w as f32 * f.clamp(0.0, 1.0)).round() as i32).max(1);
            p.cv.rect(
                Rect::new(bar.x, bar.y, fw, BAR_H),
                if c.warn { warning() } else { text() },
            );
        }
    }
}

#[cfg(test)]
mod tests;
