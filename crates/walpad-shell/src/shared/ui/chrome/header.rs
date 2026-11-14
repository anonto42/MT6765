use crate::app::{Model, Target};
use crate::panels::{battery_label, wifi_mark};
use crate::topbar;
use walpad_ui::components::Painter;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Face, Icon, Rect};

/// Width reserved for the notification bell icon box, left of the Quick Settings pill (owner
/// request 2026-11-12, change `device-notifications`).
const BELL_W: i32 = 40;

/// Header: clock (12/24 h as configured) and date at the left, the quick-settings pill at the
/// right. Returns `(quick settings pill, bell icon box)`.
pub fn render_header(p: &mut Painter, frame: &Frame, m: &Model, qs_open: bool) -> (Rect, Rect) {
    let h = frame.header;
    let w = &m.wall;
    // 12-hour mode shows "2:22" (no leading zero) with AM/PM beside it
    let time = if w.suffix.is_some() {
        format!("{}:{:02}", w.hour, w.minute)
    } else {
        format!("{:02}:{:02}", w.hour, w.minute)
    };
    let battery = battery_label(m);
    // a bolt before the percent while charging (owner request 2026-11-11)
    let battery = if m.status.as_ref().is_some_and(|s| {
        s.battery
            .state
            .as_deref()
            .is_some_and(|st| st.to_ascii_lowercase().starts_with("charg"))
    }) {
        format!("{}{battery}", walpad_ui::components::CHARGING_MARK)
    } else {
        battery
    };
    // mobile signal bars while a SIM is present (owner request 2026-11-10)
    let cell = crate::mobile::pill_cell(&m.status);
    // Bluetooth mark while the controller is powered (owner request 2026-11-10)
    let bt = crate::bluetooth::powered(&m.status);
    // GPS mark while the GPS is on (owner request 2026-11-11)
    let gps = crate::gps_screen::pill_mark(&m.status);
    let pill_w = p.quick_pill_w_marks(&battery, cell.is_some(), bt, gps.is_some());
    let low = m.status.as_ref().is_some_and(|s| s.battery.low);
    let pill = p.quick_pill_marks(
        h.right() - SP_4,
        h.y + h.h / 2,
        wifi_mark(m),
        &battery,
        low,
        m.pressed == Some(Target::QuickSettings),
        qs_open,
        cell,
        bt,
        gps,
    );
    // notification bell, just left of the pill (owner request 2026-11-12, change
    // device-notifications): a small icon box, never part of the pill component itself.
    let bell = Rect::new(
        pill.x - SP_3 - BELL_W,
        h.y + (h.h - BELL_W) / 2,
        BELL_W,
        BELL_W,
    );
    p.icon_box(
        bell,
        Icon::Bell,
        m.pressed == Some(Target::NotificationsCard),
        false,
    );
    let unread = m
        .status
        .as_ref()
        .map_or(0, |s| s.notifications.unread_count);
    if unread > 0 {
        crate::phone_ui::paint_badge(p, bell, unread);
    }
    let clock_x = h.x + SP_5;
    let mut clock_right = clock_x + h.w - pill_w - BELL_W - SP_3 - SP_4 - 2 * SP_5;
    // live stats between the clock and the pill, stopping short of the bell (owner request
    // 2026-11-11 / 2026-11-12)
    if let Some(st) = m.status.as_ref().map(|s| &s.stats) {
        let clock_end = clock_x + p.clock_time_w(&time, w.suffix);
        let date_w = p.fonts.measure(&w.date, Face::Regular, SIZE_CAPTION).ceil() as i32;
        let l = topbar::layout(p.fonts, st, h, clock_end, date_w, bell.x - SP_3);
        if let Some(sp) = l.pill {
            // the date keeps its room only while it fits left of the stats
            clock_right = if l.date { sp.x - SP_3 } else { clock_end };
            topbar::paint(p, &l);
        }
    }
    p.clock(
        Rect::new(clock_x, h.y, clock_right - clock_x, h.h),
        &time,
        w.suffix,
        &w.date,
    );
    p.divider(h.x, h.bottom() - 1, h.w);
    (pill, bell)
}
