//! Components (docs/rust/UI.md section "Components"). Each draws one widget in one explicit state
//! and returns hit rectangles where the caller needs them. No animation state lives here: the shell
//! passes the current state (and spinner angle) every frame.
//!
//! Line-art theme: controls are outlines on the charcoal background. Pressed = 10 % white fill and
//! a full-white outline (there is no hover on a touch panel). Colours are opaque pre-blends
//! (tokens.rs), so nothing here blends a translucent layer over the frame.
use crate::canvas::{Canvas, Rect};
use crate::icons::Icon;
use crate::text::{Align, Face, Fonts, Style};
use crate::tokens::*;

pub struct Painter<'a> {
    pub cv: &'a mut Canvas,
    pub fonts: &'a mut Fonts,
}

// ------------------------------------------------------------------ small enums
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Danger,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Default,
    Pressed,
    Disabled,
    /// Spinner angle in degrees.
    Loading(u16),
}
#[derive(Debug, Clone, PartialEq)]
pub enum CardValue {
    Value(String),
    /// Reading is a voltage estimate: shown with `~` and a warning caption.
    Estimated(String),
    Unknown,
    Error(String),
}
#[derive(Debug, Clone, PartialEq)]
pub enum Accessory {
    None,
    Chevron,
    Toggle(bool),
    Value(String),
    Action(String),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetState {
    Idle,
    Connected,
    /// Spinner angle in degrees.
    Connecting(u16),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Warning,
    Danger,
}
/// The Wi-Fi mark in the header pill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiMark {
    Connected,
    Disconnected,
    Failed,
    /// No status from the core.
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Home,
    Apps,
    Network,
    Device,
    Settings,
    /// Mobile network / SIM (owner request 2026-11-10; reached from Quick Settings and Settings).
    Mobile,
    /// Bluetooth devices (owner request 2026-11-10; reached from Quick Settings and Settings).
    Bluetooth,
    /// VPN, DNS and proxy (owner request 2026-11-10; reached from Settings).
    Vpn,
    /// Media browser (owner request 2026-11-11; reached from the Apps panel and Home).
    Media,
    /// Phone: dialer, recent calls, contacts, messages (owner request 2026-11-11).
    Phone,
    /// GPS switch, position and satellites (owner request 2026-11-11; Quick Settings, Settings).
    Gps,
    /// Notification centre (owner request 2026-11-12, change `device-notifications`; reached from
    /// the bell icon in the header, not a sidebar destination).
    Notifications,
}
impl Tab {
    pub const ALL: [Tab; 12] = [
        Tab::Home,
        Tab::Apps,
        Tab::Network,
        Tab::Device,
        Tab::Settings,
        Tab::Mobile,
        Tab::Bluetooth,
        Tab::Vpn,
        Tab::Media,
        Tab::Phone,
        Tab::Gps,
        Tab::Notifications,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Tab::Home => "Home",
            Tab::Apps => "Apps",
            Tab::Network => "Network",
            Tab::Device => "Device",
            Tab::Settings => "Settings",
            Tab::Mobile => "Mobile network",
            Tab::Bluetooth => "Bluetooth",
            Tab::Vpn => "VPN and DNS",
            Tab::Media => "Media",
            Tab::Phone => "Phone",
            Tab::Gps => "GPS",
            Tab::Notifications => "Notifications",
        }
    }
    pub fn icon(self) -> Icon {
        match self {
            Tab::Home => Icon::Home,
            Tab::Apps => Icon::Apps,
            Tab::Network => Icon::Wifi,
            Tab::Device => Icon::Device,
            Tab::Settings => Icon::Settings,
            Tab::Mobile => Icon::Sim,
            Tab::Bluetooth => Icon::Bluetooth,
            Tab::Vpn => Icon::Lock,
            Tab::Media => Icon::Media,
            Tab::Phone => Icon::Phone,
            Tab::Gps => Icon::Location,
            Tab::Notifications => Icon::Bell,
        }
    }
}

pub fn tone_color(t: Tone) -> Color {
    match t {
        Tone::Info => TEXT,
        Tone::Warning => WARNING,
        Tone::Danger => DANGER,
    }
}

/// Tone wash of a banner (14 %), blended over its `SURFACE_STRONG` fill.
pub fn banner_tint(t: Tone) -> Tint {
    Tint::new(
        match t {
            Tone::Info => TEXT,
            other => tone_color(other),
        },
        14,
    )
}

/// Danger wash of a danger button (rest 16 %, pressed 30 %), over its surface fill.
const DANGER_TINT: Tint = Tint::new(DANGER, 16);
const DANGER_TINT_PRESSED: Tint = Tint::new(DANGER, 30);

/// Bounding rects of the runs of vertically adjacent list rows (`(row, in a group)`; a row that
/// is not grouped, or a gap, ends the run): what `Painter::row_group` backs.
pub fn row_runs(rows: impl IntoIterator<Item = (Rect, bool)>) -> Vec<Rect> {
    let mut out: Vec<Rect> = Vec::new();
    let mut open = false;
    for (r, grouped) in rows {
        if !grouped {
            open = false;
            continue;
        }
        match out.last_mut() {
            Some(g) if open && g.bottom() == r.y && g.x == r.x && g.w == r.w => g.h += r.h,
            _ => out.push(r),
        }
        open = true;
    }
    out
}

/// Icon size for a box of side `box_px` (24 px grid icons, scaled up in the bigger boxes).
pub fn icon_for_box(box_px: i32) -> i32 {
    match box_px {
        b if b >= 64 => 28,
        b if b >= 56 => 24,
        _ => 22,
    }
}

