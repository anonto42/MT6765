//! Core app allowlist, spawning, and reaping tests.

use super::*;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::AppState;

pub(super) fn root_with(apps: &str, bins: &[&str]) -> Root {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("walpad-apps-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("etc/walpad")).unwrap();
    std::fs::create_dir_all(d.join("usr/bin")).unwrap();
    std::fs::write(d.join("etc/walpad/apps.json"), apps).unwrap();
    for b in bins {
        std::fs::write(d.join("usr/bin").join(b), "").unwrap();
    }
    Root::new(d)
}

pub(super) fn reap_all(a: &mut Apps, want: usize) -> Vec<(String, Exit)> {
    let t = Instant::now();
    let mut gone = vec![];
    while gone.len() < want && t.elapsed() < Duration::from_secs(5) {
        gone.extend(a.reap(Instant::now()));
        std::thread::sleep(Duration::from_millis(20));
    }
    gone
}

const JSON: &str = r#"[
    {"id":"sleeper","name":"S","exec":["sleep","30"],"single_instance":true,"wm_class":"sleeper"},
    {"id":"ghost","name":"G","exec":["definitely-not-installed"]},
    {"id":"","exec":["x"]},
    {"id":"noexec","exec":[]}
]"#;

#[test]
fn touch_mode_per_app_with_scroll_as_the_default() {
    use crate::logic::gesture::Mode;
    let r = root_with(
        r#"[
          {"id":"term","exec":["st"],"wm_class":"st","touch":"mouse"},
          {"id":"web","exec":["firefox-esr"],"wm_class":"firefox-esr","touch":"scroll"},
          {"id":"odd","exec":["x"],"wm_class":"oddapp","touch":"bogus"},
          {"id":"none","exec":["y"],"wm_class":"plain"}
        ]"#,
        &[],
    );
    let a = Apps::load(&r);
    let c = |i: &str, k: &str| vec![i.to_string(), k.to_string()];
    assert_eq!(a.touch_mode(&c("st", "St")), Mode::Mouse);
    assert_eq!(a.touch_mode(&c("Navigator", "Firefox-esr")), Mode::Scroll);
    assert_eq!(a.touch_mode(&c("oddapp", "Oddapp")), Mode::Scroll);
    assert_eq!(a.touch_mode(&c("plain", "Plain")), Mode::Scroll);
    assert_eq!(a.touch_mode(&c("stranger", "Stranger")), Mode::Scroll);
    assert_eq!(a.touch_mode(&[]), Mode::Scroll);
}

#[test]
fn loads_filters_and_reports_installed_only() {
    // The child ignores SIGTERM, so it is still alive while "closing" (a plain `sleep` can die
    // in milliseconds and make the Closing assertion below race); SIGKILL ends it after the grace period.
    let json = JSON.replace(
        r#"["sleep","30"]"#,
        r#"["sh","-c","trap '' TERM; exec sleep 30"]"#,
    );
    let r = root_with(&json, &["sh"]);
    let mut a = Apps::load(&r);
    assert!(a.def("noexec").is_none() && a.def("").is_none());
    let st = a.status(&r);
    assert_eq!(
        st,
        vec![AppState {
            app: "sleeper".into(),
            running: false
        }],
        "ghost is not installed, so it is hidden"
    );
    assert_eq!(a.spawn("ghost", &r), Err(LaunchError::NotInstalled));
    assert_eq!(a.spawn("rm -rf /", &r), Err(LaunchError::Unknown));
    a.spawn("sleeper", &r).unwrap();
    assert!(a.running("sleeper"));
    assert!(a.status(&r)[0].running);
    assert!(a.request_close("sleeper", Instant::now()));
    assert!(!a.running("sleeper"), "closing counts as not running");
    assert_eq!(
        a.spawn("sleeper", &r),
        Err(LaunchError::Closing),
        "an old copy must not be forgotten"
    );
    let gone = reap_all(&mut a, 1);
    assert_eq!(gone, vec![("sleeper".to_string(), Exit::Closed)]);
    assert!(!a.owns("sleeper"));
}

