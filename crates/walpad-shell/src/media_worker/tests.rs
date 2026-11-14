use super::*;

fn root(tag: &str) -> Root {
    let d = std::env::temp_dir().join(format!("walpad-mworker-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("usr/local/sbin")).unwrap();
    std::fs::create_dir_all(d.join("root/Pictures")).unwrap();
    Root::new(d)
}

fn fake_helper(r: &Root, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    let p = r.p(HELPER);
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn wait(w: &Worker, want: usize) -> Vec<Event> {
    let mut got = Vec::new();
    let t = Instant::now();
    while got.len() < want && t.elapsed() < Duration::from_secs(10) {
        got.extend(w.poll());
        std::thread::sleep(Duration::from_millis(10));
    }
    got
}

#[test]
fn info_lines_are_parsed_leniently() {
    let i = parse_info("duration=187\ntitle=Song\nartist=\nnoise\n");
    assert_eq!(i.duration, Some(187));
    assert_eq!(i.title.as_deref(), Some("Song"));
    assert_eq!(i.artist, None);
    assert_eq!(parse_info("duration=abc"), Info::default());
}

#[test]
fn scans_remembers_and_makes_thumbnails_one_at_a_time() {
    let r = root("flow");
    std::fs::write(r.p("/root/Pictures/a.jpg"), "x").unwrap();
    std::fs::write(r.p("/root/Pictures/b.png"), "x").unwrap();
    // the fake helper: "thumb" writes a PNG-ish file next to the root and prints its path
    let png = r.p("/thumb.png");
    std::fs::write(&png, b"PNGDATA").unwrap();
    fake_helper(
        &r,
        "case $1 in thumb) case $2 in *b.png) exit 5;; esac; echo /thumb.png;; \
         info) echo duration=61; echo title=Hi;; esac",
    );
    let w = Worker::start(Root::new(r.base()));
    w.send(Job::Scan);
    let ev = wait(&w, 2);
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::Scanned(v) if v.len() == 2)));
    w.send(Job::Remember("/root/Pictures/a.jpg".into()));
    let ev = wait(&w, 1);
    assert_eq!(ev, [Event::Recent(vec!["/root/Pictures/a.jpg".into()])]);
    w.send(Job::Thumbs(vec![
        "/root/Pictures/a.jpg".into(),
        "/root/Pictures/b.png".into(),
    ]));
    w.send(Job::Info(vec!["/root/Pictures/a.jpg".into()]));
    let ev = wait(&w, 3);
    assert!(ev.contains(&Event::Thumb(
        "/root/Pictures/a.jpg".into(),
        Some(b"PNGDATA".to_vec())
    )));
    assert!(ev.contains(&Event::Thumb("/root/Pictures/b.png".into(), None)));
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::Info(_, i) if i.duration == Some(61))));
    let t = Instant::now();
    while w.busy() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!w.busy(), "every job is counted off");
}

#[test]
fn a_missing_helper_gives_no_thumbnail_instead_of_a_hang() {
    let r = root("nohelper");
    let w = Worker::start(r);
    w.send(Job::Thumbs(vec!["/root/Pictures/a.jpg".into()]));
    let ev = wait(&w, 1);
    assert_eq!(ev, [Event::Thumb("/root/Pictures/a.jpg".into(), None)]);
}
