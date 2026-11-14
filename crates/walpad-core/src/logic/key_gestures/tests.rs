use super::*;

const UP: u16 = KEY_VOLUMEUP;
const DOWN: u16 = KEY_VOLUMEDOWN;

fn tap(g: &mut Gestures, code: u16, at: Ms) {
    g.on_key(code, 1, at);
    g.on_key(code, 0, at + 50);
}

#[test]
fn action_parser() {
    assert_eq!(parse_action("none"), Action::None);
    assert_eq!(parse_action(""), Action::None);
    assert_eq!(parse_action("key:ctrl+Tab"), Action::Key("ctrl+Tab".into()));
    assert_eq!(
        parse_action("ctl:brightness +40"),
        Action::Ctl("brightness +40".into())
    );
    assert_eq!(
        parse_action("launch:browser"),
        Action::Launch("browser".into())
    );
    assert_eq!(
        parse_action("run:firefox-esr"),
        Action::Run("firefox-esr".into())
    );
    assert_eq!(parse_action("volume:-5"), Action::Volume(-5));
    assert_eq!(parse_action("volume:+5"), Action::Volume(5));
    assert_eq!(parse_action("home"), Action::Home);
    assert_eq!(parse_action("back"), Action::Back);
    assert_eq!(parse_action("powermenu"), Action::PowerMenu);
    assert!(matches!(parse_action("rm -rf /"), Action::Unknown(_)));
}

#[test]
fn rule3_5_tap_counts_after_window() {
    for (n, want) in [
        (1, "key:Up"),
        (2, "key:Prior"),
        (3, "ctl:brightness +40"),
        (4, "none"),
        (5, "none"),
    ] {
        let mut g = Gestures::new(Cfg::default());
        let mut t = 0;
        for _ in 0..n {
            tap(&mut g, UP, t);
            t += 100;
        }
        let last_release = t - 100 + 50;
        assert!(
            g.tick(last_release + 349).is_empty(),
            "n={n}: still inside the window"
        );
        let got = g.tick(last_release + 350);
        let want = parse_action(want);
        if want == Action::None {
            assert!(got.is_empty());
        } else {
            assert_eq!(got, vec![want], "n={n}");
        }
    }
}

#[test]
fn rule3_quad_uses_quad_action_for_four_or_more() {
    let mut cfg = Cfg::default();
    cfg.vol_up.quad = "launch:browser".into();
    for n in [4, 6] {
        let mut g = Gestures::new(cfg.clone());
        for i in 0..n {
            tap(&mut g, UP, i * 100);
        }
        assert_eq!(
            g.tick(n * 100 + 400),
            vec![Action::Launch("browser".into())]
        );
    }
}

#[test]
fn rule2_chord_mutes_both_and_runs_both_press() {
    let mut g = Gestures::new(Cfg::default());
    g.on_key(UP, 1, 0);
    assert_eq!(
        g.on_key(DOWN, 1, 100),
        vec![Action::Ctl("rotate toggle".into())]
    );
    g.on_key(UP, 0, 200);
    g.on_key(DOWN, 0, 210);
    assert!(g.tick(5000).is_empty(), "muted: no taps afterwards");
}

#[test]
fn rule2_chord_window_and_long() {
    let mut g = Gestures::new(Cfg::default());
    g.on_key(UP, 1, 0);
    assert!(g.on_key(DOWN, 1, 161).is_empty(), "outside chord_ms");
    let mut g = Gestures::new(Cfg::default());
    g.on_key(UP, 1, 0);
    g.tick(700); // up is long now
    assert!(
        g.on_key(DOWN, 1, 100 + 600).is_empty(),
        "other already long"
    );
}

#[test]
fn rule4_long_runs_once_and_clears_taps() {
    let mut g = Gestures::new(Cfg::default());
    tap(&mut g, DOWN, 0);
    g.on_key(DOWN, 1, 100);
    assert!(g.tick(699).is_empty());
    assert_eq!(g.tick(700), vec![Action::Home]);
    assert!(g.tick(900).is_empty(), "once");
    g.on_key(DOWN, 0, 1000);
    assert!(
        g.tick(3000).is_empty(),
        "release after long adds no tap, earlier taps cleared"
    );
}

#[test]
fn rule4_hold_repeat_quirk_first_repeat_in_same_tick() {
    // Parity: last_rep is the press time, so the first repeat fires with the long action.
    let mut cfg = Cfg::default();
    cfg.vol_up.hold_repeat = true;
    let mut g = Gestures::new(cfg);
    g.on_key(UP, 1, 0);
    assert_eq!(g.tick(600), vec![Action::Keyboard, Action::Keyboard]);
    assert!(g.tick(700).is_empty());
    assert_eq!(g.tick(740), vec![Action::Keyboard]);
}

#[test]
fn wake_press_is_swallowed() {
    let mut g = Gestures::new(Cfg::default());
    g.note_wake(1000);
    tap(&mut g, UP, 1000);
    assert!(
        g.tick(3000).is_empty(),
        "short press that woke the screen: no tap"
    );
    // long press that woke the screen: no action
    g.note_wake(5000);
    g.on_key(DOWN, 1, 5000);
    assert!(g.tick(5700).is_empty());
    // a later press is fine
    tap(&mut g, UP, 9000);
    assert_eq!(g.tick(9500), vec![Action::Key("Up".into())]);
}

#[test]
fn other_keys_ignored() {
    let mut g = Gestures::new(Cfg::default());
    g.on_key(116, 1, 0);
    g.on_key(116, 0, 10);
    assert!(g.tick(1000).is_empty());
}

#[test]
fn keys_json_defaults_deserialize() {
    let cfg: Cfg =
        serde_json::from_str(r#"{"vol_up":{"quad":"launch:browser"},"long_ms":500}"#).unwrap();
    assert_eq!(cfg.long_ms, 500);
    assert_eq!(cfg.vol_up.quad, "launch:browser");
    assert_eq!(
        cfg.vol_up.single, "none",
        "missing per-button keys fall back to the serde default"
    );
}

#[test]
fn shipped_keys_json_parses_and_quad_launches_a_shipped_app() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/rootfs/etc/walpad");
    let cfg: Cfg =
        serde_json::from_str(&std::fs::read_to_string(root.join("keys.json")).unwrap()).unwrap();
    assert_eq!(cfg.vol_up.quad, "launch:browser");
    assert_eq!(
        parse_action(&cfg.vol_up.quad),
        Action::Launch("browser".into())
    );
    for a in [
        &cfg.vol_up.single,
        &cfg.vol_up.double,
        &cfg.vol_up.triple,
        &cfg.vol_up.long,
        &cfg.vol_down.single,
        &cfg.vol_down.double,
        &cfg.vol_down.triple,
        &cfg.vol_down.quad,
        &cfg.vol_down.long,
        &cfg.both.press,
    ] {
        assert!(
            !matches!(parse_action(a), Action::Run(_)),
            "run: in keys.json: {a}"
        );
    }
    let apps: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("apps.json")).unwrap()).unwrap();
    assert!(
        apps.as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "browser"),
        "apps.json has no browser"
    );
}
