//! Unit tests for the Media screen.

use super::*;
use walpad_hw::Root;
use walpad_proto::AppState;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::TOUCH_MIN;
use walpad_ui::{Canvas, Color};

fn tmp(tag: &str) -> Root {
    let d = std::env::temp_dir().join(format!("walpad-media-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Root::new(d)
}

fn touch(r: &Root, p: &str, age: u64) {
    let f = r.p(p);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(&f, "x").unwrap();
    let t = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
    std::fs::File::options()
        .write(true)
        .open(&f)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

fn item(n: &str, k: Kind) -> Item {
    Item {
        path: format!("/root/Pictures/{n}"),
        name: n.into(),
        kind: k,
        mtime: 1,
        size: 10,
    }
}

#[test]
fn kinds_come_from_the_extension_case_insensitively() {
    assert_eq!(Kind::of("/a/b.JPG"), Some(Kind::Image));
    assert_eq!(Kind::of("/a/b.mkv"), Some(Kind::Video));
    assert_eq!(Kind::of("/a/b.Flac"), Some(Kind::Audio));
    assert_eq!(Kind::of("/a/b.txt"), None);
    assert_eq!(Kind::of("/a/noext"), None);
}

#[test]
fn scan_finds_media_newest_first_and_skips_the_rest() {
    let r = tmp("scan");
    touch(&r, "/root/Pictures/old.jpg", 500);
    touch(&r, "/root/Pictures/new.png", 10);
    touch(&r, "/root/Pictures/sub/deep/x.gif", 100);
    touch(&r, "/mnt/sd/DCIM/100/v.mp4", 50);
    touch(&r, "/mnt/usb/DISK1/Music/s.mp3", 20);
    touch(&r, "/root/Music/t.flac", 30);
    touch(&r, "/root/Pictures/notes.txt", 1);
    touch(&r, "/root/Pictures/.hidden.jpg", 1);
    touch(&r, "/root/Pictures/.thumbs/t.jpg", 1);
    touch(&r, "/mnt/sd/Android/data/a.jpg", 1);
    touch(&r, "/etc/not-scanned.jpg", 1);
    // a symlink to a folder outside is not followed
    std::fs::create_dir_all(r.p("/outside")).unwrap();
    touch(&r, "/outside/secret.jpg", 1);
    std::os::unix::fs::symlink(r.p("/outside"), r.p("/root/Pictures/link")).unwrap();
    let items = scan(&r);
    let paths: Vec<&str> = items.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "/root/Pictures/new.png",
            "/mnt/usb/DISK1/Music/s.mp3",
            "/root/Music/t.flac",
            "/mnt/sd/DCIM/100/v.mp4",
            "/root/Pictures/sub/deep/x.gif",
            "/root/Pictures/old.jpg",
        ]
    );
    assert!(items.iter().all(|i| walpad_proto::valid_open_path(&i.path)));
}

#[test]
fn scan_is_bounded() {
    let r = tmp("bounded");
    for i in 0..(MAX_ITEMS + 80) {
        touch(&r, &format!("/root/Pictures/p{i:04}.jpg"), i as u64);
    }
    touch(&r, "/root/Pictures/a/b/c/d/too-deep.jpg", 0);
    let items = scan(&r);
    assert_eq!(items.len(), MAX_ITEMS);
    assert!(items.iter().all(|i| !i.path.contains("too-deep")));
    assert_eq!(items[0].name, "p0000.jpg", "the newest are kept");
}

#[test]
fn recent_list_is_deduplicated_capped_and_validated() {
    let r = tmp("recent");
    assert!(read_recent(&r).is_empty());
    remember(&r, "/root/a.mp3");
    remember(&r, "/root/b.mp3");
    let v = remember(&r, "/root/a.mp3");
    assert_eq!(v, ["/root/a.mp3", "/root/b.mp3"]);
    for i in 0..40 {
        remember(&r, &format!("/root/f{i}.mp3"));
    }
    assert_eq!(read_recent(&r).len(), MAX_RECENT);
    assert_eq!(read_recent(&r)[0], "/root/f39.mp3");
    // a hand-edited file with a bad line: it is dropped
    std::fs::write(r.p(RECENT_FILE), "/etc/passwd\n-x\n/root/ok.mp3\n").unwrap();
    assert_eq!(read_recent(&r), ["/root/ok.mp3"]);
}

#[test]
fn sections_filter_by_kind_and_recent_keeps_its_order() {
    let mut m = MediaModel::default();
    m.set_items(vec![
        item("a.jpg", Kind::Image),
        item("b.mp4", Kind::Video),
        item("c.mp3", Kind::Audio),
    ]);
    assert_eq!(m.list().len(), 1);
    m.section = Section::Videos;
    assert_eq!(m.list()[0].name, "b.mp4");
    m.section = Section::Recent;
    m.set_recent(vec![
        "/root/Pictures/c.mp3".into(),
        "/root/gone.mp4".into(),
        "/root/Pictures/a.jpg".into(),
        "/root/readme.txt".into(),
    ]);
    let names: Vec<String> = m.list().into_iter().map(|i| i.name).collect();
    assert_eq!(
        names,
        ["c.mp3", "gone.mp4", "a.jpg"],
        "unknown files stay, non-media goes"
    );
}

#[test]
fn players_are_chosen_among_the_installed_apps() {
    let mut st = crate::app::tests::status(100, &[]);
    st.apps = vec![AppState {
        app: "vlc".into(),
        running: false,
    }];
    let st = Some(st);
    assert_eq!(player_for(Kind::Video, &st), Some("vlc"));
    assert_eq!(player_for(Kind::Image, &st), None);
    assert_eq!(player_for(Kind::Audio, &None), None);
    let mut s2 = st.clone().unwrap();
    s2.apps.push(AppState {
        app: "mpv".into(),
        running: false,
    });
    s2.apps.push(AppState {
        app: "gpicview".into(),
        running: false,
    });
    let s2 = Some(s2);
    assert_eq!(player_for(Kind::Video, &s2), Some("mpv"));
    assert_eq!(player_for(Kind::Audio, &s2), Some("mpv"));
    assert_eq!(player_for(Kind::Image, &s2), Some("gpicview"));
}

#[test]
fn layout_grid_and_list_fit_both_orientations() {
    for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
        let c = f.padded();
        let g = layout(&f, Section::Photos, 23);
        assert_eq!(g.cells.len(), 23);
        for (i, a) in g.cells.iter().enumerate() {
            assert!(a.x >= c.x && a.right() <= c.right(), "{a:?}");
            assert!(a.w >= TOUCH_MIN && a.h >= TOUCH_MIN);
            for b in &g.cells[i + 1..] {
                assert!(
                    a.right() <= b.x || b.right() <= a.x || a.bottom() <= b.y || b.bottom() <= a.y,
                    "{a:?} overlaps {b:?}"
                );
            }
        }
        assert!(g.total >= g.cells.last().unwrap().bottom());
        assert!(g.seg.bottom() < g.cells[0].y);
        let cols = g.cells.iter().filter(|r| r.y == g.cells[0].y).count() as i32;
        assert_eq!(cols, (c.w + GAP) / (TILE + GAP));
        let l = layout(&f, Section::Music, 5);
        assert!(l.cells.iter().all(|r| r.h == LIST_ROW_H && r.w == c.w));
        assert_eq!(layout(&f, Section::Music, 0).cells.len(), 0);
        assert!(layout(&f, Section::Music, 0).total > l.seg.bottom());
    }
}

