//! The frame around every screen: the sidebar dock (left), the header (clock + quick-settings
//! pill) and the background layer. The background (charcoal + coffee watermark + the sidebar at
//! rest) is rendered once into a cached canvas and copied into each frame with one memcpy; it is
//! rebuilt only when its key changes (size, pinned apps, active destination, panel open).
use crate::app::{Model, Target};
use crate::panels::{battery_label, wifi_mark};
use walpad_ui::art::{coffee_rect, draw_coffee};
use walpad_ui::components::{Painter, Tab};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Canvas, Icon, Rect};

/// Index of each sidebar destination in `Tab::ALL` (`Target::Nav`).
pub const NAV_HOME: usize = 0;
pub const NAV_APPS: usize = 1;
pub const NAV_SETTINGS: usize = 4;

const PAD_TOP: i32 = 20;
const GAP: i32 = SP_3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SideItem {
    /// The drawn button.
    pub rect: Rect,
    /// The touch area: the full sidebar width around the button.
    pub hit: Rect,
    pub target: Target,
    pub icon: Icon,
    /// The Apps toggle is a circle, the rest rounded squares.
    pub round: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarLayout {
    pub items: Vec<SideItem>,
    /// y of the short divider above Settings.
    pub divider_y: i32,
}

/// Home, the pinned apps (`(tile index, icon)`, Settings shortcut excluded), a divider, Settings,
/// a flexible spacer, and the Apps toggle at the bottom (chevron right closed, left open).
pub fn sidebar_layout(frame: &Frame, pinned: &[(usize, Icon)], panel_open: bool) -> SidebarLayout {
    let sb = frame.sidebar;
    let x = sb.x + (sb.w - BOX_DOCK) / 2;
    let item = |y: i32, target: Target, icon: Icon, round: bool| SideItem {
        rect: Rect::new(x, y, BOX_DOCK, BOX_DOCK),
        hit: Rect::new(sb.x, y - GAP / 2, sb.w, BOX_DOCK + GAP),
        target,
        icon,
        round,
    };
    let toggle_y = sb.bottom() - PAD_TOP - BOX_DOCK;
    let mut items = vec![item(
        sb.y + PAD_TOP,
        Target::Nav(NAV_HOME),
        Icon::Home,
        false,
    )];
    let mut y = sb.y + PAD_TOP + BOX_DOCK + GAP;
    // pinned apps only while Settings and the toggle still fit below them
    let room = |y: i32| y + BOX_DOCK + 2 * GAP + 1 + BOX_DOCK + GAP <= toggle_y;
    for (i, icon) in pinned {
        if !room(y) {
            break;
        }
        items.push(item(y, Target::Tile(*i), *icon, false));
        y += BOX_DOCK + GAP;
    }
    let divider_y = y;
    y += 1 + GAP;
    items.push(item(y, Target::Nav(NAV_SETTINGS), Icon::Settings, false));
    items.push(item(
        toggle_y,
        Target::Nav(NAV_APPS),
        if panel_open {
            Icon::Back
        } else {
            Icon::ChevronRight
        },
        true,
    ));
    SidebarLayout { items, divider_y }
}

/// The pinned apps shown in the sidebar: Home tiles minus the Settings shortcut (Settings has
/// its own button).
pub fn pinned_items(m: &Model) -> Vec<(usize, Icon)> {
    m.tiles
        .iter()
        .enumerate()
        .filter(|(_, t)| t.id != "settings")
        .map(|(i, t)| (i, t.icon))
        .collect()
}

/// The sidebar destination drawn as active.
pub fn active_target(active: Tab, panel_open: bool) -> Option<Target> {
    if panel_open {
        return Some(Target::Nav(NAV_APPS));
    }
    match active {
        Tab::Home | Tab::Apps => Some(Target::Nav(NAV_HOME)),
        Tab::Settings => Some(Target::Nav(NAV_SETTINGS)),
        Tab::Network | Tab::Device | Tab::Mobile | Tab::Bluetooth => None,
    }
}

fn draw_item(p: &mut Painter, it: &SideItem, pressed: bool, active: bool) {
    if it.round {
        p.round_button(it.rect, it.icon, pressed, active);
    } else {
        p.icon_box(it.rect, it.icon, pressed, active);
    }
}

/// What the cached background depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChromeKey {
    pub w: i32,
    pub h: i32,
    pub sidebar: SidebarLayout,
    pub active: Option<Target>,
}

