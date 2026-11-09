//! Application state and rendering. M5.3 draws the frame (status bar, bottom navigation, content
//! placeholder) and switches tabs on tap; the real screens arrive in M5.4-M5.7.
use crate::gesture::Gesture;
use walpad_ui::components::{Painter, Tab};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Rect, Style};

pub struct App {
    pub frame: Frame,
    pub active: Tab,
    nav_hits: Vec<Rect>,
}

impl App {
    pub fn new(w: i32, h: i32) -> App {
        App {
            frame: Frame::new(w, h),
            active: Tab::Home,
            nav_hits: Vec::new(),
        }
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        self.frame = Frame::new(w, h);
        self.nav_hits.clear();
    }

    /// Returns true when the screen must be redrawn.
    pub fn on_gesture(&mut self, g: &Gesture) -> bool {
        if let Gesture::Tap { x, y } = g {
            if let Some(i) = self.nav_hits.iter().position(|r| r.contains(*x, *y)) {
                let tab = Tab::ALL[i];
                if tab != self.active {
                    self.active = tab;
                    return true;
                }
            }
        }
        false
    }

    pub fn render(&mut self, p: &mut Painter) {
        p.cv.fill(BG);
        p.status_bar(self.frame.status, None, None, false, false);
        let c = self.frame.padded();
        p.screen_title(self.active.label(), Rect::new(c.x, c.y + SP_4, c.w, 40));
        p.text(
            "WalPad Shell skeleton (M5.3)",
            Style::new(Face::Regular, SIZE_SECONDARY, TEXT_SECONDARY),
            Rect::new(c.x, c.y + 64, c.w, 24),
            Align::Left,
        );
        self.nav_hits = p.bottom_nav(self.frame.nav, self.active);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use walpad_ui::{Canvas, Fonts};

    fn render(app: &mut App) -> Canvas {
        let mut f =
            Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"))
                .unwrap();
        let mut c = Canvas::new(app.frame.w as u32, app.frame.h as u32, BG).unwrap();
        app.render(&mut Painter {
            cv: &mut c,
            fonts: &mut f,
        });
        c
    }

    #[test]
    fn tap_on_a_nav_item_switches_tab_and_taps_elsewhere_do_not() {
        let mut app = App::new(800, 1280);
        render(&mut app);
        assert_eq!(app.active, Tab::Home);
        assert!(
            !app.on_gesture(&Gesture::Tap { x: 400, y: 600 }),
            "content tap: nothing"
        );
        assert!(
            app.on_gesture(&Gesture::Tap { x: 400, y: 1240 }),
            "centre of the nav bar = Network"
        );
        assert_eq!(app.active, Tab::Network);
        assert!(
            !app.on_gesture(&Gesture::Tap { x: 400, y: 1240 }),
            "same tab: no redraw"
        );
        assert!(app.on_gesture(&Gesture::Tap { x: 790, y: 1240 }));
        assert_eq!(app.active, Tab::Settings);
    }

    #[test]
    fn layout_follows_the_orientation() {
        let mut app = App::new(800, 1280);
        render(&mut app);
        assert_eq!(app.frame.nav.y, 1192);
        app.resize(1280, 800);
        render(&mut app);
        assert_eq!((app.frame.nav.y, app.frame.nav.w), (712, 1280));
        assert!(app.on_gesture(&Gesture::Tap {
            x: 1280 / 5 * 3 + 10,
            y: 760
        }));
        assert_eq!(app.active, Tab::Device);
    }
}
