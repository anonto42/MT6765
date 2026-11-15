//! Header clock and quick-settings status pill rendering for [`Painter`].
use super::{Painter, WifiMark};
use crate::canvas::Rect;
use crate::icons::Icon;
use crate::text::{Align, Face, Style};
use crate::tokens::*;

/// A leading `CHARGING_MARK` in the battery label draws a bolt before the percent (and widens the pill);
/// the mark itself is never drawn as text.
pub const CHARGING_MARK: char = '\u{1}';
/// Width of the charging bolt in the header pill.
pub const BOLT_W: i32 = 14;
/// Width of the Bluetooth mark in the header pill.
pub const BT_MARK_W: i32 = 18;
/// Width of the GPS mark in the header pill.
pub const GPS_MARK_W: i32 = 18;
/// Width of `Painter::cell_bars`.
pub const CELL_BARS_W: i32 = 18;

fn split_charging(label: &str) -> (bool, &str) {
    match label.strip_prefix(CHARGING_MARK) {
        Some(rest) => (true, rest),
        None => (false, label),
    }
}

impl Painter<'_> {
    /// Width of the clock without the date: the time and the AM/PM suffix, as `clock` places them.
    pub fn clock_time_w(&mut self, time: &str, suffix: Option<&str>) -> i32 {
        let tw = self.fonts.measure(time, Face::Regular, SIZE_CLOCK).ceil() as i32;
        let sfx = suffix.map_or(0, |s| {
            let sw = self.fonts.measure(s, Face::Regular, SIZE_CAPTION).ceil() as i32;
            sw + SP_1 + 2
        });
        tw + SP_1 + sfx
    }

    pub fn clock(&mut self, r: Rect, time: &str, suffix: Option<&str>, date: &str) {
        let style = Style::new(Face::Regular, SIZE_CLOCK, text());
        let tw = self.fonts.measure(time, Face::Regular, SIZE_CLOCK).ceil() as i32;
        self.text(time, style, Rect::new(r.x, r.y, tw + 2, r.h), Align::Left);
        let mut x = r.x + tw + SP_1;
        if let Some(sfx) = suffix {
            let sw = self.fonts.measure(sfx, Face::Regular, SIZE_CAPTION).ceil() as i32;
            self.caption(
                sfx,
                Rect::new(x + 2, r.y + 2, sw + 2, r.h),
                Align::Left,
                text_secondary(),
            );
            x += sw + SP_1 + 2;
        }
        let dx = x + SP_3;
        if r.right() - dx > 80 {
            self.caption(
                date,
                Rect::new(dx, r.y + 1, r.right() - dx, r.h),
                Align::Left,
                text_secondary(),
            );
        }
    }

    /// Width of the quick-settings pill for this battery label.
    pub fn quick_pill_w(&mut self, battery: &str) -> i32 {
        self.quick_pill_w_cell(battery, false)
    }

    /// Width with the mobile signal bars (owner request 2026-11-10) when `cell`.
    pub fn quick_pill_w_cell(&mut self, battery: &str, cell: bool) -> i32 {
        self.quick_pill_w_ex(battery, cell, false)
    }

    /// Width with the mobile bars and the Bluetooth mark (shown while Bluetooth is powered).
    pub fn quick_pill_w_ex(&mut self, battery: &str, cell: bool, bt: bool) -> i32 {
        let (charging, battery) = split_charging(battery);
        let bolt_w = if charging { BOLT_W + SP_1 } else { 0 };
        let bw = bolt_w
            + self
                .fonts
                .measure(battery, Face::Regular, SIZE_SECONDARY)
                .ceil() as i32;
        let cell_w = if cell { CELL_BARS_W + SP_2 } else { 0 };
        let cell_w = cell_w + if bt { BT_MARK_W + SP_2 } else { 0 };
        14 + cell_w + 20 + SP_3 + bw + SP_1 + 22 + SP_2 + 16 + 14
    }

    /// Width with the mobile bars, the Bluetooth mark and the GPS mark (while the GPS is on).
    pub fn quick_pill_w_marks(&mut self, battery: &str, cell: bool, bt: bool, gps: bool) -> i32 {
        self.quick_pill_w_ex(battery, cell, bt) + if gps { GPS_MARK_W + SP_2 } else { 0 }
    }

    /// Small mobile signal bars for the header pill: four bars 3 px wide, 16 px high.
    pub fn cell_bars(&mut self, x: i32, y: i32, bars: u8, color: Color) {
        for i in 0..4 {
            let h = 4 + i * 4;
            let lit = (i as u8) < bars;
            self.cv.rect(
                Rect::new(x + i * 5, y + 16 - h, 3, h),
                if lit { color } else { divider() },
            );
        }
    }

    /// The pill at the right of the header: Wi-Fi mark, battery percent + icon, chevron. `right`
    /// is its right edge, `cy` its vertical centre. Returns its rect (the hit area).
    #[allow(clippy::too_many_arguments)]
    pub fn quick_pill(
        &mut self,
        right: i32,
        cy: i32,
        wifi: WifiMark,
        battery: &str,
        low: bool,
        pressed: bool,
        open: bool,
    ) -> Rect {
        self.quick_pill_cell(right, cy, wifi, battery, low, pressed, open, None)
    }

    /// The pill with the mobile signal bars in front (`cell` = (bars 0-4, colour); None = no SIM).
    #[allow(clippy::too_many_arguments)]
    pub fn quick_pill_cell(
        &mut self,
        right: i32,
        cy: i32,
        wifi: WifiMark,
        battery: &str,
        low: bool,
        pressed: bool,
        open: bool,
        cell: Option<(u8, Color)>,
    ) -> Rect {
        self.quick_pill_ex(right, cy, wifi, battery, low, pressed, open, cell, false)
    }

    /// The pill with an optional Bluetooth mark after the mobile bars (`bt` = Bluetooth powered).
    #[allow(clippy::too_many_arguments)]
    pub fn quick_pill_ex(
        &mut self,
        right: i32,
        cy: i32,
        wifi: WifiMark,
        battery: &str,
        low: bool,
        pressed: bool,
        open: bool,
        cell: Option<(u8, Color)>,
        bt: bool,
    ) -> Rect {
        self.quick_pill_marks(right, cy, wifi, battery, low, pressed, open, cell, bt, None)
    }

    /// The pill with the GPS mark after the Bluetooth mark (`gps` = Some(colour) while the GPS is
    /// on: white with a fix, dim while searching).
    #[allow(clippy::too_many_arguments)]
    pub fn quick_pill_marks(
        &mut self,
        right: i32,
        cy: i32,
        wifi: WifiMark,
        battery: &str,
        low: bool,
        pressed: bool,
        open: bool,
        cell: Option<(u8, Color)>,
        bt: bool,
        gps: Option<Color>,
    ) -> Rect {
        let w = self.quick_pill_w_marks(battery, cell.is_some(), bt, gps.is_some());
        let (charging, battery) = split_charging(battery);
        let r = Rect::new(right - w, cy - 20, w, 40);
        self.outline_box(r, 0, pressed, open, false);
        let mut x = r.x + 14;
        if let Some((bars, c)) = cell {
            self.cell_bars(x, cy - 8, bars, c);
            x += CELL_BARS_W + SP_2;
        }
        if bt {
            self.cv.icon(Icon::Bluetooth, x, cy - 9, BT_MARK_W, text());
            x += BT_MARK_W + SP_2;
        }
        if let Some(c) = gps {
            self.cv.icon(Icon::Location, x, cy - 9, GPS_MARK_W, c);
            x += GPS_MARK_W + SP_2;
        }
        let wifi_c = match wifi {
            WifiMark::Connected => text(),
            WifiMark::Failed => danger(),
            WifiMark::Disconnected | WifiMark::Unknown => text_disabled(),
        };
        self.cv.icon(Icon::Wifi, x, cy - 10, 20, wifi_c);
        x += 20 + SP_3;
        let bc = if low { warning() } else { text() };
        if charging {
            self.cv.icon(Icon::Bolt, x, cy - 8, BOLT_W + 2, success());
            x += BOLT_W + SP_1;
        }
        let bw = self
            .fonts
            .measure(battery, Face::Regular, SIZE_SECONDARY)
            .ceil() as i32;
        self.text(
            battery,
            Style::new(Face::Regular, SIZE_SECONDARY, bc),
            Rect::new(x, r.y, bw + 2, r.h),
            Align::Left,
        );
        x += bw + SP_1;
        self.cv.icon(Icon::Battery, x, cy - 11, 22, bc);
        x += 22 + SP_2;
        self.cv
            .icon(Icon::ChevronDown, x, cy - 8, 16, text_secondary());
        r
    }
}
