//! Selection toolbar and clipboard list snapshots (owner request 2026-11-10: copy and paste).
use std::path::{Path, PathBuf};
use walpad_shell::seltools::{bar_rect, list_rect};
use walpad_ui::components::Painter;
use walpad_ui::edit_bar::{ClipHit, EditBtn, TERMINAL_BUTTONS, TEXT_BUTTONS};
use walpad_ui::snapshot::assert_snapshot_in;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn fonts() -> Fonts {
    Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts")).unwrap()
}

fn bar(name: &str, buttons: &[EditBtn], pressed: Option<usize>) {
    let mut f = fonts();
    let r = bar_rect((800, 1280), 384, buttons.len());
    let mut c = Canvas::new(r.w as u32, r.h as u32, BG).unwrap();
    Painter {
        cv: &mut c,
        fonts: &mut f,
    }
    .edit_bar(buttons, pressed);
    assert_snapshot_in(&golden(), name, &c);
}

fn list(name: &str, area: (i32, i32), entries: &[String], pressed: Option<ClipHit>) {
    let mut f = fonts();
    let r = list_rect(area, if area.0 > area.1 { 240 } else { 384 }, entries.len());
    let shown = entries.len().min(((r.h - 52) / 56).max(0) as usize);
    let mut c = Canvas::new(r.w as u32, r.h as u32, BG).unwrap();
    Painter {
        cv: &mut c,
        fonts: &mut f,
    }
    .clipboard_list(&entries[..shown], pressed);
    assert_snapshot_in(&golden(), name, &c);
}

#[test]
fn selection_toolbar_text_and_terminal() {
    bar("seltools_text", &TEXT_BUTTONS, None);
    bar("seltools_terminal", &TERMINAL_BUTTONS, None);
}

#[test]
fn selection_toolbar_pressed_button() {
    bar("seltools_text_copy_pressed", &TEXT_BUTTONS, Some(1));
}

fn sample() -> Vec<String> {
    [
        "https://example.org/a/rather/long/address/that/does/not/fit/in/one/row/of/the/list?q=1",
        "Meet at 6 pm\nbring the charger",
        "  tab\tseparated   words  ",
        "ls -la /mnt",
        "x",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[test]
fn clipboard_list_with_entries_and_empty() {
    list("clipboard_list_portrait", (800, 1280), &sample(), None);
    list(
        "clipboard_list_pressed_entry",
        (800, 1280),
        &sample(),
        Some(ClipHit::Entry(1)),
    );
    list("clipboard_list_empty", (800, 1280), &[], None);
    let ten: Vec<String> = (1..=10).map(|i| format!("entry number {i}")).collect();
    list("clipboard_list_landscape_ten", (1280, 800), &ten, None);
}
