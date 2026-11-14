use super::*;

#[test]
fn bar_layout_is_a_row_of_equal_buttons() {
    let l = bar_layout(&TEXT_BUTTONS);
    assert_eq!(l.len(), 5);
    assert_eq!(bar_size(5), (480, 52));
    assert!(l.iter().all(|(r, _)| r.w == BAR_BTN_W && r.h == BAR_H));
    assert_eq!(l[4].0.right(), 480);
    assert_eq!(bar_layout(&TERMINAL_BUTTONS).len(), 2);
    const { assert!(BAR_H >= 48) };
}

#[test]
fn preview_is_one_short_line() {
    assert_eq!(preview("  hello \n\n  world\t!  "), "hello world !");
    assert_eq!(preview(""), "");
    assert_eq!(preview("a\u{0}b"), "a b");
    assert!(preview(&"x".repeat(1000)).chars().count() <= 200);
}

#[test]
fn clip_layout_rows_and_header_buttons() {
    let l = clip_layout(CLIP_W, 3);
    assert_eq!(l.len(), 5);
    assert_eq!(clip_height(3), CLIP_HEAD_H + 3 * CLIP_ROW_H);
    assert_eq!(clip_height(0), CLIP_HEAD_H + 72);
    let entry = |i| l.iter().find(|(_, h)| *h == ClipHit::Entry(i)).unwrap().0;
    assert_eq!(entry(0).y, CLIP_HEAD_H);
    assert_eq!(entry(2).bottom(), clip_height(3));
    let close = l.iter().find(|(_, h)| *h == ClipHit::Close).unwrap().0;
    assert_eq!(close.right(), CLIP_W);
    assert!(close.w >= 48 && close.h >= 48);
}