impl Painter<'_> {
    pub fn text(&mut self, s: &str, style: Style, r: Rect, align: Align) {
        self.fonts.draw_in(self.cv, s, style, r, align);
    }
    fn body(&mut self, s: &str, r: Rect, align: Align, color: Color) {
        self.text(s, Style::new(Face::Regular, SIZE_BODY, color), r, align);
    }
    fn small(&mut self, s: &str, r: Rect, align: Align, color: Color) {
        self.text(
            s,
            Style::new(Face::Regular, SIZE_SECONDARY, color),
            r,
            align,
        );
    }
    fn caption(&mut self, s: &str, r: Rect, align: Align, color: Color) {
        self.text(s, Style::new(Face::Regular, SIZE_CAPTION, color), r, align);
    }

    // ------------------------------------------------------------------ outlines
    /// The basic line-art control: a translucent `SURFACE` fill (blended over the wallpaper) and
    /// a rounded outline; pressed adds `PRESSED_TINT`. `active` marks the current destination
    /// (full-white outline).
    pub fn outline_box(
        &mut self,
        r: Rect,
        radius: i32,
        pressed: bool,
        active: bool,
        disabled: bool,
    ) {
        self.surface_box(r, radius, SURFACE, pressed, active, disabled);
    }

    /// `outline_box` with an explicit surface fill (`SURFACE_ICON` for icon boxes). Fill only, no
    /// stroke (owner request 2026-11-12: no borders on app icons, dropdowns or other controls);
    /// `active`/`pressed` read through a stronger fill instead of a brighter outline.
    pub fn surface_box(
        &mut self,
        r: Rect,
        radius: i32,
        fill: Tint,
        pressed: bool,
        active: bool,
        disabled: bool,
    ) {
        self.cv.rrect(r, radius, fill);
        if active && !disabled {
            self.cv.rrect(r, radius, SURFACE_STRONG);
        }
        if pressed && !disabled {
            self.cv.rrect(r, radius, PRESSED_TINT);
        }
    }

    /// Rounded-square icon button (sidebar 56, Home grid 64, Apps panel 48).
    pub fn icon_box(&mut self, r: Rect, icon: Icon, pressed: bool, active: bool) {
        self.surface_box(
            r,
            RADIUS_ICON.min(r.w / 3),
            SURFACE_ICON,
            pressed,
            active,
            false,
        );
        let s = icon_for_box(r.w);
        self.cv
            .icon(icon, r.x + (r.w - s) / 2, r.y + (r.h - s) / 2, s, TEXT);
    }

    /// Round button with an icon (the sidebar's Apps toggle). Fill only, no outline (owner
    /// request 2026-11-12): circle fill, a brighter fill for pressed/active; nothing is drawn
    /// outside the circle inscribed in `r` (its bounding-box corners stay untouched).
    pub fn round_button(&mut self, r: Rect, icon: Icon, pressed: bool, active: bool) {
        let d = r.w.min(r.h);
        let (cx, cy) = (r.x as f32 + r.w as f32 / 2.0, r.y as f32 + r.h as f32 / 2.0);
        let rad = d as f32 / 2.0;
        // it sits on the wallpaper art (bottom of the sidebar): the strong fill keeps the chevron clear
        self.cv.circle(cx, cy, rad, SURFACE_STRONG);
        if active {
            self.cv.circle(cx, cy, rad, SURFACE_STRONG);
        }
        if pressed {
            self.cv.circle(cx, cy, rad, PRESSED_TINT);
        }
        self.cv.icon(
            icon,
            r.x + (r.w - ICON) / 2,
            r.y + (r.h - ICON) / 2,
            ICON,
            TEXT,
        );
    }

    /// An app in a grid cell: `box_px` icon box centred at the top of `cell`, the name under it.
    /// A running app has a small dot in the box's top-right corner.
    #[allow(clippy::too_many_arguments)]
    pub fn app_icon(
        &mut self,
        cell: Rect,
        box_px: i32,
        icon: Icon,
        name: &str,
        pressed: bool,
        running: bool,
    ) {
        let b = Rect::new(cell.x + (cell.w - box_px) / 2, cell.y, box_px, box_px);
        self.icon_box(b, icon, pressed, false);
        if running {
            self.cv
                .circle((b.right() - 9) as f32, (b.y + 9) as f32, 3.0, TEXT);
        }
        let px = if box_px >= BOX_GRID {
            SIZE_LABEL
        } else {
            SIZE_TINY
        };
        self.text(
            name,
            Style::new(Face::Regular, px, TEXT_LABEL),
            Rect::new(cell.x - 4, b.bottom() + 6, cell.w + 8, 18),
            Align::Center,
        );
    }

    /// Launcher tile: the Home-grid look (64 px box + label) centred in `r`.
    pub fn app_tile(&mut self, r: Rect, icon: Icon, name: &str, pressed: bool, running: bool) {
        let top = r.y + (r.h - (BOX_GRID + 24)) / 2;
        self.app_icon(
            Rect::new(r.x, top, r.w, BOX_GRID + 24),
            BOX_GRID,
            icon,
            name,
            pressed,
            running,
        );
    }

    /// Card container: translucent `SURFACE` fill only (pressed: + `PRESSED_TINT`). No outline
    /// (owner request 2026-11-12, `ui-polish`): a card/row-group reads apart from the page purely
    /// through the shade difference between `SURFACE` and `PAGE_SURFACE`/the wallpaper, the same
    /// grouped-section language as iOS/Material settings (background colour, not a box outline).
    pub fn card(&mut self, r: Rect, pressed: bool) {
        self.cv.rrect(r, RADIUS_CARD, SURFACE);
        if pressed {
            self.cv.rrect(r, RADIUS_CARD, PRESSED_TINT);
        }
    }
    /// The page surface of a full-screen page: `PAGE_SURFACE` over the wallpaper, no outline
    /// (owner request 2026-11-12, `ui-polish`: the scrollable content area read as a box inside a
    /// box next to every card/row-group it holds; the shade difference from the wallpaper and from
    /// the cards on top of it is enough). Drawn once, into the cached background layer (never per
    /// row).
    pub fn page_surface(&mut self, content: Rect) {
        let r = page_rect(content);
        self.cv.rrect(r, RADIUS_CARD, PAGE_SURFACE);
    }
    /// The backing of a run of list rows (Settings, Device): a card around them, so rows over the
    /// wallpaper art stay readable. The rows themselves add no fill (only the pressed tint).
    pub fn row_group(&mut self, r: Rect) {
        if self.cv.visible(r) {
            self.card(r, false);
        }
    }
    /// Popover / dropdown / dialog / sheet body: `SURFACE_STRONG` fill only, no border (owner
    /// request 2026-11-12: no borders on dropdowns/pages); the stronger fill carries the edge
    /// against whatever floats under it.
    pub fn popover(&mut self, r: Rect) {
        self.cv.rrect(r, RADIUS_CARD, SURFACE_STRONG);
    }
    /// 1 px horizontal divider.
    pub fn divider(&mut self, x: i32, y: i32, w: i32) {
        self.cv.rect(Rect::new(x, y, w, 1), DIVIDER);
    }
    /// Section label: medium-small, secondary colour, upper case.
    pub fn section_label(&mut self, s: &str, r: Rect) {
        self.text(
            &s.to_uppercase(),
            Style::new(Face::Bold, SIZE_SECTION, TEXT_SECONDARY),
            r,
            Align::Left,
        );
    }
    /// Tiny upper-case caption ("ACTIVE APPS").
    pub fn caption_upper(&mut self, s: &str, r: Rect) {
        self.text(
            &s.to_uppercase(),
            Style::new(Face::Bold, SIZE_TINY, TEXT_SECONDARY),
            r,
            Align::Left,
        );
    }
    pub fn screen_title(&mut self, s: &str, r: Rect) {
        self.text(
            s,
            Style::new(Face::Regular, SIZE_TITLE, TEXT),
            r,
            Align::Left,
        );
    }

    // ------------------------------------------------------------------ Button
    pub fn button(
        &mut self,
        r: Rect,
        label: &str,
        kind: ButtonKind,
        state: ButtonState,
        icon: Option<Icon>,
    ) {
        let disabled = state == ButtonState::Disabled;
        let pressed = state == ButtonState::Pressed;
        let (wash, line, fg) = match (kind, disabled) {
            (_, true) => (None, DIVIDER, TEXT_DISABLED),
            (ButtonKind::Primary, _) => (pressed.then_some(PRESSED_TINT), TEXT, TEXT),
            (ButtonKind::Danger, _) => (
                Some(if pressed {
                    DANGER_TINT_PRESSED
                } else {
                    DANGER_TINT
                }),
                DANGER,
                DANGER,
            ),
            (ButtonKind::Secondary, _) => (
                pressed.then_some(PRESSED_TINT),
                if pressed { TEXT } else { OUTLINE },
                TEXT,
            ),
        };
        self.cv.rrect(r, RADIUS, SURFACE);
        if let Some(f) = wash {
            self.cv.rrect(r, RADIUS, f);
        }
        let width = if kind == ButtonKind::Primary && !disabled {
            2.0
        } else {
            OUTLINE_W
        };
        self.cv.rrect_stroke(r, RADIUS, width, line);
        if let ButtonState::Loading(deg) = state {
            let (cx, cy) = r.center();
            self.cv
                .icon_rotated(Icon::Spinner, cx - 12, cy - 12, ICON, TEXT, deg as f32);
            return;
        }
        let style = Style::new(Face::Regular, SIZE_BODY, fg);
        match icon {
            Some(ic) => {
                let w = self.fonts.measure(label, Face::Regular, SIZE_BODY) as i32 + ICON + SP_2;
                let x0 = r.x + (r.w - w).max(SP_3 * 2) / 2;
                self.cv.icon(ic, x0, r.y + (r.h - ICON) / 2, ICON, fg);
                let tx = x0 + ICON + SP_2;
                self.text(
                    label,
                    style,
                    Rect::new(tx, r.y, r.right() - SP_3 - tx, r.h),
                    Align::Left,
                );
            }
            None => self.text(label, style, r.inset(SP_2), Align::Center),
        }
    }

    // ------------------------------------------------------------------ StatusCard
    pub fn status_card(
        &mut self,
        r: Rect,
        title: &str,
        value: &CardValue,
        detail: Option<&str>,
        pressed: bool,
    ) {
        self.card(r, pressed);
        let inner = r.inset(SP_4);
        self.text(
            &title.to_uppercase(),
            Style::new(Face::Bold, SIZE_TINY, TEXT_SECONDARY),
            Rect::new(inner.x, inner.y, inner.w, 18),
            Align::Left,
        );
        let (text, color, caption): (String, Color, Option<(&str, Color)>) = match value {
            CardValue::Value(v) => (v.clone(), TEXT, None),
            CardValue::Estimated(v) => (
                format!("~{v}"),
                TEXT,
                Some(("Estimated from voltage", WARNING)),
            ),
            CardValue::Unknown => ("Unknown".into(), TEXT_SECONDARY, None),
            CardValue::Error(e) => (e.clone(), DANGER, None),
        };
        self.text(
            &text,
            Style::new(Face::Regular, SIZE_TITLE, color),
            Rect::new(inner.x, inner.y + 22, inner.w, 36),
            Align::Left,
        );
        let line3 = Rect::new(inner.x, inner.y + 60, inner.w, 20);
        match (caption, detail) {
            (Some((c, col)), _) => self.caption(c, line3, Align::Left, col),
            (None, Some(d)) => self.text(
                d,
                Style::new(Face::Mono, SIZE_CAPTION, TEXT_SECONDARY),
                line3,
                Align::Left,
            ),
            _ => {}
        }
    }

    // ------------------------------------------------------------------ Toggle / Slider / Segmented
    pub fn toggle(&mut self, x: i32, y: i32, on: bool, pressed: bool, disabled: bool) -> Rect {
        let r = Rect::new(x, y, 56, 32);
        let knob_r = if pressed { 11.0 } else { 10.0 };
        let cy = (r.y + 16) as f32;
        if on && !disabled {
            self.cv.rrect(r, 16, TEXT);
            self.cv.circle((r.right() - 16) as f32, cy, knob_r, BG);
        } else {
            let line = if disabled { DIVIDER } else { OUTLINE };
            self.cv.rrect_stroke(r, 16, OUTLINE_W, line);
            let kx = if on { r.right() - 16 } else { r.x + 16 };
            self.cv.circle(
                kx as f32,
                cy,
                knob_r - 2.0,
                if disabled { DIVIDER } else { OUTLINE },
            );
        }
        r
    }

    /// Value 0.0..=1.0. Returns the track rect (the caller maps x to a value).
    pub fn slider(&mut self, r: Rect, value: f32, dragging: bool, disabled: bool) -> Rect {
        let v = value.clamp(0.0, 1.0);
        let track = Rect::new(r.x + 16, r.y + r.h / 2 - 2, r.w - 32, 4);
        let fill = if disabled { TEXT_DISABLED } else { TEXT };
        self.cv.rrect(track, 2, DIVIDER);
        let fw = (track.w as f32 * v) as i32;
        if fw > 0 {
            self.cv
                .rrect(Rect::new(track.x, track.y, fw.max(4), 4), 2, fill);
        }
        let kx = (track.x + fw) as f32;
        let ky = (track.y + 2) as f32;
        self.cv
            .circle(kx, ky, if dragging { 15.0 } else { 12.0 }, fill);
        self.cv.circle(kx, ky, if dragging { 5.0 } else { 4.0 }, BG);
        track
    }

    /// Returns one hit rect per segment.
    pub fn segmented(
        &mut self,
        r: Rect,
        labels: &[&str],
        selected: usize,
        pressed: Option<usize>,
        disabled: bool,
    ) -> Vec<Rect> {
        self.cv.rrect(r, RADIUS, SURFACE);
        self.cv
            .rrect_stroke(r, RADIUS, 1.0, if disabled { DIVIDER } else { OUTLINE });
        let n = labels.len().max(1) as i32;
        let w = (r.w - 8) / n;
        let mut out = Vec::new();
        for (i, l) in labels.iter().enumerate() {
            let seg = Rect::new(r.x + 4 + i as i32 * w, r.y + 4, w, r.h - 8);
            let sel = i == selected;
            if sel {
                self.cv.rrect(seg, RADIUS - 4, PRESSED_TINT);
                self.cv.rrect_stroke(
                    seg,
                    RADIUS - 4,
                    OUTLINE_W,
                    if disabled { TEXT_DISABLED } else { TEXT },
                );
            } else if pressed == Some(i) && !disabled {
                self.cv.rrect(seg, RADIUS - 4, PRESSED_TINT);
            }
            let fg = match (sel, disabled) {
                (_, true) => TEXT_DISABLED,
                (true, _) => TEXT,
                _ => TEXT_SECONDARY,
            };
            self.text(
                l,
                Style::new(
                    if sel { Face::Bold } else { Face::Regular },
                    SIZE_SECONDARY,
                    fg,
                ),
                seg,
                Align::Center,
            );
            out.push(seg);
        }
        out
    }

    // ------------------------------------------------------------------ ListRow
    /// Pressed highlight of a list row: a rounded `PRESSED_TINT` inset from the row edges.
    pub fn row_pressed(&mut self, r: Rect) {
        self.cv.rrect(
            Rect::new(r.x + 4, r.y + 3, r.w - 8, r.h - 6),
            RADIUS - 2,
            PRESSED_TINT,
        );
    }

    /// Returns the rect of the toggle when the accessory is one (for hit testing).
    #[allow(clippy::too_many_arguments)]
    pub fn list_row(
        &mut self,
        r: Rect,
        icon: Option<Icon>,
        title: &str,
        subtitle: Option<&str>,
        acc: &Accessory,
        pressed: bool,
        disabled: bool,
    ) -> Option<Rect> {
        if !self.cv.visible(r) {
            return matches!(acc, Accessory::Toggle(_))
                .then(|| Rect::new(r.right() - SP_4 - 56, r.y + (r.h - 32) / 2, 56, 32));
        }
        if pressed && !disabled {
            self.row_pressed(r);
        }
        self.divider(r.x + SP_4, r.bottom() - 1, r.w - 2 * SP_4);
        let fg = if disabled { TEXT_DISABLED } else { TEXT };
        let mut x = r.x + SP_4;
        if let Some(ic) = icon {
            self.cv.icon(
                ic,
                x,
                r.y + (r.h - ICON) / 2,
                ICON,
                if disabled { TEXT_DISABLED } else { OUTLINE },
            );
            x += ICON + SP_4;
        }
        let right_w = match acc {
            Accessory::None => 0,
            Accessory::Chevron => ICON + SP_4,
            Accessory::Toggle(_) => 56 + SP_4,
            Accessory::Value(v) | Accessory::Action(v) => {
                self.fonts.measure(v, Face::Regular, SIZE_BODY) as i32 + SP_4 + SP_3
            }
        };
        let text_w = r.w - (x - r.x) - right_w - SP_2;
        match subtitle {
            Some(sub) => {
                self.body(title, Rect::new(x, r.y + 8, text_w, 28), Align::Left, fg);
                self.small(
                    sub,
                    Rect::new(x, r.y + 34, text_w, 22),
                    Align::Left,
                    TEXT_SECONDARY,
                );
            }
            None => self.body(title, Rect::new(x, r.y, text_w, r.h), Align::Left, fg),
        }
        let ar = Rect::new(r.right() - right_w - SP_2, r.y, right_w, r.h);
        match acc {
            Accessory::None => None,
            Accessory::Chevron => {
                self.cv.icon(
                    Icon::ChevronRight,
                    r.right() - SP_4 - ICON,
                    r.y + (r.h - ICON) / 2,
                    ICON,
                    TEXT_SECONDARY,
                );
                None
            }
            Accessory::Toggle(on) => Some(self.toggle(
                r.right() - SP_4 - 56,
                r.y + (r.h - 32) / 2,
                *on,
                pressed,
                disabled,
            )),
            Accessory::Value(v) => {
                self.body(
                    v,
                    Rect::new(ar.x, ar.y, right_w - SP_4, ar.h),
                    Align::Right,
                    TEXT_SECONDARY,
                );
                None
            }
            Accessory::Action(v) => {
                self.text(
                    v,
                    Style::new(
                        Face::Bold,
                        SIZE_BODY,
                        if disabled { TEXT_DISABLED } else { TEXT },
                    ),
                    Rect::new(ar.x, ar.y, right_w - SP_4, ar.h),
                    Align::Right,
                );
                None
            }
        }
    }

    // ------------------------------------------------------------------ signal, NetworkRow
    /// Four-step signal icon (0..=4 bars lit).
    pub fn signal_bars(&mut self, x: i32, y: i32, bars: u8, color: Color) {
        for i in 0..4 {
            let h = 6 + i * 4;
            let lit = (i as u8) < bars;
            self.cv.rrect(
                Rect::new(x + i * 7, y + 18 - h, 4, h),
                1,
                if lit { color } else { DIVIDER },
            );
        }
    }

    pub fn network_row(
        &mut self,
        r: Rect,
        ssid: &str,
        bars: u8,
        secured: bool,
        state: NetState,
        pressed: bool,
    ) {
        if !self.cv.visible(r) {
            return;
        }
        if pressed {
            self.row_pressed(r);
        }
        self.divider(r.x + SP_4, r.bottom() - 1, r.w - 2 * SP_4);
        let (sub, subcol) = match (state, secured) {
            (NetState::Connected, _) => ("Connected", SUCCESS),
            (NetState::Connecting(_), _) => ("Connecting…", WARNING),
            (_, true) => ("Secured", TEXT_SECONDARY),
            (_, false) => ("Open", TEXT_SECONDARY),
        };
        let right = r.right() - SP_4;
        let sig_x = right - 26;
        self.signal_bars(
            sig_x,
            r.y + (r.h - 18) / 2,
            bars,
            if state == NetState::Connected {
                SUCCESS
            } else {
                TEXT
            },
        );
        let mut tx = sig_x - SP_3;
        if secured {
            tx -= ICON;
            self.cv
                .icon(Icon::Lock, tx, r.y + (r.h - ICON) / 2, ICON, TEXT_SECONDARY);
            tx -= SP_2;
        }
        // connection state icon (check / spinner) left of the lock
        let status_x = match state {
            NetState::Idle => None,
            _ => {
                tx -= ICON;
                let x = tx;
                tx -= SP_2;
                Some(x)
            }
        };
        let w = tx - (r.x + SP_4);
        self.text(
            ssid,
            Style::new(
                if state == NetState::Connected {
                    Face::Bold
                } else {
                    Face::Regular
                },
                SIZE_BODY,
                TEXT,
            ),
            Rect::new(r.x + SP_4, r.y + 10, w, 28),
            Align::Left,
        );
        self.text(
            sub,
            Style::new(Face::Regular, SIZE_SECONDARY, subcol),
            Rect::new(r.x + SP_4, r.y + 38, w, 22),
            Align::Left,
        );
        if let Some(x) = status_x {
            let y = r.y + (r.h - ICON) / 2;
            match state {
                NetState::Connecting(deg) => {
                    self.cv
                        .icon_rotated(Icon::Spinner, x, y, ICON, WARNING, deg as f32)
                }
                _ => self.cv.icon(Icon::Check, x, y, ICON, SUCCESS),
            }
        }
    }

    // ------------------------------------------------------------------ Sheet, Dialog, PowerMenu
    /// Bottom sheet container; returns the content rect inside it.
    pub fn sheet(&mut self, r: Rect, title: &str) -> Rect {
        self.popover(r);
        self.cv
            .rrect(Rect::new(r.x + r.w / 2 - 24, r.y + 8, 48, 4), 2, DIVIDER);
        self.text(
            title,
            Style::new(Face::Regular, SIZE_TITLE, TEXT),
            Rect::new(r.x + SP_5, r.y + 20, r.w - 2 * SP_5, 40),
            Align::Left,
        );
        Rect::new(r.x + SP_5, r.y + 68, r.w - 2 * SP_5, r.h - 68 - SP_5)
    }

    /// Centred confirmation dialog. Returns (cancel, confirm) button rects. `armed` = the
    /// destructive first tap happened: the confirm button reads `confirm_armed`.
    #[allow(clippy::too_many_arguments)]
    pub fn dialog(
        &mut self,
        screen: Rect,
        title: &str,
        body: &str,
        cancel: &str,
        confirm: &str,
        confirm_armed: &str,
        danger: bool,
        armed: bool,
    ) -> (Rect, Rect) {
        let w = 560.min(screen.w - 2 * SP_5);
        let h = 260;
        let r = Rect::new(
            screen.x + (screen.w - w) / 2,
            screen.y + (screen.h - h) / 2,
            w,
            h,
        );
        self.popover(r);
        self.text(
            title,
            Style::new(Face::Regular, SIZE_TITLE, TEXT),
            Rect::new(r.x + SP_5, r.y + SP_5, r.w - 2 * SP_5, 40),
            Align::Left,
        );
        self.small(
            body,
            Rect::new(r.x + SP_5, r.y + 72, r.w - 2 * SP_5, 48),
            Align::Left,
            TEXT_SECONDARY,
        );
        let bw = (r.w - 2 * SP_5 - SP_4) / 2;
        let cancel_r = Rect::new(r.x + SP_5, r.bottom() - SP_5 - BUTTON_H, bw, BUTTON_H);
        let confirm_r = Rect::new(cancel_r.right() + SP_4, cancel_r.y, bw, BUTTON_H);
        self.button(
            cancel_r,
            cancel,
            ButtonKind::Secondary,
            ButtonState::Default,
            None,
        );
        let kind = if danger {
            ButtonKind::Danger
        } else {
            ButtonKind::Primary
        };
        self.button(
            confirm_r,
            if armed { confirm_armed } else { confirm },
            kind,
            ButtonState::Default,
            None,
        );
        (cancel_r, confirm_r)
    }

    /// Power menu (long power press). `focus` is the keyboard/hardware-key focus ring, `armed` the
    /// item whose first tap happened (danger + "Tap again"). Returns one rect per item.
    pub fn power_menu(
        &mut self,
        screen: Rect,
        items: &[(&str, bool)],
        focus: usize,
        armed: Option<usize>,
    ) -> Vec<Rect> {
        let w = 560.min(screen.w - 2 * SP_5);
        let item_h = BUTTON_H + SP_2;
        let h = SP_5 * 2 + 40 + SP_3 + items.len() as i32 * item_h;
        let r = Rect::new(
            screen.x + (screen.w - w) / 2,
            screen.y + (screen.h - h) / 2,
            w,
            h,
        );
        self.popover(r);
        self.text(
            "Power",
            Style::new(Face::Regular, SIZE_TITLE, TEXT),
            Rect::new(r.x + SP_5, r.y + SP_5, r.w - 2 * SP_5, 40),
            Align::Left,
        );
        let mut out = Vec::new();
        for (i, (label, dangerous)) in items.iter().enumerate() {
            let ir = Rect::new(
                r.x + SP_5,
                r.y + SP_5 + 40 + SP_3 + i as i32 * item_h,
                r.w - 2 * SP_5,
                BUTTON_H,
            );
            let is_armed = armed == Some(i);
            let text = if is_armed {
                format!("Tap again to {}", label.to_lowercase())
            } else {
                label.to_string()
            };
            let kind = if *dangerous && is_armed {
                ButtonKind::Danger
            } else {
                ButtonKind::Secondary
            };
            self.button(ir, &text, kind, ButtonState::Default, None);
            if focus == i {
                // focus ring: a 2 px white outline just outside the button
                self.cv.rrect_stroke(ir.inset(-4), RADIUS + 4, 2.0, TEXT);
            }
            out.push(ir);
        }
        out
    }

    // ------------------------------------------------------------------ TextField
    /// Returns the rect of the show/hide button (secret fields) for hit testing.
    #[allow(clippy::too_many_arguments)]
    pub fn text_field(
        &mut self,
        r: Rect,
        value: &str,
        placeholder: &str,
        secret: bool,
        shown: bool,
        focused: bool,
        error: Option<&str>,
    ) -> Option<Rect> {
        let (border, width) = if error.is_some() {
            (DANGER, OUTLINE_W)
        } else if focused {
            (TEXT, OUTLINE_W)
        } else {
            (OUTLINE, 1.0)
        };
        self.cv.rrect(r, RADIUS, SURFACE);
        self.cv.rrect_stroke(r, RADIUS, width, border);
        let eye = secret.then(|| Rect::new(r.right() - TOUCH_MIN, r.y, TOUCH_MIN, r.h));
        let text_w = r.w - 2 * SP_4 - if secret { TOUCH_MIN - SP_4 } else { 0 };
        let area = Rect::new(r.x + SP_4, r.y, text_w, r.h);
        // the text as displayed: bullets for hidden secrets, otherwise the tail that fits
        let display = if value.is_empty() {
            String::new()
        } else if secret && !shown {
            "•".repeat(value.chars().count())
        } else {
            tail_fit(self.fonts, value, text_w as f32)
        };
        if value.is_empty() {
            self.body(placeholder, area, Align::Left, TEXT_DISABLED);
        } else {
            self.body(&display, area, Align::Left, TEXT);
        }
        if focused {
            let w = if display.is_empty() {
                0.0
            } else {
                self.fonts.measure(&display, Face::Regular, SIZE_BODY)
            };
            let cx = (area.x as f32 + w).round() as i32 + 1;
            self.cv.rect(Rect::new(cx, r.y + 14, 2, r.h - 28), TEXT);
        }
        if let Some(e) = eye {
            self.cv.icon(
                if shown { Icon::EyeOff } else { Icon::Eye },
                e.x + (e.w - ICON) / 2,
                e.y + (e.h - ICON) / 2,
                ICON,
                TEXT_SECONDARY,
            );
        }
        if let Some(msg) = error {
            self.small(
                msg,
                Rect::new(r.x + 4, r.bottom() + 6, r.w, 22),
                Align::Left,
                DANGER,
            );
        }
        eye
    }

    // ------------------------------------------------------------------ Toast, Banner, states
    /// One toast, centred near the top of `area` (the main area below the header).
    pub fn toast(&mut self, area: Rect, text: &str, tone: Tone) -> Rect {
        let tw = self.fonts.measure(text, Face::Regular, SIZE_SECONDARY) as i32;
        let w = (tw + 2 * SP_4 + ICON + SP_2 + 8)
            .min(area.w - 2 * SP_5)
            .max(160);
        let r = Rect::new(area.x + (area.w - w) / 2, area.y + SP_2, w, 48);
        self.cv.rrect(r, 24, SURFACE_STRONG);
        self.cv.rrect_stroke(r, 24, OUTLINE_W, tone_color(tone));
        let ic = match tone {
            Tone::Info => Icon::Check,
            _ => Icon::Warning,
        };
        self.cv
            .icon(ic, r.x + SP_4, r.y + 12, ICON, tone_color(tone));
        self.small(
            text,
            Rect::new(
                r.x + SP_4 + ICON + SP_2,
                r.y,
                r.w - 2 * SP_4 - ICON - SP_2,
                r.h,
            ),
            Align::Left,
            TEXT,
        );
        r
    }

    pub fn banner(&mut self, r: Rect, text: &str, tone: Tone) {
        self.cv.rrect(r, RADIUS, SURFACE_STRONG);
        self.cv.rrect(r, RADIUS, banner_tint(tone));
        self.cv.rrect_stroke(r, RADIUS, 1.0, tone_color(tone));
        self.cv.icon(
            Icon::Warning,
            r.x + SP_4,
            r.y + (r.h - ICON) / 2,
            ICON,
            tone_color(tone),
        );
        self.small(
            text,
            Rect::new(
                r.x + SP_4 + ICON + SP_3,
                r.y,
                r.w - 2 * SP_4 - ICON - SP_3,
                r.h,
            ),
            Align::Left,
            TEXT,
        );
    }

    pub fn spinner(&mut self, cx: i32, cy: i32, size: i32, deg: f32) {
        self.cv
            .icon_rotated(Icon::Spinner, cx - size / 2, cy - size / 2, size, TEXT, deg);
    }

    pub fn empty_state(&mut self, r: Rect, icon: Icon, title: &str, hint: &str) {
        let (cx, cy) = r.center();
        self.cv.icon(icon, cx - 24, cy - 56, 48, TEXT_DISABLED);
        self.text(
            title,
            Style::new(Face::Regular, SIZE_BODY, TEXT_SECONDARY),
            Rect::new(r.x, cy, r.w, 28),
            Align::Center,
        );
        self.small(
            hint,
            Rect::new(r.x + SP_5, cy + 30, r.w - 2 * SP_5, 24),
            Align::Center,
            TEXT_DISABLED,
        );
    }

    /// Returns the retry button rect.
    pub fn error_state(&mut self, r: Rect, title: &str, msg: &str, retry: &str) -> Rect {
        let (cx, cy) = r.center();
        self.cv.icon(Icon::Warning, cx - 24, cy - 88, 48, DANGER);
        self.text(
            title,
            Style::new(Face::Regular, SIZE_BODY, TEXT),
            Rect::new(r.x, cy - 28, r.w, 28),
            Align::Center,
        );
        self.small(
            msg,
            Rect::new(r.x + SP_5, cy + 2, r.w - 2 * SP_5, 24),
            Align::Center,
            TEXT_SECONDARY,
        );
        let b = Rect::new(cx - 90, cy + 40, 180, BUTTON_H);
        self.button(
            b,
            retry,
            ButtonKind::Secondary,
            ButtonState::Default,
            Some(Icon::Refresh),
        );
        b
    }

    // ------------------------------------------------------------------ Header: clock, quick pill
    /// Clock (and the 12-hour suffix) at the left of the header, the date after it in secondary.
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
        let style = Style::new(Face::Regular, SIZE_CLOCK, TEXT);
        let tw = self.fonts.measure(time, Face::Regular, SIZE_CLOCK).ceil() as i32;
        self.text(time, style, Rect::new(r.x, r.y, tw + 2, r.h), Align::Left);
        let mut x = r.x + tw + SP_1;
        if let Some(sfx) = suffix {
            let sw = self.fonts.measure(sfx, Face::Regular, SIZE_CAPTION).ceil() as i32;
            self.caption(
                sfx,
                Rect::new(x + 2, r.y + 2, sw + 2, r.h),
                Align::Left,
                TEXT_SECONDARY,
            );
            x += sw + SP_1 + 2;
        }
        let dx = x + SP_3;
        if r.right() - dx > 80 {
            self.caption(
                date,
                Rect::new(dx, r.y + 1, r.right() - dx, r.h),
                Align::Left,
                TEXT_SECONDARY,
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
            self.cv.rrect(
                Rect::new(x + i * 5, y + 16 - h, 3, h),
                1,
                if lit { color } else { DIVIDER },
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
        self.outline_box(r, 20, pressed, open, false);
        let mut x = r.x + 14;
        if let Some((bars, c)) = cell {
            self.cell_bars(x, cy - 8, bars, c);
            x += CELL_BARS_W + SP_2;
        }
        if bt {
            self.cv.icon(Icon::Bluetooth, x, cy - 9, BT_MARK_W, TEXT);
            x += BT_MARK_W + SP_2;
        }
        if let Some(c) = gps {
            self.cv.icon(Icon::Location, x, cy - 9, GPS_MARK_W, c);
            x += GPS_MARK_W + SP_2;
        }
        let wifi_c = match wifi {
            WifiMark::Connected => TEXT,
            WifiMark::Failed => DANGER,
            WifiMark::Disconnected | WifiMark::Unknown => TEXT_DISABLED,
        };
        self.cv.icon(Icon::Wifi, x, cy - 10, 20, wifi_c);
        x += 20 + SP_3;
        let bc = if low { WARNING } else { TEXT };
        if charging {
            self.cv.icon(Icon::Bolt, x, cy - 8, BOLT_W + 2, SUCCESS);
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
            .icon(Icon::ChevronDown, x, cy - 8, 16, TEXT_SECONDARY);
        r
    }
}

/// A leading `CHARGING_MARK` in the battery label draws a bolt before the percent (and widens the pill);
/// the mark itself is never drawn as text.
pub const CHARGING_MARK: char = '\u{1}';
/// Width of the charging bolt in the header pill.
pub const BOLT_W: i32 = 14;
fn split_charging(label: &str) -> (bool, &str) {
    match label.strip_prefix(CHARGING_MARK) {
        Some(rest) => (true, rest),
        None => (false, label),
    }
}

/// Width of the Bluetooth mark in the header pill.
pub const BT_MARK_W: i32 = 18;

/// Width of the GPS mark in the header pill.
pub const GPS_MARK_W: i32 = 18;

/// Width of `Painter::cell_bars`.
pub const CELL_BARS_W: i32 = 18;

/// The end of `s` that fits `max_w` (long field contents stay visible at the cursor).
fn tail_fit(fonts: &mut Fonts, s: &str, max_w: f32) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        let t: String = chars[start..].iter().collect();
        if fonts.measure(&t, Face::Regular, SIZE_BODY) <= max_w {
            return t;
        }
        start += 1;
    }
    String::new()
}

