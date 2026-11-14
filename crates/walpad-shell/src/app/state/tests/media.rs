//! Media screen (owner request 2026-11-11, change media-apps-capture).
use super::*;
use crate::media::{Info, Item, Kind, MediaTarget, Section};
use crate::media_worker::Event;

fn it(name: &str, kind: Kind) -> Item {
    Item {
        path: format!("/root/Pictures/{name}"),
        name: name.into(),
        kind,
        mtime: 1,
        size: 1000,
    }
}

fn with_players(app: &mut App, ids: &[&str]) {
    let apps: Vec<(&str, bool)> = ids.iter().map(|i| (*i, false)).collect();
    app.on_core(&CoreEvent::Status(Box::new(status(120, &apps))), 100);
}

fn open_media(app: &mut App) -> Outcome {
    let i = app.model.apps.iter().position(|t| t.id == "media").unwrap();
    let out = tap(app, Target::AppTile(i));
    assert_eq!(app.active, Tab::Media);
    out
}

#[test]
fn home_tile_and_apps_panel_tool_open_the_screen_and_start_a_scan() {
    let mut app = online();
    assert!(app.model.apps.iter().any(|t| t.id == "media"));
    let out = open_media(&mut app);
    assert!(out.cmds.contains(&Cmd::MediaScan), "{:?}", out.cmds);
    assert!(app.model.media.scanning);
    // from the Apps panel's Tools grid
    go(&mut app, Tab::Home);
    tap(&mut app, Target::Nav(1));
    let tool = crate::panels::TOOLS
        .iter()
        .position(|t| t.label == "Media")
        .unwrap();
    let out = tap(&mut app, Target::PanelTool(tool));
    assert_eq!(app.active, Tab::Media);
    assert!(out.cmds.contains(&Cmd::MediaScan));
    assert!(app.model.overlay.is_none(), "the panel closes");
}

#[test]
fn scan_results_fill_the_sections_and_sections_switch_by_tap() {
    let mut app = online();
    open_media(&mut app);
    assert!(app.on_media(Event::Scanned(vec![
        it("a.jpg", Kind::Image),
        it("b.mp4", Kind::Video),
        it("c.mp3", Kind::Audio),
        it("d.png", Kind::Image),
    ])));
    assert!(!app.model.media.scanning);
    render(&mut app);
    assert_eq!(app.model.media.list().len(), 2, "photos");
    tap(&mut app, Target::Media(MediaTarget::Section(1)));
    assert_eq!(app.model.media.section, Section::Videos);
    assert_eq!(app.model.media.list()[0].name, "b.mp4");
    tap(&mut app, Target::Media(MediaTarget::Section(3)));
    assert_eq!(app.model.media.section, Section::Recent);
    assert!(app.model.media.list().is_empty());
    // refresh scans again
    let out = tap(&mut app, Target::Media(MediaTarget::Refresh));
    assert!(out.cmds.contains(&Cmd::MediaScan));
    assert!(app.model.media.scanning);
    // events while another screen is open do not ask for a redraw
    go(&mut app, Tab::Home);
    assert!(!app.on_media(Event::Recent(vec![])));
}

#[test]
fn tapping_an_item_opens_it_in_the_matching_player_and_remembers_it() {
    let mut app = online();
    with_players(&mut app, &["browser", "mpv", "gpicview"]);
    open_media(&mut app);
    app.on_media(Event::Scanned(vec![
        it("a.jpg", Kind::Image),
        it("b.mp4", Kind::Video),
        it("c.mp3", Kind::Audio),
    ]));
    render(&mut app);
    let out = tap(&mut app, Target::Media(MediaTarget::Item(0)));
    assert_eq!(
        out.cmds,
        vec![
            Cmd::LaunchFile {
                app: "gpicview".into(),
                path: "/root/Pictures/a.jpg".into()
            },
            Cmd::MediaRemember("/root/Pictures/a.jpg".into())
        ]
    );
    // the wire form: launch by id with args.path, nothing else
    let (name, args) = out.cmds[0].request();
    assert_eq!(name, "launch");
    assert_eq!(
        args,
        json!({"app":"gpicview","args":{"path":"/root/Pictures/a.jpg"}})
    );
    tap(&mut app, Target::Media(MediaTarget::Section(2)));
    let out = tap(&mut app, Target::Media(MediaTarget::Item(0)));
    assert!(
        matches!(&out.cmds[0], Cmd::LaunchFile { app, .. } if app == "mpv"),
        "music goes to mpv too"
    );
}

#[test]
fn a_missing_player_or_core_is_a_toast_and_no_command() {
    let mut app = online(); // browser and terminal only
    open_media(&mut app);
    app.on_media(Event::Scanned(vec![it("a.jpg", Kind::Image)]));
    render(&mut app);
    let out = tap(&mut app, Target::Media(MediaTarget::Item(0)));
    assert!(out.cmds.is_empty());
    let t = app.model.toast.clone().unwrap();
    assert!(t.text.contains("photo viewer"), "{}", t.text);
    // the core away: no status, so no player is known either
    with_players(&mut app, &["gpicview"]);
    app.on_core(&CoreEvent::Disconnected, 200);
    let out = tap(&mut app, Target::Media(MediaTarget::Item(0)));
    assert!(out.cmds.is_empty());
}

#[test]
fn a_refused_open_shows_the_core_message() {
    let mut app = online();
    with_players(&mut app, &["browser", "gpicview"]);
    open_media(&mut app);
    app.on_media(Event::Scanned(vec![it("a.jpg", Kind::Image)]));
    render(&mut app);
    let out = tap(&mut app, Target::Media(MediaTarget::Item(0)));
    app.note_sent(5, &out.cmds[0]);
    reply_err(&mut app, 5, "invalid_args", "no such file");
    assert_eq!(app.model.toast.clone().unwrap().text, "no such file");
}

#[test]
fn the_visible_items_are_asked_for_through_tick_once() {
    let mut app = online();
    open_media(&mut app);
    let items: Vec<Item> = (0..300)
        .map(|i| it(&format!("p{i}.jpg"), Kind::Image))
        .collect();
    app.on_media(Event::Scanned(items));
    render(&mut app);
    app.tick(500);
    let cmds = app.take_cmds();
    let asked: Vec<&Vec<String>> = cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::MediaThumbs(v) => Some(v),
            _ => None,
        })
        .collect();
    assert_eq!(asked.len(), 1);
    assert!(
        asked[0].len() > 8 && asked[0].len() < 120,
        "{}",
        asked[0].len()
    );
    render(&mut app);
    app.tick(600);
    assert!(
        app.take_cmds()
            .iter()
            .all(|c| !matches!(c, Cmd::MediaThumbs(_))),
        "not asked twice"
    );
    // scrolling brings new ones
    app.scroll = 3000;
    render(&mut app);
    app.tick(700);
    assert!(app
        .take_cmds()
        .iter()
        .any(|c| matches!(c, Cmd::MediaThumbs(_))));
    // facts and thumbnails arriving redraw the open screen
    assert!(app.on_media(Event::Info("/root/Pictures/p0.jpg".into(), Info::default())));
    assert!(app.on_media(Event::Thumb("/root/Pictures/p0.jpg".into(), None)));
}
