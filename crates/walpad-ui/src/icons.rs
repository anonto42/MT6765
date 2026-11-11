//! Vector icons on a 24 px grid with 2 px round strokes, drawn in code (no image assets).
use crate::canvas::{rounded_rect_f, Canvas};
use crate::tokens::Color;
use tiny_skia::{Path, PathBuilder, Transform};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Home,
    Apps,
    Wifi,
    Device,
    Settings,
    Globe,
    Terminal,
    Folder,
    Keyboard,
    Sun,
    Rotate,
    Power,
    Battery,
    Lock,
    ChevronRight,
    ChevronDown,
    Bluetooth,
    Check,
    Close,
    Refresh,
    Screen,
    Back,
    Warning,
    Backspace,
    Shift,
    Eye,
    EyeOff,
    Spinner,
    /// The mouse pad tool (Apps panel > Tools).
    Mouse,
    /// SIM card (mobile network, owner request 2026-11-10).
    Sim,
    /// Speaker with sound waves / crossed out (volume, owner request 2026-11-10).
    Volume,
    VolumeOff,
    /// Clipboard (Apps panel > Tools).
    Clipboard,
    /// Media screen (change media-apps-capture): film frame with a play triangle, a picture, a note.
    Media,
    Image,
    Music,
    /// Phone handset, speech bubble, person, microphone (muted), dial pad (owner request
    /// 2026-11-11, `phone-dialer`).
    Phone,
    Message,
    Person,
    Mic,
    MicOff,
    Keypad,
    /// Map pin (GPS, owner request 2026-11-11).
    Location,
}

impl Icon {
    pub const ALL: [Icon; 43] = [
        Icon::Home,
        Icon::Apps,
        Icon::Wifi,
        Icon::Device,
        Icon::Settings,
        Icon::Globe,
        Icon::Terminal,
        Icon::Folder,
        Icon::Keyboard,
        Icon::Sun,
        Icon::Rotate,
        Icon::Power,
        Icon::Battery,
        Icon::Lock,
        Icon::ChevronRight,
        Icon::ChevronDown,
        Icon::Bluetooth,
        Icon::Check,
        Icon::Close,
        Icon::Refresh,
        Icon::Screen,
        Icon::Back,
        Icon::Warning,
        Icon::Backspace,
        Icon::Shift,
        Icon::Eye,
        Icon::EyeOff,
        Icon::Spinner,
        Icon::Mouse,
        Icon::Sim,
        Icon::Volume,
        Icon::VolumeOff,
        Icon::Clipboard,
        Icon::Media,
        Icon::Image,
        Icon::Music,
        Icon::Phone,
        Icon::Message,
        Icon::Person,
        Icon::Mic,
        Icon::MicOff,
        Icon::Keypad,
        Icon::Location,
    ];

    /// Icon id used in apps.json (`"icon": "globe"`).
    pub fn from_name(n: &str) -> Option<Icon> {
        Some(match n {
            "home" => Icon::Home,
            "apps" => Icon::Apps,
            "wifi" => Icon::Wifi,
            "device" => Icon::Device,
            "settings" => Icon::Settings,
            "globe" => Icon::Globe,
            "terminal" => Icon::Terminal,
            "folder" | "files" => Icon::Folder,
            "keyboard" => Icon::Keyboard,
            "bluetooth" => Icon::Bluetooth,
            "mouse" => Icon::Mouse,
            "sim" => Icon::Sim,
            "volume" => Icon::Volume,
            "clipboard" => Icon::Clipboard,
            "location" | "gps" => Icon::Location,
            "media" | "video" => Icon::Media,
            "image" | "photos" => Icon::Image,
            "music" => Icon::Music,
            "phone" => Icon::Phone,
            "message" | "messages" => Icon::Message,
            _ => return None,
        })
    }
}

/// Collects stroked subpaths and filled dots in grid coordinates.
struct Pen {
    stroke: PathBuilder,
    dots: PathBuilder,
}