/// The wallpaper (bottom-aligned: the shell shrinks above the dock strip), the sidebar edge and its
/// buttons at rest.  Without a wallpaper (decode failed) plain charcoal with the line-art cup.
pub fn render_background(p: &mut Painter, frame: &Frame, key: &ChromeKey, wall: Option<&Canvas>) {
    render_base(p, frame, key, wall);
    render_items(p, key);
}

/// Everything of the background except the sidebar buttons.
fn render_base(p: &mut Painter, frame: &Frame, key: &ChromeKey, wall: Option<&Canvas>) {
    p.cv.fill(BG);
    match wall {
        Some(w) => p.cv.blit(w, 0, frame.h - w.height()),
        None => draw_coffee(p.cv, coffee_rect(frame.content), 3.0, WATERMARK),
    }
    let sb = frame.sidebar;
    p.cv.rect(Rect::new(sb.right() - 1, sb.y, 1, sb.h), DIVIDER);
    p.divider(sb.x + (sb.w - 40) / 2, key.sidebar.divider_y, 40);
}

fn render_items(p: &mut Painter, key: &ChromeKey) {
    for it in &key.sidebar.items {
        draw_item(p, it, false, key.active == Some(it.target));
    }
}

struct Cached {
    key: ChromeKey,
    cv: Canvas,
    /// The sidebar strip before its buttons were drawn (x 0, y 0, sidebar width): a pressed
    /// button is drawn over this, never over the resting one (translucent fills would stack) and
    /// never over a square of plain colour (the round toggle would show a box).
    bare_sidebar: Option<Canvas>,
}

/// The cached background layer.
#[derive(Default)]
pub struct Chrome {
    cache: Option<Cached>,
    /// Decoded wallpaper and whether it is the landscape one (decoded once per orientation).
    wall: Option<(bool, Canvas)>,
    /// Times the layer was rebuilt (tests check it is not per frame).
    pub builds: u64,
}

impl Chrome {
    /// Copy the background for `key` into `p.cv`, rebuilding the cache when the key changed.
    pub fn paint(&mut self, p: &mut Painter, frame: &Frame, key: ChromeKey) {
        let stale = self.cache.as_ref().is_none_or(|c| c.key != key);
        let landscape = frame.w > frame.h;
        if self.wall.as_ref().is_none_or(|(l, _)| *l != landscape) {
            self.wall = walpad_ui::art::wallpaper(frame.w, frame.h).map(|c| (landscape, c));
        }
        let wall = self.wall.as_ref().map(|(_, c)| c);
        if stale {
            match Canvas::new(frame.w as u32, frame.h as u32, BG) {
                Some(mut cv) => {
                    let mut bp = Painter {
                        cv: &mut cv,
                        fonts: &mut *p.fonts,
                    };
                    render_base(&mut bp, frame, &key, wall);
                    let sb = frame.sidebar;
                    let bare_sidebar = bp.cv.crop(Rect::new(0, 0, sb.right(), frame.h));
                    render_items(&mut bp, &key);
                    self.builds += 1;
                    self.cache = Some(Cached {
                        key: key.clone(),
                        cv,
                        bare_sidebar,
                    });
                }
                None => self.cache = None,
            }
        }
        match &self.cache {
            Some(c) if p.cv.copy_from(&c.cv) => {}
            // no cache (allocation failed) or a canvas of another size: draw it directly
            _ => render_background(p, frame, &key, wall),
        }
    }

    /// Restore `r` from the cached background (the header and banner strip after scrolled
    /// content was drawn under them). Falls back to a plain fill.
    pub fn restore(&self, cv: &mut Canvas, r: Rect) {
        match &self.cache {
            Some(c) if cv.copy_rect_from(&c.cv, r) => {}
            _ => cv.rect(r, BG),
        }
    }

