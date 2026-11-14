use super::*;

#[test]
fn rows_show_the_split_buttons_only_in_split_mode() {
    let mut p = WindowsPrefs::default();
    assert_eq!(rows(&p).len(), 1);
    p.mode = LayoutMode::Split;
    assert_eq!(rows(&p).len(), 3);
    p.mode = LayoutMode::Free;
    assert_eq!(rows(&p).len(), 1);
}

#[test]
fn taps_cycle_mode_and_ratio_and_send_layout() {
    let p = WindowsPrefs::default();
    let (p2, c) = tap(&p, Target::WindowsMode).unwrap();
    assert_eq!(p2.mode, LayoutMode::Split);
    assert_eq!(
        c.request(),
        ("layout", serde_json::json!({"mode": "split"}))
    );
    let (p3, c) = tap(&p2, Target::WindowsRatio).unwrap();
    assert_eq!(p3.ratio, 67);
    assert_eq!(c.request(), ("layout", serde_json::json!({"ratio": 67})));
    let (_, c) = tap(&p3, Target::WindowsSwap).unwrap();
    assert_eq!(c.request(), ("layout", serde_json::json!({"swap": true})));
    assert!(tap(&p, Target::Back).is_none());
}

#[test]
fn prefs_come_from_shell_json_with_safe_defaults() {
    let dir = std::env::temp_dir().join(format!("walpad-wui-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("etc/walpad")).unwrap();
    let root = Root::new(&dir);
    assert_eq!(read_prefs(&root), WindowsPrefs::default());
    std::fs::write(
        dir.join("etc/walpad/shell.json"),
        r#"{"windows_mode":"free","split_ratio":67}"#,
    )
    .unwrap();
    assert_eq!(
        read_prefs(&root),
        WindowsPrefs {
            mode: LayoutMode::Free,
            ratio: 67
        }
    );
    std::fs::write(
        dir.join("etc/walpad/shell.json"),
        r#"{"windows_mode":"grid","split_ratio":12}"#,
    )
    .unwrap();
    assert_eq!(read_prefs(&root), WindowsPrefs::default());
    let _ = std::fs::remove_dir_all(&dir);
}