// ------------------------------------------------------------------ on-screen keyboard
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardLayer {
    Lower,
    Upper,
    Symbols,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Shift,
    Backspace,
    /// Switch between letters and symbols.
    Layer,
    Space,
    Enter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBtn {
    pub rect: Rect,
    pub key: Key,
}

const KEY_W: i32 = 72;
const KEY_H: i32 = 64;
const KEY_GAP: i32 = 6;

/// Key layout: rows of up to 10 keys of 72x64 with 6 px gaps, plus shift, 123, space, backspace.
pub fn keyboard_layout(area: Rect, layer: KeyboardLayer) -> Vec<KeyBtn> {
    let rows: [&str; 3] = match layer {
        KeyboardLayer::Lower => ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
        KeyboardLayer::Upper => ["QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"],
        KeyboardLayer::Symbols => ["1234567890", "-/:;()$&@\"", ".,?!'#%"],
    };
    let mut out = Vec::new();
    let unit = KEY_W + KEY_GAP;
    let row_w = |n: i32| n * unit - KEY_GAP;
    let top = area.y;
    let row_y = |i: i32| top + i * (KEY_H + KEY_GAP);
    for (ri, row) in rows.iter().enumerate() {
        let n = row.chars().count() as i32;
        let special = ri == 2;
        let total = row_w(n) + if special { 2 * (unit + KEY_W / 2) } else { 0 };
        let mut x = area.x + (area.w - total) / 2;
        if special {
            let w = KEY_W + KEY_W / 2;
            out.push(KeyBtn {
                rect: Rect::new(x, row_y(ri as i32), w, KEY_H),
                key: if layer == KeyboardLayer::Symbols {
                    Key::Char('=')
                } else {
                    Key::Shift
                },
            });
            x += w + KEY_GAP;
        }
        for ch in row.chars() {
            out.push(KeyBtn {
                rect: Rect::new(x, row_y(ri as i32), KEY_W, KEY_H),
                key: Key::Char(ch),
            });
            x += unit;
        }
        if special {
            out.push(KeyBtn {
                rect: Rect::new(x, row_y(ri as i32), KEY_W + KEY_W / 2, KEY_H),
                key: Key::Backspace,
            });
        }
    }
    // bottom row: 123 / ABC, space, enter
    let y = row_y(3);
    let total = row_w(10);
    let x0 = area.x + (area.w - total) / 2;
    let side = 2 * unit - KEY_GAP;
    out.push(KeyBtn {
        rect: Rect::new(x0, y, side, KEY_H),
        key: Key::Layer,
    });
    out.push(KeyBtn {
        rect: Rect::new(
            x0 + side + KEY_GAP,
            y,
            total - 2 * side - 2 * KEY_GAP,
            KEY_H,
        ),
        key: Key::Space,
    });
    out.push(KeyBtn {
        rect: Rect::new(x0 + total - side, y, side, KEY_H),
        key: Key::Enter,
    });
    out
}

impl Painter<'_> {
    /// Draws the keyboard and returns the key hit rects. `pressed` = index into the returned vec.
    pub fn keyboard(
        &mut self,
        area: Rect,
        layer: KeyboardLayer,
        pressed: Option<usize>,
    ) -> Vec<KeyBtn> {
        let keys = keyboard_layout(area, layer);
        for (i, k) in keys.iter().enumerate() {
            let special = !matches!(k.key, Key::Char(_) | Key::Space);
            let down = pressed == Some(i);
            // a pressed key is filled white: the strongest feedback while typing blind-ish
            if down {
                self.cv.rrect(k.rect, RADIUS - 4, TEXT);
            } else if k.key == Key::Shift && layer == KeyboardLayer::Upper {
                // shift engaged: drawn as held
                self.outline_box(k.rect, RADIUS - 4, true, true, false);
            } else {
                self.cv.rrect(k.rect, RADIUS - 4, SURFACE_ICON);
                self.cv.rrect_stroke(
                    k.rect,
                    RADIUS - 4,
                    1.0,
                    if special { OUTLINE } else { DIVIDER },
                );
            }
            let fg = if down { BG } else { TEXT };
            let c = k.rect;
            match k.key {
                Key::Char(ch) => self.text(
                    &ch.to_string(),
                    Style::new(Face::Regular, 24.0, fg),
                    c,
                    Align::Center,
                ),
                Key::Shift => self.cv.icon(
                    Icon::Shift,
                    c.x + (c.w - ICON) / 2,
                    c.y + (c.h - ICON) / 2,
                    ICON,
                    fg,
                ),
                Key::Backspace => self.cv.icon(
                    Icon::Backspace,
                    c.x + (c.w - ICON) / 2,
                    c.y + (c.h - ICON) / 2,
                    ICON,
                    fg,
                ),
                Key::Layer => self.text(
                    if layer == KeyboardLayer::Symbols {
                        "ABC"
                    } else {
                        "123"
                    },
                    Style::new(Face::Bold, SIZE_BODY, fg),
                    c,
                    Align::Center,
                ),
                Key::Space => self.small(
                    "space",
                    c,
                    Align::Center,
                    if down { BG } else { TEXT_SECONDARY },
                ),
                Key::Enter => self.text(
                    "Done",
                    Style::new(Face::Bold, SIZE_BODY, fg),
                    c,
                    Align::Center,
                ),
            }
        }
        keys
    }
}

