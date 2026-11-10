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
}
impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Home,
        Tab::Apps,
        Tab::Network,
        Tab::Device,
        Tab::Settings,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Tab::Home => "Home",
            Tab::Apps => "Apps",
            Tab::Network => "Network",
            Tab::Device => "Device",
            Tab::Settings => "Settings",
        }
    }
    pub fn icon(self) -> Icon {
        match self {
            Tab::Home => Icon::Home,
            Tab::Apps => Icon::Apps,
            Tab::Network => Icon::Wifi,
            Tab::Device => Icon::Device,
            Tab::Settings => Icon::Settings,
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

/// Background of a banner: its tone at 14 % over the charcoal (opaque).
pub fn banner_fill(t: Tone) -> Color {
    BG.mix(
        match t {
            Tone::Info => TEXT,
            other => tone_color(other),
        },
        14,
    )
}

const DANGER_FILL: Color = BG.mix(DANGER, 16);

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
    /// The basic line-art control: a rounded outline, filled only while pressed. `active` marks
    /// the current destination (full-white outline without the fill).
    pub fn outline_box(
        &mut self,
        r: Rect,
        radius: i32,
        pressed: bool,
        active: bool,
        disabled: bool,
    ) {
        if pressed && !disabled {
            self.cv.rrect(r, radius, PRESSED_FILL);
        }
        let c = if disabled {
            DIVIDER
        } else if pressed || active {
            TEXT
        } else {
            OUTLINE
        };
        self.cv.rrect_stroke(r, radius, OUTLINE_W, c);
    }

    /// Rounded-square icon button (sidebar 56, Home grid 64, Apps panel 48).
    pub fn icon_box(&mut self, r: Rect, icon: Icon, pressed: bool, active: bool) {
        self.outline_box(r, RADIUS_ICON.min(r.w / 3), pressed, active, false);
        let s = icon_for_box(r.w);
        self.cv
            .icon(icon, r.x + (r.w - s) / 2, r.y + (r.h - s) / 2, s, TEXT);
    }

    /// Round outlined button with an icon (the sidebar's Apps toggle).
    pub fn round_button(&mut self, r: Rect, icon: Icon, pressed: bool, active: bool) {
        self.outline_box(r, r.w / 2, pressed, active, false);
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

    /// Card container: 1 px border, transparent inside (pressed: 10 % fill).
    pub fn card(&mut self, r: Rect, pressed: bool) {
        if pressed {
            self.cv.rrect(r, RADIUS_CARD, PRESSED_FILL);
        }
        self.cv.rrect_stroke(r, RADIUS_CARD, 1.0, CARD_BORDER);
    }
    /// Popover / dropdown / dialog body: background fill (hides what is below) and a 1 px border.
    pub fn popover(&mut self, r: Rect) {
        self.cv.rrect(r, RADIUS_CARD, BG);
        self.cv.rrect_stroke(r, RADIUS_CARD, 1.0, CARD_BORDER);
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
        let (fill, line, fg) = match (kind, disabled) {
            (_, true) => (None, DIVIDER, TEXT_DISABLED),
            (ButtonKind::Primary, _) => (pressed.then_some(PRESSED_FILL), TEXT, TEXT),
            (ButtonKind::Danger, _) => (
                Some(if pressed {
                    BG.mix(DANGER, 30)
                } else {
                    DANGER_FILL
                }),
                DANGER,
                DANGER,
            ),
            (ButtonKind::Secondary, _) => (
                pressed.then_some(PRESSED_FILL),
                if pressed { TEXT } else { OUTLINE },
                TEXT,
            ),
        };
        if let Some(f) = fill {
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
        self.cv
            .rrect_stroke(r, RADIUS, 1.0, if disabled { DIVIDER } else { OUTLINE });
        let n = labels.len().max(1) as i32;
        let w = (r.w - 8) / n;
        let mut out = Vec::new();
        for (i, l) in labels.iter().enumerate() {
            let seg = Rect::new(r.x + 4 + i as i32 * w, r.y + 4, w, r.h - 8);
            let sel = i == selected;
            if sel {
                self.cv.rrect(seg, RADIUS - 4, PRESSED_FILL);
                self.cv.rrect_stroke(
                    seg,
                    RADIUS - 4,
                    OUTLINE_W,
                    if disabled { TEXT_DISABLED } else { TEXT },
                );
            } else if pressed == Some(i) && !disabled {
                self.cv.rrect(seg, RADIUS - 4, PRESSED_FILL);
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
    /// Pressed highlight of a list row: a rounded 10 % fill inset from the row edges.
    pub fn row_pressed(&mut self, r: Rect) {
        self.cv.rrect(
            Rect::new(r.x + 4, r.y + 3, r.w - 8, r.h - 6),
            RADIUS - 2,
            PRESSED_FILL,
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
        self.cv.rrect(r, 24, BG);
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
        self.cv.rrect(r, RADIUS, banner_fill(tone));
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
        let bw = self
            .fonts
            .measure(battery, Face::Regular, SIZE_SECONDARY)
            .ceil() as i32;
        14 + 20 + SP_3 + bw + SP_1 + 22 + SP_2 + 16 + 14
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
        let w = self.quick_pill_w(battery);
        let r = Rect::new(right - w, cy - 20, w, 40);
        self.outline_box(r, 20, pressed, open, false);
        let mut x = r.x + 14;
        let wifi_c = match wifi {
            WifiMark::Connected => TEXT,
            WifiMark::Failed => DANGER,
            WifiMark::Disconnected | WifiMark::Unknown => TEXT_DISABLED,
        };
        self.cv.icon(Icon::Wifi, x, cy - 10, 20, wifi_c);
        x += 20 + SP_3;
        let bc = if low { WARNING } else { TEXT };
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
    fn tabs_are_in_spec_order() {
        let l: Vec<&str> = Tab::ALL.iter().map(|t| t.label()).collect();
        assert_eq!(l, ["Home", "Apps", "Network", "Device", "Settings"]);
    }
}