#[test]
fn apps_get_the_system_proxy_in_their_environment() {
    let r = root_with("[]", &["sh"]);
    let out = r.p("/proxy-seen");
    let json = serde_json::json!([{
        "id": "envdump",
        "exec": ["sh", "-c", "echo $http_proxy $HTTPS_PROXY $no_proxy > \"$OUT\""],
        "env": {"OUT": out.to_string_lossy()}
    }]);
    std::fs::write(r.p("/etc/walpad/apps.json"), json.to_string()).unwrap();
    let mut a = Apps::load(&r);
    a.set_proxy(Some("http://10.0.0.2:3128"));
    a.spawn("envdump", &r).unwrap();
    reap_all(&mut a, 1);
    assert_eq!(
        std::fs::read_to_string(&out).unwrap().trim(),
        "http://10.0.0.2:3128 http://10.0.0.2:3128 localhost,127.0.0.1,::1"
    );
}

#[test]
fn app_that_disappears_from_disk_is_unlisted_again() {
    let r = root_with(JSON, &["sleep"]);
    let a = Apps::load(&r);
    assert_eq!(a.status(&r).len(), 1);
    std::fs::remove_file(r.p("/usr/bin/sleep")).unwrap();
    assert!(a.status(&r).is_empty(), "missing executable: not listed");
}

#[test]
fn exit_kinds_clean_crash_signal_exit_ok_and_closed() {
    let json = r#"[
        {"id":"ok","exec":["true"]},
        {"id":"bad","exec":["false"]},
        {"id":"term","exec":["sh","-c","kill -9 $$"]},
        {"id":"shellish","exec":["false"],"exit_ok":[1]}
    ]"#;
    let r = root_with(json, &["true", "false", "sh"]);
    let mut a = Apps::load(&r);
    for id in ["ok", "bad", "term", "shellish"] {
        a.spawn(id, &r).unwrap();
    }
    let mut gone = reap_all(&mut a, 4);
    gone.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(
        gone,
        vec![
            (
                "bad".to_string(),
                Exit::Crashed {
                    code: Some(1),
                    signal: None
                }
            ),
            ("ok".to_string(), Exit::Clean),
            ("shellish".to_string(), Exit::Clean),
            (
                "term".to_string(),
                Exit::Crashed {
                    code: None,
                    signal: Some(9)
                }
            ),
        ]
    );
}

#[test]
fn closing_child_that_ignores_sigterm_is_killed_after_the_grace_period() {
    let json =
        r#"[{"id":"stubborn","exec":["sh","-c","trap '' TERM; while :; do sleep 1; done"]}]"#;
    let r = root_with(json, &["sh"]);
    let mut a = Apps::load(&r);
    a.set_close_grace(Duration::from_millis(300));
    a.spawn("stubborn", &r).unwrap();
    std::thread::sleep(Duration::from_millis(200)); // let sh install its trap
    assert!(a.request_close("stubborn", Instant::now()));
    let gone = reap_all(&mut a, 1);
    assert_eq!(gone, vec![("stubborn".to_string(), Exit::Closed)]);
}

#[test]
fn ext_windows_count_as_running_and_report_changes() {
    let r = root_with(JSON, &["sleep"]);
    let mut a = Apps::load(&r);
    let set = |ids: &[&str]| ids.iter().map(|s| s.to_string()).collect::<HashSet<_>>();
    assert_eq!(
        a.set_ext(set(&["sleeper"])),
        vec![("sleeper".to_string(), true)]
    );
    assert!(a.running("sleeper") && a.status(&r)[0].running);
    assert!(a.set_ext(set(&["sleeper"])).is_empty());
    assert_eq!(a.set_ext(set(&[])), vec![("sleeper".to_string(), false)]);
    assert!(!a.running("sleeper"));
}

#[test]
fn reaps_exited_children() {
    let r = root_with(r#"[{"id":"quick","exec":["true"]}]"#, &["true"]);
    let mut a = Apps::load(&r);
    a.spawn("quick", &r).unwrap();
    let gone = reap_all(&mut a, 1);
    assert_eq!(gone, vec![("quick".to_string(), Exit::Clean)]);
    assert!(!a.running("quick"));
}
