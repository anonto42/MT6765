use super::*;

fn root_with(files: &[(&str, &str)]) -> Root {
    // tests run in parallel: every call gets its own directory
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("walpad-cfg-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("etc/walpad")).unwrap();
    for (n, c) in files {
        std::fs::write(d.join("etc/walpad").join(n), c).unwrap();
    }
    Root::new(d)
}

#[test]
fn devd_defaults_and_overrides() {
    assert_eq!(read_devd(&root_with(&[])), DevdSettings::default());
    let r = root_with(&[(
        "devd.json",
        r#"{"dim_after_s":30,"off_after_s":0,"lid_off":false,"low_batt_mv":3600,"junk":1}"#,
    )]);
    let d = read_devd(&r);
    assert_eq!(
        (
            d.dim_after_s,
            d.off_after_s,
            d.lid_off,
            d.low_batt_mv,
            d.critical_batt_mv
        ),
        (30, 0, false, 3600, 3350)
    );
    assert_eq!(
        read_devd(&root_with(&[("devd.json", "not json")])),
        DevdSettings::default()
    );
}

#[test]
fn actions_are_readable() {
    for (a, want) in [
        ("key:Up", "Key Up"),
        ("ctl:brightness +40", "Brightness +40"),
        ("launch:browser", "Open browser"),
        ("run:firefox-esr", "Run firefox-esr (old)"),
        ("home", "Home"),
        ("keyboard", "Toggle keyboard"),
        ("none", "None"),
        ("", "None"),
        ("volume:+5", "Volume +5"),
    ] {
        assert_eq!(pretty_action(a), want);
    }
}

#[test]
fn gestures_from_keys_json() {
    let r = root_with(&[(
        "keys.json",
        r#"{"vol_up":{"single":"key:Up","long":"keyboard"},"vol_down":{"triple":"ctl:brightness -40"},"both":{"press":"ctl:rotate toggle"}}"#,
    )]);
    let g = read_gestures(&r);
    assert_eq!(g[0], ("Volume up · tap".to_string(), "Key Up".to_string()));
    assert!(g.contains(&(
        "Volume down · triple tap".to_string(),
        "Brightness -40".to_string()
    )));
    assert_eq!(
        g.last().unwrap(),
        &("Both buttons".to_string(), "Rotate toggle".to_string())
    );
    assert!(read_gestures(&root_with(&[])).is_empty());
}