impl Pen {
    fn new() -> Self {
        Pen {
            stroke: PathBuilder::new(),
            dots: PathBuilder::new(),
        }
    }
    fn line(&mut self, a: (f32, f32), b: (f32, f32)) {
        self.stroke.move_to(a.0, a.1);
        self.stroke.line_to(b.0, b.1);
    }
    fn poly(&mut self, pts: &[(f32, f32)], closed: bool) {
        if let Some((first, rest)) = pts.split_first() {
            self.stroke.move_to(first.0, first.1);
            for p in rest {
                self.stroke.line_to(p.0, p.1);
            }
            if closed {
                self.stroke.close();
            }
        }
    }
    fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32) {
        if let Some(p) = rounded_rect_f(x, y, w, h, r) {
            self.stroke.push_path(&p);
        }
    }
    fn circle(&mut self, cx: f32, cy: f32, r: f32) {
        self.stroke.push_circle(cx, cy, r);
    }
    fn dot(&mut self, cx: f32, cy: f32, r: f32) {
        self.dots.push_circle(cx, cy, r);
    }
    /// Angles in degrees: 0 = right, 90 = down.
    fn arc(&mut self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) {
        let n = ((a1 - a0).abs() / 8.0).ceil().max(4.0) as usize;
        for i in 0..=n {
            let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
            let (x, y) = (cx + r * a.cos(), cy + r * a.sin());
            if i == 0 {
                self.stroke.move_to(x, y);
            } else {
                self.stroke.line_to(x, y);
            }
        }
    }
    fn quad(&mut self, from: (f32, f32), ctl: (f32, f32), to: (f32, f32)) {
        self.stroke.move_to(from.0, from.1);
        self.stroke.quad_to(ctl.0, ctl.1, to.0, to.1);
    }
    fn quad_to(&mut self, ctl: (f32, f32), to: (f32, f32)) {
        self.stroke.quad_to(ctl.0, ctl.1, to.0, to.1);
    }
    fn rays(&mut self, cx: f32, cy: f32, r0: f32, r1: f32, n: usize) {
        for i in 0..n {
            let a = (360.0 * i as f32 / n as f32).to_radians();
            self.line(
                (cx + r0 * a.cos(), cy + r0 * a.sin()),
                (cx + r1 * a.cos(), cy + r1 * a.sin()),
            );
        }
    }
    fn arrow_head(&mut self, tip: (f32, f32), a: (f32, f32), b: (f32, f32)) {
        self.poly(&[a, tip, b], false);
    }
}

