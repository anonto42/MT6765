use super::*;

fn down(r: &mut Recogniser, x: i32, y: i32, t: Ms) -> Vec<Gesture> {
    r.feed(Input::Down { x, y }, t)
}

#[test]
fn tap_within_slop_and_time() {
    let mut r = Recogniser::new();
    assert_eq!(
        down(&mut r, 100, 100, 0),
        vec![Gesture::Pressed { x: 100, y: 100 }]
    );
    assert_eq!(
        r.feed(Input::Up { x: 105, y: 96 }, 200),
        vec![Gesture::Tap { x: 105, y: 96 }]
    );
    assert_eq!(r.next_deadline(), None);
}

#[test]
fn slop_boundary_is_10_px() {
    let mut r = Recogniser::new();
    down(&mut r, 0, 0, 0);
    assert_eq!(
        r.feed(Input::Move { x: 10, y: 10 }, 10),
        vec![],
        "exactly 10 px is still a press"
    );
    assert_eq!(
        r.feed(Input::Up { x: 10, y: 10 }, 20),
        vec![Gesture::Tap { x: 10, y: 10 }]
    );
    down(&mut r, 0, 0, 100);
    let g = r.feed(Input::Move { x: 11, y: 0 }, 110);
    assert_eq!(
        g,
        vec![
            Gesture::Cancel,
            Gesture::DragStart { x: 0, y: 0 },
            Gesture::Drag { dx: 11, dy: 0 }
        ]
    );
}

#[test]
fn long_press_fires_once_at_500ms_and_release_is_silent() {
    let mut r = Recogniser::new();
    down(&mut r, 50, 60, 1000);
    assert_eq!(r.next_deadline(), Some(1500));
    assert!(r.tick(1499).is_empty());
    assert_eq!(
        r.tick(1500),
        vec![Gesture::Cancel, Gesture::LongPress { x: 50, y: 60 }]
    );
    assert!(r.tick(1600).is_empty(), "fires once");
    assert!(
        r.feed(Input::Move { x: 200, y: 200 }, 1700).is_empty(),
        "after a long press, moving does not scroll"
    );
    assert!(r.feed(Input::Up { x: 200, y: 200 }, 1800).is_empty());
}

#[test]
fn slow_release_without_a_tick_still_counts_as_long_press() {
    let mut r = Recogniser::new();
    down(&mut r, 5, 5, 0);
    assert_eq!(
        r.feed(Input::Up { x: 5, y: 5 }, 700),
        vec![Gesture::Cancel, Gesture::LongPress { x: 5, y: 5 }]
    );
}

#[test]
fn tap_boundary_is_500ms() {
    let mut r = Recogniser::new();
    down(&mut r, 5, 5, 0);
    assert_eq!(
        r.feed(Input::Up { x: 5, y: 5 }, 499),
        vec![Gesture::Tap { x: 5, y: 5 }]
    );
}

#[test]
fn drag_scrolls_with_deltas_and_ends_on_release() {
    let mut r = Recogniser::new();
    down(&mut r, 100, 500, 0);
    let first = r.feed(Input::Move { x: 100, y: 450 }, 20);
    assert_eq!(first[2], Gesture::Drag { dx: 0, dy: -50 });
    assert_eq!(
        r.feed(Input::Move { x: 100, y: 430 }, 40),
        vec![Gesture::Drag { dx: 0, dy: -20 }]
    );
    assert!(
        r.feed(Input::Move { x: 100, y: 430 }, 50).is_empty(),
        "no movement, no event"
    );
    assert_eq!(
        r.feed(Input::Up { x: 100, y: 430 }, 60),
        vec![Gesture::DragEnd]
    );
    // a long hold while dragging never becomes a long press
    let mut r = Recogniser::new();
    down(&mut r, 0, 0, 0);
    r.feed(Input::Move { x: 0, y: 40 }, 10);
    assert!(r.tick(5000).is_empty());
}

#[test]
fn stray_events_when_idle_are_ignored_and_new_press_restarts() {
    let mut r = Recogniser::new();
    assert!(r.feed(Input::Move { x: 1, y: 1 }, 0).is_empty());
    assert!(r.feed(Input::Up { x: 1, y: 1 }, 1).is_empty());
    down(&mut r, 1, 1, 2);
    // a second Down without an Up (lost event) restarts cleanly
    assert_eq!(down(&mut r, 9, 9, 3), vec![Gesture::Pressed { x: 9, y: 9 }]);
    assert_eq!(
        r.feed(Input::Up { x: 9, y: 9 }, 50),
        vec![Gesture::Tap { x: 9, y: 9 }]
    );
}