#[test]
fn only_what_is_near_the_screen_is_asked_for_once() {
    let f = Frame::new(800, 1280);
    let list: Vec<Item> = (0..200)
        .map(|i| item(&format!("p{i}.jpg"), Kind::Image))
        .collect();
    let mut m = MediaModel::default();
    m.set_items(list.clone());
    let l = layout(&f, Section::Photos, list.len());
    let (a, b) = visible_range(&l, 0, f.content.h).unwrap();
    assert_eq!(a, 0);
    assert!(b > 12 && b < 100, "a couple of screens, not all 200: {b}");
    m.note_visible(&list, a, b);
    assert_eq!(m.want_thumbs.len(), b - a + 1);
    assert!(m.want_info.is_empty(), "photos have no duration");
    let n = m.want_thumbs.len();
    m.note_visible(&list, a, b);
    assert_eq!(m.want_thumbs.len(), n, "never asked twice");
    // videos and music also want their facts
    let mut v = MediaModel::default();
    let vl = vec![item("v.mp4", Kind::Video), item("s.mp3", Kind::Audio)];
    v.section = Section::Recent;
    v.note_visible(&vl, 0, 1);
    assert_eq!(v.want_info.len(), 2);
    assert_eq!(
        v.want_thumbs.len(),
        1,
        "a list row has no thumbnail for music"
    );
}

#[test]
fn thumbnails_decode_scale_and_are_capped() {
    let mut m = MediaModel::default();
    let png = {
        let c = Canvas::new(192, 192, Color::hex(0x336699)).unwrap();
        c.encode_png().unwrap()
    };
    m.on_thumb("/root/a.jpg", Some(&png));
    match m.thumbs.get("/root/a.jpg") {
        Some(Thumb::Ready(c)) => assert_eq!((c.width(), c.height()), (TILE, TILE)),
        _ => panic!("ready"),
    }
    m.on_thumb("/root/bad.jpg", Some(b"not a png"));
    assert!(matches!(
        m.thumbs.get("/root/bad.jpg"),
        Some(Thumb::Missing)
    ));
    m.on_thumb("/root/none.jpg", None);
    assert!(matches!(
        m.thumbs.get("/root/none.jpg"),
        Some(Thumb::Missing)
    ));
    for i in 0..MAX_THUMBS + 20 {
        m.on_thumb(&format!("/root/t{i}.jpg"), Some(&png));
    }
    let ready = m
        .thumbs
        .values()
        .filter(|t| matches!(t, Thumb::Ready(_)))
        .count();
    assert!(ready <= MAX_THUMBS);
}

#[test]
fn durations_and_sizes_are_formatted() {
    assert_eq!(fmt_duration(187), "3:07");
    assert_eq!(fmt_duration(3729), "1:02:09");
    assert_eq!(fmt_duration(5), "0:05");
    assert_eq!(fmt_size(500), "1 KB");
    assert_eq!(fmt_size(5 * 1024 * 1024), "5 MB");
    assert_eq!(stem("song.final.mp3"), "song.final");
    assert_eq!(stem(".hidden"), ".hidden");
}
