//! Golden-image harness. `assert_snapshot("name", &canvas)` compares with
//! tests/golden/<name>.png; UPDATE_SNAPSHOTS=1 regenerates (and prints the changed names).
//! Tolerance: per-channel difference <= 2 and at most 0.1% of pixels differ (anti-aliasing noise).
#![allow(clippy::expect_used)]
use crate::canvas::Canvas;
use std::path::PathBuf;

fn golden_dir() -> PathBuf {
    std::env::var_os("WALPAD_GOLDEN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden"))
}

pub fn assert_snapshot(name: &str, cv: &Canvas) {
    assert_snapshot_in(&golden_dir(), name, cv)
}

/// Same with an explicit golden directory (the shell crate keeps its own).
pub fn assert_snapshot_in(dir: &std::path::Path, name: &str, cv: &Canvas) {
    let golden = dir.join(format!("{name}.png"));
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        let old = std::fs::read(&golden).ok();
        let new = cv.encode_png().expect("png encode");
        if old.as_deref() != Some(new.as_slice()) {
            cv.save_png(&golden).expect("write golden");
            eprintln!("SNAPSHOT UPDATED: {name}");
        }
        return;
    }
    let bytes = std::fs::read(&golden).unwrap_or_else(|_| {
        panic!(
            "missing golden {} (run with UPDATE_SNAPSHOTS=1)",
            golden.display()
        )
    });
    let want = tiny_skia::Pixmap::decode_png(&bytes).expect("decode golden");
    let (w, h) = (cv.width() as u32, cv.height() as u32);
    if (want.width(), want.height()) != (w, h) {
        panic!(
            "snapshot {name}: size {}x{} but golden is {}x{}",
            w,
            h,
            want.width(),
            want.height()
        );
    }
    let (a, b) = (cv.rgba(), want.data());
    let mut bad = 0usize;
    for (p, q) in a.chunks(4).zip(b.chunks(4)) {
        if p.iter().zip(q).any(|(x, y)| x.abs_diff(*y) > 2) {
            bad += 1;
        }
    }
    let allowed = (a.len() / 4) / 1000;
    if bad > allowed {
        let actual = std::env::temp_dir().join(format!("{name}.actual.png"));
        let _ = cv.save_png(&actual);
        panic!(
            "snapshot {name}: {bad} pixels differ (allowed {allowed}); actual saved to {}",
            actual.display()
        );
    }
}
