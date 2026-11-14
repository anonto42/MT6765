use super::*;
use crate::keyboard::Stroke;
use walpad_ui::edit_bar::{clip_height, EditBtn};

fn cls(a: &str, b: &str) -> Vec<String> {
    vec![a.to_string(), b.to_string()]
}

#[test]
fn terminals_are_recognised_by_wm_class() {
    assert!(is_terminal(&cls("st", "St")));
    assert!(is_terminal(&cls("xterm", "XTerm")));
    assert!(is_terminal(&cls("st-256color", "st-256color")));
    // regression (2026-11-13): the "terminal"/"kali" apps got their own real WM_CLASS
    // (term-st/kali-st, not a shared "st") so a restart could tell them apart - this list
    // must track that or Ctrl+C stops being an interrupt and paste stops using Ctrl+Shift+V.
    assert!(is_terminal(&cls("term-st", "term-st")));
    assert!(is_terminal(&cls("kali-st", "kali-st")));
    assert!(!is_terminal(&cls("Navigator", "firefox-esr")));
    assert!(!is_terminal(&cls("pcmanfm", "Pcmanfm")));
    assert!(!is_terminal(&[]));
}

#[test]
fn button_strokes_are_the_ctrl_shortcuts_and_never_interrupt_a_terminal() {
    let k = |c: char| Some(Stroke::ctrl(c as u32));
    assert_eq!(button_stroke(EditBtn::Cut, false), k('x'));
    assert_eq!(button_stroke(EditBtn::Copy, false), k('c'));
    assert_eq!(button_stroke(EditBtn::Paste, false), k('v'));
    assert_eq!(button_stroke(EditBtn::SelectAll, false), k('a'));
    assert_eq!(button_stroke(EditBtn::Undo, false), k('z'));
    // terminal: paste is Ctrl+Shift+V; nothing else types (Ctrl+C would interrupt)
    let tv = button_stroke(EditBtn::Paste, true).unwrap();
    assert!(tv.ctrl && tv.shift && tv.keysym == 'v' as u32);
    for b in [
        EditBtn::Cut,
        EditBtn::Copy,
        EditBtn::SelectAll,
        EditBtn::Undo,
    ] {
        assert_eq!(button_stroke(b, true), None, "{b:?}");
    }
    assert_eq!(paste_stroke(false), Stroke::ctrl('v' as u32));
}

#[test]
fn toolbar_sits_above_the_keyboard_or_the_screen_edge() {
    let r = bar_rect((800, 1280), 384, 5);
    assert_eq!((r.x, r.w, r.h), (8, 480, 52));
    assert_eq!(r.bottom(), 1280 - 384 - 8);
    let r = bar_rect((800, 1280), 0, 5);
    assert_eq!(r.bottom(), 1280 - 8);
    // the dock handle (56 wide, 8 from the right edge) does not overlap it
    assert!(r.right() <= 800 - 8 - 56);
    let r = bar_rect((1280, 800), 240, 2);
    assert_eq!((r.w, r.bottom()), (192, 800 - 240 - 8));
}

#[test]
fn list_sits_above_the_toolbar_and_shrinks_to_fit() {
    let r = list_rect((800, 1280), 384, 10);
    assert_eq!(r.h, clip_height(10));
    assert!(r.bottom() <= bar_rect((800, 1280), 384, 5).y);
    assert!(r.y >= 0);
    // landscape with the keyboard: fewer rows fit
    let r = list_rect((1280, 800), 240, 10);
    assert!(r.h < clip_height(10));
    assert!(r.y >= 0 && r.bottom() <= bar_rect((1280, 800), 240, 5).y);
    let r = list_rect((800, 1280), 0, 0);
    assert_eq!(r.h, clip_height(0));
    assert_eq!(r.w, 520);
}

#[test]
fn toolbar_visibility_rule() {
    let mut s = BarState::default();
    assert!(!s.visible(0));
    // owner report 2026-11-13, "i don't need [it on top of the keyboard] becose our latest
    // sytem of copy will cover thows": merely focusing a text input no longer shows it -
    // only a real selection does.
    s.home = true;
    s.selection(0);
    assert!(!s.visible(0), "never over the shell's own screen");
    s.home = false;
    s.dock_out = true;
    assert!(!s.visible(0), "the expanded dock covers its place");
    s.dock_out = false;
    s.clear_selection();
    s.selection(1000);
    assert!(s.visible(1000 + SELECTION_MS - 1));
    assert!(
        !s.visible(1000 + SELECTION_MS),
        "a selection toolbar times out"
    );
    s.selection(5000);
    s.clear_selection();
    assert!(!s.visible(5001));
    assert_eq!(s.deadline(), None);
}
