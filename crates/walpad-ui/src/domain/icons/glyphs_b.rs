use super::pen::Pen;
use super::Icon;

pub(super) fn build_b(icon: Icon, p: &mut Pen) {
    match icon {
        Icon::Volume => {
            speaker(p);
            p.arc(11.0, 12.0, 5.0, -48.0, 48.0);
            p.arc(11.0, 12.0, 9.0, -48.0, 48.0);
        }
        Icon::VolumeOff => {
            speaker(p);
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
        Icon::Bolt => {
            p.poly(
                &[
                    (13.5, 2.0),
                    (6.0, 13.0),
                    (11.5, 13.0),
                    (10.5, 22.0),
                    (18.0, 10.5),
                    (12.5, 10.5),
                ],
                true,
            );
        }
        Icon::Bell => {
            // dome, flared sides to a base line, a top knob and a clapper dot
            p.arc(12.0, 11.0, 6.0, 200.0, 340.0);
            p.line((6.36, 8.95), (4.5, 16.0));
            p.line((17.64, 8.95), (19.5, 16.0));
            p.line((4.5, 16.0), (19.5, 16.0));
            p.line((12.0, 5.0), (12.0, 3.0));
            p.dot(12.0, 19.3, 1.6);
        }
        Icon::Code => {
            p.poly(&[(9.0, 5.0), (3.0, 12.0), (9.0, 19.0)], false);
            p.poly(&[(15.0, 5.0), (21.0, 12.0), (15.0, 19.0)], false);
            p.line((13.5, 4.0), (10.5, 20.0));
        }
        Icon::Cut => {
            // two blades crossing in an X above two ring handles, like a pair of scissors
            p.circle(7.0, 17.0, 2.3);
            p.circle(17.0, 17.0, 2.3);
            p.line((7.0, 17.0), (19.0, 5.0));
            p.line((17.0, 17.0), (5.0, 5.0));
        }
        Icon::Copy => {
            // two overlapping sheets
            p.rrect(3.5, 3.5, 13.0, 13.0, 2.0);
            p.rrect(8.5, 8.5, 12.0, 12.0, 2.0);
        }
        Icon::SelectAll => {
            // four corner brackets, like a crop/viewfinder
            p.poly(&[(4.0, 9.0), (4.0, 4.0), (9.0, 4.0)], false);
            p.poly(&[(15.0, 4.0), (20.0, 4.0), (20.0, 9.0)], false);
            p.poly(&[(4.0, 15.0), (4.0, 20.0), (9.0, 20.0)], false);
            p.poly(&[(15.0, 20.0), (20.0, 20.0), (20.0, 15.0)], false);
        }
        Icon::Undo => {
            // a left-curling arrow: a wide arc open on the right, arrowhead at its lower end
            p.arc(13.0, 13.0, 7.0, 280.0, 100.0);
            p.arrow_head((7.0, 21.0), (12.0, 18.5), (10.0, 23.5));
        }
        _ => {}
    }
}

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
