use super::*;

fn dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("walpad-appdefs-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("apps.d")).unwrap();
    d
}
fn ids(v: &[Value]) -> Vec<&str> {
    v.iter().filter_map(|e| e["id"].as_str()).collect()
}

#[test]
fn apps_d_extends_but_never_shadows_apps_json() {
    let d = dir("merge");
    std::fs::write(
        d.join("apps.json"),
        r#"[{"id":"browser","exec":["firefox-esr"]}]"#,
    )
    .unwrap();
    let w = |n: &str, c: &str| std::fs::write(d.join("apps.d").join(n), c).unwrap();
    w(
        "mousepad.json",
        r#"{"id":"mousepad","name":"Mousepad","exec":["mousepad"]}"#,
    );
    w("evil.json", r#"{"id":"browser","exec":["rm","-rf","/"]}"#);
    w(
        "two.json",
        r#"[{"id":"a","exec":["a"]},{"id":"mousepad","exec":["x"]},{"id":"b","exec":["b"]}]"#,
    );
    w("bad-id.json", r#"{"id":"Bad Id","exec":["x"]}"#);
    w("no-exec.json", r#"{"id":"noexec","exec":[]}"#);
    w("not-strings.json", r#"{"id":"ns","exec":[1]}"#);
    w("broken.json", "{");
    w("note.txt", r#"{"id":"txt","exec":["x"]}"#);
    let e = entries(&d.join("apps.json"), &d.join("apps.d"));
    assert_eq!(ids(&e), ["browser", "mousepad", "a", "b"]);
    let browser = e.iter().find(|x| x["id"] == "browser").unwrap();
    assert_eq!(browser["exec"][0], "firefox-esr", "apps.json wins");
    let mp = e.iter().find(|x| x["id"] == "mousepad").unwrap();
    assert_eq!(
        mp["exec"][0], "mousepad",
        "the first file in name order wins"
    );
}

#[test]
fn missing_files_mean_no_entries() {
    let d = dir("none");
    assert!(entries(&d.join("nope.json"), &d.join("nodir")).is_empty());
    std::fs::write(d.join("apps.json"), "not json").unwrap();
    assert!(entries(&d.join("apps.json"), &d.join("apps.d")).is_empty());
}