fn build(icon: Icon) -> (Option<Path>, Option<Path>) {
    let mut p = Pen::new();
    match icon {
        Icon::Home => {
            p.poly(&[(3.0, 11.0), (12.0, 3.0), (21.0, 11.0)], false);
            p.poly(
                &[(5.0, 10.0), (5.0, 21.0), (19.0, 21.0), (19.0, 10.0)],
                false,
            );
            p.poly(
                &[(10.0, 21.0), (10.0, 14.0), (14.0, 14.0), (14.0, 21.0)],
                false,
            );
        }
        Icon::Apps => {
            for (x, y) in [(4.0, 4.0), (14.0, 4.0), (4.0, 14.0), (14.0, 14.0)] {
                p.rrect(x, y, 6.0, 6.0, 1.5);
            }
        }
        Icon::Wifi => {
            for r in [15.0, 10.0, 5.0] {
                p.arc(12.0, 19.0, r, -135.0, -45.0);
            }
            p.dot(12.0, 19.0, 1.6);
        }
        Icon::Device => {
            p.rrect(5.0, 2.0, 14.0, 20.0, 2.5);
            p.line((10.5, 18.5), (13.5, 18.5));
        }
        Icon::Settings => {
            p.circle(12.0, 12.0, 3.0);
            p.circle(12.0, 12.0, 7.0);
            p.rays(12.0, 12.0, 7.5, 10.0, 8);
        }
        Icon::Globe => {
            p.circle(12.0, 12.0, 9.0);
            p.line((3.0, 12.0), (21.0, 12.0));
            p.quad((12.0, 3.0), (6.0, 12.0), (12.0, 21.0));
            p.quad((12.0, 3.0), (18.0, 12.0), (12.0, 21.0));
        }
        Icon::Terminal => {
            p.rrect(3.0, 4.0, 18.0, 16.0, 2.0);
            p.poly(&[(7.0, 9.0), (10.0, 12.0), (7.0, 15.0)], false);
            p.line((12.0, 15.0), (17.0, 15.0));
        }
        Icon::Folder => p.poly(
            &[
                (3.0, 7.0),
                (3.0, 19.0),
                (21.0, 19.0),
                (21.0, 9.0),
                (11.0, 9.0),
                (9.0, 7.0),
            ],
            true,
        ),
        Icon::Keyboard => {
            p.rrect(2.0, 6.0, 20.0, 12.0, 2.0);
            for x in [6.0, 10.0, 14.0, 18.0] {
                p.line((x, 10.0), (x + 0.5, 10.0));
            }
            p.line((7.0, 14.0), (17.0, 14.0));
        }
        Icon::Sun => {
            p.circle(12.0, 12.0, 4.0);
            p.rays(12.0, 12.0, 7.0, 10.0, 8);
        }
        Icon::Rotate => {
            p.arc(12.0, 12.0, 8.0, -60.0, 200.0);
            p.arrow_head((4.0, 12.5), (2.0, 8.5), (7.5, 9.5));
        }
        Icon::Power => {
            p.arc(12.0, 13.0, 8.0, -50.0, 230.0);
            p.line((12.0, 3.0), (12.0, 12.0));
        }
        Icon::Battery => {
            p.rrect(2.0, 7.0, 18.0, 10.0, 2.0);
            p.line((22.5, 10.5), (22.5, 13.5));
        }
        Icon::Lock => {
            p.rrect(5.0, 11.0, 14.0, 10.0, 2.0);
            p.poly(&[(8.0, 11.0), (8.0, 8.0)], false);
            p.arc(12.0, 8.0, 4.0, 180.0, 360.0);
            p.line((16.0, 8.0), (16.0, 11.0));
        }
        Icon::ChevronRight => p.poly(&[(9.0, 6.0), (15.0, 12.0), (9.0, 18.0)], false),
        Icon::ChevronDown => p.poly(&[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)], false),
        Icon::Bluetooth => p.poly(
            &[
                (6.5, 7.0),
                (17.0, 16.5),
                (12.0, 21.0),
                (12.0, 3.0),
                (17.0, 7.5),
                (6.5, 17.0),
            ],
            false,
        ),
        Icon::Check => p.poly(&[(5.0, 13.0), (10.0, 18.0), (19.0, 7.0)], false),
        Icon::Close => {
            p.line((6.0, 6.0), (18.0, 18.0));
            p.line((18.0, 6.0), (6.0, 18.0));
        }
        Icon::Refresh => {
            p.arc(12.0, 12.0, 8.0, -30.0, 240.0);
            p.arrow_head((20.0, 5.0), (14.5, 4.5), (20.5, 10.5));
        }
        Icon::Screen => {
            p.rrect(3.0, 4.0, 18.0, 12.0, 2.0);
            p.line((12.0, 16.0), (12.0, 20.0));
            p.line((8.0, 20.0), (16.0, 20.0));
        }
        Icon::Back => p.poly(&[(15.0, 6.0), (9.0, 12.0), (15.0, 18.0)], false),
        Icon::Warning => {
            p.poly(&[(12.0, 3.5), (22.0, 20.5), (2.0, 20.5)], true);
            p.line((12.0, 9.5), (12.0, 14.5));
            p.dot(12.0, 17.5, 1.2);
        }
        Icon::Backspace => {
            p.poly(
                &[
                    (21.0, 5.0),
                    (8.0, 5.0),
                    (2.0, 12.0),
                    (8.0, 19.0),
                    (21.0, 19.0),
                ],
                true,
            );
            p.line((11.0, 9.0), (17.0, 15.0));
            p.line((17.0, 9.0), (11.0, 15.0));
        }
        Icon::Shift => p.poly(
            &[
                (12.0, 4.0),
                (4.0, 13.0),
                (9.0, 13.0),
                (9.0, 20.0),
                (15.0, 20.0),
                (15.0, 13.0),
                (20.0, 13.0),
            ],
            true,
        ),
        Icon::Eye | Icon::EyeOff => {
            p.stroke.move_to(2.0, 12.0);
            p.quad_to((12.0, 3.0), (22.0, 12.0));
            p.quad_to((12.0, 21.0), (2.0, 12.0));
            p.circle(12.0, 12.0, 3.0);
            if icon == Icon::EyeOff {
                p.line((4.0, 4.0), (20.0, 20.0));
            }
        }
        Icon::Spinner => p.arc(12.0, 12.0, 9.0, -90.0, 180.0),
        Icon::Mouse => {
            p.rrect(6.0, 2.5, 12.0, 19.0, 6.0);
            p.line((12.0, 2.5), (12.0, 9.5));
            p.line((6.0, 9.5), (18.0, 9.5));
        }
        Icon::Sim => {
            // card with a clipped top-right corner and the contact chip
            p.poly(
                &[
                    (6.0, 2.5),
                    (14.5, 2.5),
                    (19.0, 7.0),
                    (19.0, 21.5),
                    (5.0, 21.5),
                    (5.0, 3.5),
                    (6.0, 2.5),
                ],
                true,
            );
            p.rrect(8.5, 11.0, 7.0, 7.0, 1.0);
            p.line((12.0, 11.0), (12.0, 18.0));
        }
        Icon::Volume => {
            speaker(&mut p);
            p.arc(11.0, 12.0, 5.0, -48.0, 48.0);
            p.arc(11.0, 12.0, 9.0, -48.0, 48.0);
        }
        Icon::VolumeOff => {
            speaker(&mut p);
            p.line((16.0, 9.0), (22.0, 15.0));
            p.line((22.0, 9.0), (16.0, 15.0));
        }
        Icon::Clipboard => {
            // board with the clip on top and two text lines
            p.rrect(5.0, 4.5, 14.0, 17.0, 2.0);
            p.rrect(9.0, 2.5, 6.0, 4.0, 1.5);
            p.line((8.5, 12.0), (15.5, 12.0));
            p.line((8.5, 16.0), (13.0, 16.0));
        }
        Icon::Media => {
            p.rrect(3.0, 5.0, 18.0, 14.0, 2.5);
            p.poly(&[(10.0, 9.0), (10.0, 15.0), (15.5, 12.0)], true);
        }
        Icon::Image => {
            p.rrect(3.0, 4.5, 18.0, 15.0, 2.5);
            p.poly(
                &[
                    (4.5, 17.5),
                    (9.5, 11.5),
                    (13.0, 15.0),
                    (15.5, 12.5),
                    (19.5, 17.5),
                ],
                false,
            );
            p.circle(16.0, 9.0, 1.8);
        }
        Icon::Music => {
            p.poly(&[(9.0, 18.0), (9.0, 5.0), (19.0, 3.0), (19.0, 16.0)], false);
            p.line((9.0, 9.0), (19.0, 7.0));
            p.circle(6.5, 18.0, 2.5);
            p.circle(16.5, 16.0, 2.5);
        }
        Icon::Phone => {
            // a handset: earpiece top-left, mouthpiece bottom-right, the curved grip between
            p.stroke.move_to(6.0, 3.5);
            p.stroke.line_to(9.5, 3.5);
            p.stroke.line_to(11.0, 8.5);
            p.stroke.line_to(8.5, 10.0);
            p.stroke.quad_to(9.5, 14.5, 14.0, 15.5);
            p.stroke.line_to(15.5, 13.0);
            p.stroke.line_to(20.5, 14.5);
            p.stroke.line_to(20.5, 18.0);
            p.stroke.quad_to(20.5, 20.5, 18.0, 20.5);
            p.stroke.quad_to(3.5, 20.0, 3.5, 6.0);
            p.stroke.quad_to(3.5, 3.5, 6.0, 3.5);
            p.stroke.close();
        }
        Icon::Message => {
            p.rrect(3.0, 4.0, 18.0, 13.0, 2.5);
            p.poly(&[(8.0, 17.0), (8.0, 21.0), (12.5, 17.0)], false);
            p.line((7.5, 9.0), (16.5, 9.0));
            p.line((7.5, 12.5), (13.0, 12.5));
        }
        Icon::Person => {
            p.circle(12.0, 8.0, 4.0);
            p.arc(12.0, 21.0, 8.0, 180.0, 360.0);
        }
        Icon::Mic | Icon::MicOff => {
            p.rrect(9.0, 3.0, 6.0, 11.0, 3.0);
            p.arc(12.0, 11.0, 6.5, 0.0, 180.0);
            p.line((12.0, 17.5), (12.0, 21.0));
            p.line((8.5, 21.0), (15.5, 21.0));
            if icon == Icon::MicOff {
                p.line((4.0, 4.0), (20.0, 20.0));
            }
        }
        Icon::Keypad => {
            for y in [5.0, 11.0, 17.0] {
                for x in [6.0, 12.0, 18.0] {
                    p.dot(x, y, 1.7);
                }
            }
            p.dot(12.0, 22.0, 1.7);
        }
        Icon::Location => {
            // map pin: round head, point at the bottom, ring in the middle
            p.arc(12.0, 9.5, 6.5, 135.0, 405.0);
            p.poly(&[(16.6, 14.1), (12.0, 21.5), (7.4, 14.1)], false);
            p.circle(12.0, 9.5, 2.3);
        }
    }
    (p.stroke.finish(), p.dots.finish())
}

