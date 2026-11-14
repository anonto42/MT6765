//! Tests for `apps.d` overrides, file-opening path resolution, and theme env.

use super::tests::{reap_all, root_with};
use super::*;
use std::time::Instant;
use walpad_hw::Root;

#[test]
fn apps_d_adds_apps_but_never_replaces_the_shipped_ones() {
    let r = root_with(
        r#"[{"id":"sleeper","exec":["sleep","30"]}]"#,
        &["sleep", "true"],
    );
    let d = r.p("/var/lib/walpad/apps.d");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("quick.json"),
        r#"{"id":"quick","name":"Q","exec":["true"]}"#,
    )
    .unwrap();
    // an entry that tries to take over a shipped id, and ones the allowlist rules reject
    std::fs::write(
        d.join("sleeper.json"),
        r#"{"id":"sleeper","exec":["true"]}"#,
    )
    .unwrap();
    std::fs::write(d.join("bad.json"), r#"{"id":"rm -rf /","exec":["true"]}"#).unwrap();
    std::fs::write(d.join("empty.json"), r#"{"id":"empty","exec":[]}"#).unwrap();
    let mut a = Apps::load(&r);
    assert_eq!(a.def("sleeper").unwrap().exec, ["sleep", "30"]);
    assert!(a.def("quick").is_some() && a.def("empty").is_none() && a.def("rm -rf /").is_none());
    assert!(
        a.status(&r).iter().any(|s| s.app == "quick"),
        "installed: its executable exists"
    );
    assert_eq!(a.spawn("rm -rf /", &r), Err(LaunchError::Unknown));
    // reload sees an app registered later and one that was unregistered; children stay
    a.spawn("sleeper", &r).unwrap();
    std::fs::remove_file(d.join("quick.json")).unwrap();
    std::fs::write(d.join("late.json"), r#"{"id":"late","exec":["true"]}"#).unwrap();
    a.reload(&r);
    assert!(a.def("quick").is_none() && a.def("late").is_some());
    assert!(a.owns("sleeper"), "a running app survives the reload");
    a.request_close("sleeper", Instant::now());
    reap_all(&mut a, 1);
}

#[test]
fn bad_json_means_no_apps() {
    let r = root_with("not json", &[]);
    assert!(Apps::load(&r).status(&r).is_empty());
}

fn media_root() -> Root {
    let r = root_with("[]", &["sh", "cat"]);
    for d in [
        "root/Pictures",
        "mnt/sd/DCIM",
        "etc",
        "tmp/Downloads",
        "tmp/other",
    ] {
        std::fs::create_dir_all(r.p(&format!("/{d}"))).unwrap();
    }
    std::fs::write(r.p("/root/Pictures/a.png"), "x").unwrap();
    std::fs::write(r.p("/mnt/sd/DCIM/b.mp4"), "x").unwrap();
    std::fs::write(r.p("/root/Pictures/-rf.png"), "x").unwrap();
    std::fs::write(r.p("/etc/shadow"), "secret").unwrap();
    std::fs::write(r.p("/tmp/other/c.png"), "x").unwrap();
    r
}

#[test]
fn open_path_accepts_files_under_the_allowed_roots_only() {
    use std::os::unix::fs::symlink;
    let r = media_root();
    assert_eq!(
        resolve_open_path(&r, "/root/Pictures/a.png").unwrap(),
        "/root/Pictures/a.png"
    );
    assert_eq!(
        resolve_open_path(&r, "/mnt/sd/DCIM/b.mp4").unwrap(),
        "/mnt/sd/DCIM/b.mp4"
    );
    // traversal, relative, option-looking, outside the roots, a folder, a missing file
    for bad in [
        "/root/Pictures/../../etc/shadow",
        "/root/../etc/shadow",
        "root/Pictures/a.png",
        "-rf",
        "--help",
        "/root/Pictures/-rf.png",
        "/etc/shadow",
        "/tmp/other/c.png",
        "/root/Pictures",
        "/root/Pictures/missing.png",
        "/root/Pictures/a\n.png",
        "/rootx/a.png",
        "",
    ] {
        assert!(
            resolve_open_path(&r, bad).is_err(),
            "{bad:?} must be refused"
        );
    }
    // symlinks: one that stays inside the roots works and is resolved, escapes are refused
    symlink(r.p("/root/Pictures/a.png"), r.p("/root/Pictures/link.png")).unwrap();
    symlink(r.p("/etc/shadow"), r.p("/root/Pictures/evil.png")).unwrap();
    symlink(r.p("/etc"), r.p("/mnt/sd/etcdir")).unwrap();
    symlink(r.p("/tmp/other"), r.p("/tmp/Downloads/o")).unwrap();
    assert_eq!(
        resolve_open_path(&r, "/root/Pictures/link.png").unwrap(),
        "/root/Pictures/a.png"
    );
    for bad in [
        "/root/Pictures/evil.png",
        "/mnt/sd/etcdir/shadow",
        "/tmp/Downloads/o/c.png",
    ] {
        assert!(resolve_open_path(&r, bad).is_err(), "{bad:?} escapes");
    }
}

#[test]
fn open_builds_the_argv_itself_and_only_for_apps_that_declare_it() {
    let r = media_root();
    let out = r.p("/argv-seen");
    let json = serde_json::json!([
        {"id":"viewer","exec":["sh","-c","for a in \"$@\"; do echo \"$a\"; done > \"$OUT\"","sh"],
         "open":true,"env":{"OUT":out.to_string_lossy()}},
        {"id":"plain","exec":["sh","-c","true"]}
    ]);
    std::fs::write(r.p("/etc/walpad/apps.json"), json.to_string()).unwrap();
    let mut a = Apps::load(&r);
    let d = a.def("viewer").unwrap().clone();
    assert_eq!(
        Apps::argv(&d, Some("/root/Pictures/a.png")),
        [
            "sh",
            "-c",
            "for a in \"$@\"; do echo \"$a\"; done > \"$OUT\"",
            "sh",
            "--",
            "/root/Pictures/a.png"
        ]
    );
    assert_eq!(Apps::argv(&d, None).len(), 4);
    assert!(matches!(
        a.spawn_open("plain", &r, Some("/root/Pictures/a.png")),
        Err(LaunchError::Spawn(_))
    ));
    a.spawn_open("viewer", &r, Some("/root/Pictures/a.png"))
        .unwrap();
    reap_all(&mut a, 1);
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "--\n/root/Pictures/a.png\n"
    );
}

#[test]
fn spawned_apps_receive_gtk_theme_from_shell_json() {
    let r = media_root();
    let out = r.p("/gtk-theme-seen");
    let json = serde_json::json!([
        {"id":"probe","exec":["sh","-c","printf '%s' \"$GTK_THEME\" > \"$OUT\""],"env":{"OUT":out.to_string_lossy()}}
    ]);
    std::fs::write(r.p("/etc/walpad/apps.json"), json.to_string()).unwrap();

    let mut a = Apps::load(&r);
    a.spawn("probe", &r).unwrap();
    reap_all(&mut a, 1);
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "Adwaita:dark");

    std::fs::write(r.p("/etc/walpad/shell.json"), r#"{"theme":"light"}"#).unwrap();
    a.spawn("probe", &r).unwrap();
    reap_all(&mut a, 1);
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "Adwaita:light");
}