// ------------------------------------------------------------------ system keyboard (terminal)
/// A key of the shell's system keyboard (the on-screen keyboard for apps: terminal, browser).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysKey {
    Char(char),
    Esc,
    Tab,
    /// Sticky one-shot Control.
    Ctrl,
    /// Sticky one-shot Alt.
    Alt,
    /// The paste shortcut (Ctrl+V; Ctrl+Shift+V in a terminal).
    Paste,
    /// Sticky one-shot Shift (letters are drawn upper case while it is on).
    Shift,
    /// Letters <-> symbols.
    Layer,
    Backspace,
    Enter,
    Space,
    Up,
    Down,
    Left,
    Right,
    /// Minimise (hide) the keyboard.
    Hide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysKeyBtn {
    pub rect: Rect,
    pub key: SysKey,
}

/// What the system keyboard shows: the layer and the latched one-shot modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SysKbdState {
    pub symbols: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// Rows are measured in half key units; every row is 24 half units (12 keys) wide, so the arrow
/// keys line up as an inverted T at the right edge.
const SYS_HALF_UNITS: i32 = 24;
pub const SYS_ROWS: i32 = 5;

fn sys_rows(symbols: bool) -> [Vec<(SysKey, i32)>; 5] {
    use SysKey::*;
    let chars = |s: &str| s.chars().map(|c| (Char(c), 2)).collect::<Vec<_>>();
    let mut r0 = vec![(Esc, 2), (Tab, 2)];
    r0.extend(chars("1234567890"));
    let mut r1 = chars(if symbols { "!@#$%^&*()" } else { "qwertyuiop" });
    r1.push((Backspace, 4));
    let mut r2 = vec![(Ctrl, 3)];
    r2.extend(chars(if symbols { "-_=+[]{}|" } else { "asdfghjkl" }));
    r2.push((Enter, 3));
    let r3 = if symbols {
        let mut r = chars(r#"<>`~\;:'","#);
        r.extend([(Up, 2), (Char('?'), 2)]);
        r
    } else {
        let mut r = vec![(Shift, 4)];
        r.extend(chars("zxcvbnm."));
        r.extend([(Up, 2), (Char('/'), 2)]);
        r
    };
    let r4 = vec![
        (Layer, 3),
        (Hide, 3),
        (Alt, 3),
        (Space, 6),
        (Paste, 3),
        (Left, 2),
        (Down, 2),
        (Right, 2),
    ];
    [r0, r1, r2, r3, r4]
}

/// Padding and gap for a keyboard of height `h` (the landscape keyboard is compact).
fn sys_spacing(h: i32) -> (i32, i32) {
    if h >= 300 {
        (SP_2, 6)
    } else {
        (6, 4)
    }
}

/// Key rectangles for the system keyboard filling `area` (5 rows, full width).
pub fn system_keyboard_layout(area: Rect, symbols: bool) -> Vec<SysKeyBtn> {
    let (pad, gap) = sys_spacing(area.h);
    let key_h = (area.h - 2 * pad - (SYS_ROWS - 1) * gap) / SYS_ROWS;
    // the leftover pixels go above the first row, so the bottom row sits `pad` from the edge
    let top = area.bottom() - pad - SYS_ROWS * key_h - (SYS_ROWS - 1) * gap;
    let span = area.w - 2 * pad + gap;
    let x_at = |hu: i32| area.x + pad + hu * span / SYS_HALF_UNITS;
    let mut out = Vec::new();
    for (ri, row) in sys_rows(symbols).iter().enumerate() {
        let y = top + ri as i32 * (key_h + gap);
        let mut hu = 0;
        for (key, units) in row {
            let (x0, x1) = (x_at(hu), x_at(hu + units));
            out.push(SysKeyBtn {
                rect: Rect::new(x0, y, x1 - x0 - gap, key_h),
                key: *key,
            });
            hu += units;
        }
    }
    out
}

/// The character a `Char` key types in this state (letters follow Shift).
pub fn sys_key_char(c: char, st: SysKbdState) -> char {
    if st.shift && c.is_ascii_lowercase() {
        c.to_ascii_uppercase()
    } else {
        c
    }
}

impl Painter<'_> {
    /// Draws the system keyboard over the whole canvas and returns the key hit rects.
    /// `pressed` = index into the returned vec.
    pub fn system_keyboard(&mut self, st: SysKbdState, pressed: Option<usize>) -> Vec<SysKeyBtn> {
        let area = self.cv.bounds();
        // its own window over other apps (no compositor, nothing to blend with): the strip is
        // SURFACE_STRONG over BG flattened into one opaque fill; the keys blend over it
        self.cv.rect(area, SURFACE_STRONG.over(BG));
        self.divider(area.x, area.y, area.w);
        let keys = system_keyboard_layout(area, st.symbols);
        let compact = area.h < 300;
        let char_px = if compact { 20.0 } else { 24.0 };
        let label_px = if compact { SIZE_SECONDARY } else { SIZE_BODY };
        for (i, k) in keys.iter().enumerate() {
            let latched = match k.key {
                SysKey::Shift => st.shift,
                SysKey::Ctrl => st.ctrl,
                SysKey::Alt => st.alt,
                _ => false,
            };
            let down = pressed == Some(i);
            // surface fill; pressed: + white 12 % and a white outline; latched modifier: white outline
            self.outline_box(k.rect, RADIUS - 4, down || latched, latched, false);
            let c = k.rect;
            let label = |s: &'static str| (s, Face::Bold, label_px);
            let text: Option<(&str, Face, f32)> = match k.key {
                SysKey::Char(_) | SysKey::Shift | SysKey::Backspace | SysKey::Hide => None,
                SysKey::Esc => Some(label("Esc")),
                SysKey::Tab => Some(label("Tab")),
                SysKey::Ctrl => Some(label("Ctrl")),
                SysKey::Alt => Some(label("Alt")),
                SysKey::Paste => Some(label("Paste")),
                SysKey::Layer => Some(label(if st.symbols { "ABC" } else { "?123" })),
                SysKey::Enter => Some(label("Enter")),
                SysKey::Space => Some(("space", Face::Regular, SIZE_SECONDARY)),
                SysKey::Up => Some(("\u{2191}", Face::Regular, char_px)),
                SysKey::Down => Some(("\u{2193}", Face::Regular, char_px)),
                SysKey::Left => Some(("\u{2190}", Face::Regular, char_px)),
                SysKey::Right => Some(("\u{2192}", Face::Regular, char_px)),
            };
            let icon_at = |w: i32| (c.x + (c.w - w) / 2, c.y + (c.h - ICON) / 2);
            match (k.key, text) {
                (SysKey::Char(ch), _) => self.text(
                    &sys_key_char(ch, st).to_string(),
                    Style::new(Face::Regular, char_px, TEXT),
                    c,
                    Align::Center,
                ),
                (SysKey::Shift, _) => {
                    let (x, y) = icon_at(ICON);
                    self.cv.icon(Icon::Shift, x, y, ICON, TEXT);
                }
                (SysKey::Backspace, _) => {
                    let (x, y) = icon_at(ICON);
                    self.cv.icon(Icon::Backspace, x, y, ICON, TEXT);
                }
                (SysKey::Hide, _) => {
                    // keyboard with a chevron down: "put the keyboard away"
                    let (x, y) = icon_at(ICON + 2 + 16);
                    self.cv.icon(Icon::Keyboard, x, y, ICON, TEXT);
                    self.cv
                        .icon(Icon::ChevronDown, x + ICON + 2, y + 4, 16, TEXT);
                }
                (_, Some((s, face, px))) => {
                    let colour = if k.key == SysKey::Space {
                        TEXT_SECONDARY
                    } else {
                        TEXT
                    };
                    self.text(s, Style::new(face, px, colour), c, Align::Center)
                }
                (_, None) => {}
            }
        }
        keys
    }
}