/// The speaker body of the volume icons.
fn speaker(p: &mut Pen) {
    p.poly(
        &[
            (3.5, 9.0),
            (7.5, 9.0),
            (12.5, 4.5),
            (12.5, 19.5),
            (7.5, 15.0),
            (3.5, 15.0),
        ],
        true,
    );
}

/// Draw `icon` with its top-left at (x, y), `size` px square (24 = native).
pub fn draw_icon(cv: &mut Canvas, icon: Icon, x: i32, y: i32, size: i32, color: Color) {
    draw_icon_rotated(cv, icon, x, y, size, color, 0.0);
}

/// Same, rotated by `angle_deg` around the icon centre (spinners).
pub fn draw_icon_rotated(
    cv: &mut Canvas,
    icon: Icon,
    x: i32,
    y: i32,
    size: i32,
    color: Color,
    angle_deg: f32,
) {
    let s = size as f32 / 24.0;
    let t = Transform::from_rotate_at(angle_deg, 12.0, 12.0)
        .post_scale(s, s)
        .post_translate(x as f32, y as f32);
    let (stroke, dots) = build(icon);
    if let Some(path) = stroke {
        // 2 px at the native 24 px size; grows slowly so large icons do not turn into blobs
        cv.stroke(&path, (1.0 + s) / s, color, t);
    }
    if let Some(path) = dots {
        cv.fill_path(&path, color, t);
    }
}

