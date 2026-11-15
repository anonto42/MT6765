//! Text helpers, surface boxes, icon tiles, cards, and section headers for [`Painter`].
use super::{icon_for_box, page_rect, Painter};
use crate::canvas::Rect;
use crate::icons::Icon;
use crate::text::{Align, Face, Style};
use crate::tokens::*;

impl Painter<'_> {
    pub fn text(&mut self, s: &str, style: Style, r: Rect, align: Align) {
        self.fonts.draw_in(self.cv, s, style, r, align);
    }
    pub(super) fn body(&mut self, s: &str, r: Rect, align: Align, color: Color) {
        self.text(s, Style::new(Face::Regular, SIZE_BODY, color), r, align);
    }
    pub(super) fn small(&mut self, s: &str, r: Rect, align: Align, color: Color) {
        self.text(
            s,
            Style::new(Face::Regular, SIZE_SECONDARY, color),
            r,
            align,
        );
    }
    pub(super) fn caption(&mut self, s: &str, r: Rect, align: Align, color: Color) {
        self.text(s, Style::new(Face::Regular, SIZE_CAPTION, color), r, align);
    }

    // ------------------------------------------------------------------ surfaces & boxes
    /// The basic boxy control: a translucent `SURFACE` fill (blended over the wallpaper) with sharp
    /// 0 px corners and no box outline; pressed adds `PRESSED_TINT`. `active` marks the current
    /// destination with a stronger fill and a 2 px left indicator bar.
    pub fn outline_box(
        &mut self,
        r: Rect,
        radius: i32,
        pressed: bool,
        active: bool,
        disabled: bool,
    ) {
        self.surface_box(r, radius, surface(), pressed, active, disabled);
    }

    /// `outline_box` with an explicit surface fill (`SURFACE_ICON` for icon boxes). Fill only, no
    /// box outline (owner request 2026-11-12: 0 px radius, no borders on app icons, dropdowns or
    /// other controls); `active` reads through a stronger fill + 2 px indicator bar.
    pub fn surface_box(
        &mut self,
        r: Rect,
        _radius: i32,
        fill: Tint,
        pressed: bool,
        active: bool,
        disabled: bool,
    ) {
        self.cv.rect(r, fill);
        if active && !disabled {
            self.cv.rect(r, surface_strong());
            self.cv.rect(Rect::new(r.x, r.y, 2, r.h), text());
        }
        if pressed && !disabled {
            self.cv.rect(r, pressed_tint());
        }
    }

    /// Sharp square icon button (sidebar 56, Home grid 64, Apps panel 48).
    pub fn icon_box(&mut self, r: Rect, icon: Icon, pressed: bool, active: bool) {
        self.surface_box(r, 0, surface_icon(), pressed, active, false);
        let s = icon_for_box(r.w);
        self.cv
            .icon(icon, r.x + (r.w - s) / 2, r.y + (r.h - s) / 2, s, text());
    }

    /// Square icon button with strong backing (the sidebar's Apps toggle). Sharp 0 px corners, fill
    /// only, no outline (owner request 2026-11-12).
    pub fn round_button(&mut self, r: Rect, icon: Icon, pressed: bool, active: bool) {
        self.surface_box(r, 0, surface_strong(), pressed, active, false);
        self.cv.icon(
            icon,
            r.x + (r.w - ICON) / 2,
            r.y + (r.h - ICON) / 2,
            ICON,
            text(),
        );
    }

    /// An app in a grid cell: `box_px` icon box centred at the top of `cell`, the name under it.
    /// A running app has a small square indicator in the box's top-right corner.
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
                .rect(Rect::new(b.right() - 10, b.y + 4, 6, 6), text());
        }
        let px = if box_px >= BOX_GRID {
            SIZE_LABEL
        } else {
            SIZE_TINY
        };
        self.text(
            name,
            Style::new(Face::Regular, px, text_label()),
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

    /// Card container: translucent `SURFACE` fill only (pressed: + `PRESSED_TINT`), 0 px radius, no
    /// outline.
    pub fn card(&mut self, r: Rect, pressed: bool) {
        self.cv.rect(r, surface());
        if pressed {
            self.cv.rect(r, pressed_tint());
        }
    }
    /// The page surface of a full-screen page: `PAGE_SURFACE` over the wallpaper, flush to the
    /// content area with 0 px radius and no outline.
    pub fn page_surface(&mut self, content: Rect) {
        let r = page_rect(content);
        self.cv.rect(r, page_surface());
    }
    /// The backing of a run of list rows (Settings, Device): a card around them, so rows over the
    /// wallpaper art stay readable. The rows themselves add no fill (only the pressed tint).
    pub fn row_group(&mut self, r: Rect) {
        if self.cv.visible(r) {
            self.card(r, false);
        }
    }
    /// Popover / dropdown / dialog / sheet body: opaque `SURFACE_STRONG.over(BG)` fill with 0 px
    /// radius and no box outline, preventing underlying icons or text from bleeding through.
    pub fn popover(&mut self, r: Rect) {
        self.cv.rect(r, surface_strong().over(bg()));
    }
    /// 1 px horizontal divider.
    pub fn divider(&mut self, x: i32, y: i32, w: i32) {
        self.cv.rect(Rect::new(x, y, w, 1), divider());
    }
    /// Section label: medium-small, secondary colour, upper case.
    pub fn section_label(&mut self, s: &str, r: Rect) {
        self.text(
            &s.to_uppercase(),
            Style::new(Face::Bold, SIZE_SECTION, text_secondary()),
            r,
            Align::Left,
        );
    }
    /// Tiny upper-case caption ("ACTIVE APPS").
    pub fn caption_upper(&mut self, s: &str, r: Rect) {
        self.text(
            &s.to_uppercase(),
            Style::new(Face::Bold, SIZE_TINY, text_secondary()),
            r,
            Align::Left,
        );
    }
    pub fn screen_title(&mut self, s: &str, r: Rect) {
        self.text(
            s,
            Style::new(Face::Regular, SIZE_TITLE, text()),
            r,
            Align::Left,
        );
    }
}