/// The page surface rect for a content area (frame.content): inset by `PAGE_MARGIN` at the sides
/// and the bottom, flush with the header at the top.
pub fn page_rect(content: Rect) -> Rect {
    Rect::new(
        content.x + PAGE_MARGIN,
        content.y,
        content.w - 2 * PAGE_MARGIN,
        content.h - PAGE_MARGIN,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_fits_800_and_keys_are_big_enough() {
        for layer in [
            KeyboardLayer::Lower,
            KeyboardLayer::Upper,
            KeyboardLayer::Symbols,
        ] {
            let area = Rect::new(0, 900, 800, 280);
            let keys = keyboard_layout(area, layer);
            assert!(
                keys.iter().all(|k| k.rect.x >= 0 && k.rect.right() <= 800),
                "{layer:?} stays on screen"
            );
            assert!(
                keys.iter()
                    .all(|k| k.rect.h >= TOUCH_MIN && k.rect.w >= TOUCH_MIN),
                "{layer:?} touch targets"
            );
            assert!(
                keys.iter().any(|k| k.key == Key::Space)
                    && keys.iter().any(|k| k.key == Key::Backspace)
                    && keys.iter().any(|k| k.key == Key::Layer)
            );
            // no overlaps
            for (i, a) in keys.iter().enumerate() {
                for b in &keys[i + 1..] {
                    let overlap = a.rect.x < b.rect.right()
                        && b.rect.x < a.rect.right()
                        && a.rect.y < b.rect.bottom()
                        && b.rect.y < a.rect.bottom();
                    assert!(!overlap, "{layer:?}: {:?} overlaps {:?}", a.key, b.key);
                }
            }
        }
        let lower = keyboard_layout(Rect::new(0, 0, 800, 280), KeyboardLayer::Lower);
        assert_eq!(
            lower
                .iter()
                .filter(|k| matches!(k.key, Key::Char(_)))
                .count(),
            26
        );
        let top_row: Vec<_> = lower.iter().filter(|k| k.rect.y == 0).collect();
        assert_eq!(top_row.len(), 10);
        assert_eq!(
            (top_row[1].rect.x - top_row[0].rect.right()),
            6,
            "6 px gaps"
        );
    }

    #[test]
    fn row_runs_merge_adjacent_grouped_rows_only() {
        let r = |y, h| Rect::new(0, y, 100, h);
        let runs = row_runs([
            (r(0, 64), true),
            (r(64, 64), true),
            (r(128, 56), false),
            (r(184, 64), true),
            (r(260, 64), true), // gap above: a new run
        ]);
        assert_eq!(runs, vec![r(0, 128), r(184, 64), r(260, 64)]);
        assert!(row_runs([(r(0, 10), false)]).is_empty());
    }

    #[test]
    fn tabs_are_in_spec_order() {
        let l: Vec<&str> = Tab::ALL.iter().map(|t| t.label()).collect();
        assert_eq!(
            l,
            [
                "Home",
                "Apps",
                "Network",
                "Device",
                "Settings",
                "Mobile network",
                "Bluetooth",
                "VPN and DNS",
                "Media",
                "Phone",
                "GPS",
                "Notifications",
            ]
        );
    }
}