impl Canvas {
    pub fn icon(&mut self, icon: Icon, x: i32, y: i32, size: i32, color: Color) {
        draw_icon(self, icon, x, y, size, color);
    }
    pub fn icon_rotated(
        &mut self,
        icon: Icon,
        x: i32,
        y: i32,
        size: i32,
        color: Color,
        angle_deg: f32,
    ) {
        draw_icon_rotated(self, icon, x, y, size, color, angle_deg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::*;

    #[test]
    fn every_icon_draws_something_inside_its_box() {
        for icon in Icon::ALL {
            let mut c = Canvas::new(48, 48, BG).unwrap();
            c.icon(icon, 12, 12, 24, TEXT);
            let lit: Vec<(i32, i32)> = (0..48)
                .flat_map(|x| (0..48).map(move |y| (x, y)))
                .filter(|&(x, y)| c.pixel(x, y) != Some(BG))
                .collect();
            assert!(lit.len() > 15, "{icon:?} drew {} pixels", lit.len());
            // strokes may bleed 1 px past the 24 grid (round caps), nothing further
            assert!(
                lit.iter()
                    .all(|&(x, y)| (11..37).contains(&x) && (11..37).contains(&y)),
                "{icon:?} drew outside its box"
            );
        }
    }

    #[test]
    fn icons_scale() {
        let mut small = Canvas::new(64, 64, BG).unwrap();
        let mut big = Canvas::new(64, 64, BG).unwrap();
        small.icon(Icon::Wifi, 0, 0, 24, TEXT);
        big.icon(Icon::Wifi, 0, 0, 48, TEXT);
        let count = |c: &Canvas| {
            (0..64)
                .flat_map(|x| (0..64).map(move |y| (x, y)))
                .filter(|&(x, y)| c.pixel(x, y) != Some(BG))
                .count()
        };
        assert!(count(&big) > count(&small) * 2);
    }

    #[test]
    fn names_from_apps_json() {
        assert_eq!(Icon::from_name("globe"), Some(Icon::Globe));
        assert_eq!(Icon::from_name("files"), Some(Icon::Folder));
        assert_eq!(Icon::from_name("nope"), None);
        assert_eq!(Icon::ALL.len(), 43);
        assert_eq!(Icon::from_name("phone"), Some(Icon::Phone));
        assert_eq!(Icon::from_name("bluetooth"), Some(Icon::Bluetooth));
    }
}