    /// Draw the pressed sidebar button (the only per-frame sidebar work): its rect is first put
    /// back to the bare background (wallpaper, no button), then the button is drawn pressed. The
    /// round toggle's pressed state is a circle; its bounding-box corners keep the background.
    pub fn render_pressed(&self, p: &mut Painter, layout: &SidebarLayout, pressed: Option<Target>) {
        let Some(it) = layout.items.iter().find(|it| Some(it.target) == pressed) else {
            return;
        };
        // without a cache the frame was drawn directly and the resting button stays under the
        // pressed one (only the tint and the white outline are added: still round)
        if let Some(bare) = self.cache.as_ref().and_then(|c| c.bare_sidebar.as_ref()) {
            p.cv.copy_rect_from_at(bare, (0, 0), it.rect);
        }
        draw_item(p, it, true, false);
    }
}

/// Header: clock (12/24 h as configured) and date at the left, the quick-settings pill at the
/// right. Returns the pill rect.
pub fn render_header(p: &mut Painter, frame: &Frame, m: &Model, qs_open: bool) -> Rect {
    let h = frame.header;
    let w = &m.wall;
    // 12-hour mode shows "2:22" (no leading zero) with AM/PM beside it
    let time = if w.suffix.is_some() {
        format!("{}:{:02}", w.hour, w.minute)
    } else {
        format!("{:02}:{:02}", w.hour, w.minute)
    };
    let battery = battery_label(m);
    // mobile signal bars while a SIM is present (owner request 2026-11-10)
    let cell = crate::mobile::pill_cell(&m.status);
    // Bluetooth mark while the controller is powered (owner request 2026-11-10)
    let bt = crate::bluetooth::powered(&m.status);
    let pill_w = p.quick_pill_w_ex(&battery, cell.is_some(), bt);
    let low = m.status.as_ref().is_some_and(|s| s.battery.low);
    let pill = p.quick_pill_ex(
        h.right() - SP_4,
        h.y + h.h / 2,
        wifi_mark(m),
        &battery,
        low,
        m.pressed == Some(Target::QuickSettings),
        qs_open,
        cell,
        bt,
    );
    p.clock(
        Rect::new(h.x + SP_5, h.y, h.w - pill_w - SP_4 - 2 * SP_5, h.h),
        &time,
        w.suffix,
        &w.date,
    );
    pill
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    }

    #[test]
    fn sidebar_has_home_pinned_settings_and_the_toggle_at_the_bottom() {
        let pinned = [(0, Icon::Globe), (1, Icon::Terminal)];
        for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
            let l = sidebar_layout(&f, &pinned, false);
            let t: Vec<Target> = l.items.iter().map(|i| i.target).collect();
            assert_eq!(
                t,
                [
                    Target::Nav(NAV_HOME),
                    Target::Tile(0),
                    Target::Tile(1),
                    Target::Nav(NAV_SETTINGS),
                    Target::Nav(NAV_APPS)
                ]
            );
            let toggle = l.items.last().unwrap();
            assert!(toggle.round && toggle.icon == Icon::ChevronRight);
            assert_eq!(toggle.rect.bottom(), f.h - PAD_TOP);
            for (i, a) in l.items.iter().enumerate() {
                assert!(a.rect.w >= TOUCH_MIN && a.hit.w == SIDEBAR_W);
                assert!(f.sidebar.contains(a.rect.x, a.rect.y));
                for b in &l.items[i + 1..] {
                    assert!(!overlaps(a.hit, b.hit), "{a:?} / {b:?}");
                }
            }
            assert!(l.items[2].rect.bottom() < l.divider_y && l.divider_y < l.items[3].rect.y);
        }
        let open = sidebar_layout(&Frame::new(800, 1280), &pinned, true);
        assert_eq!(open.items.last().unwrap().icon, Icon::Back);
    }

    #[test]
    fn too_many_pins_never_push_settings_off_the_bar() {
        let pinned: Vec<(usize, Icon)> = (0..12).map(|i| (i, Icon::Apps)).collect();
        let l = sidebar_layout(&Frame::new(1280, 800), &pinned, false);
        let settings = l
            .items
            .iter()
            .find(|i| i.target == Target::Nav(NAV_SETTINGS))
            .unwrap();
        let toggle = l.items.last().unwrap();
        assert!(settings.hit.bottom() <= toggle.hit.y);
    }
}
