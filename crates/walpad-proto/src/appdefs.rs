//! App definitions shared by the core (launching) and the shell (names, icons, window classes):
//! the shipped `/etc/walpad/apps.json` plus the user-writable `apps.d/` directory the app store
//! registers installed apps in (change `app-catalogue-manager`).
//!
//! Allowlist semantics are unchanged: an entry is only a name and an argv, apps still start by id,
//! and an `apps.d` entry can never replace (shadow) an id of `apps.json` or of an earlier file.
use serde_json::Value;
use std::path::Path;

/// Where the core registers installed catalogue apps (one `<id>.json` file per app).
pub const APPS_D: &str = "/var/lib/walpad/apps.d";
/// The shipped allowlist.
pub const APPS_JSON: &str = "/etc/walpad/apps.json";

fn entry_ok(e: &Value) -> bool {
    let Some(id) = e["id"].as_str() else {
        return false;
    };
    crate::valid_app_id(id)
        && e["exec"]
            .as_array()
            .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
}

/// Every app entry as JSON: the `apps.json` array (when it parses) followed by the `apps.d/*.json`
/// files in name order. A file holds one entry object or an array of them. Entries without a valid
/// id or a non-empty string `exec`, unreadable files and ids already taken are skipped.
pub fn entries(apps_json: &Path, apps_d: &Path) -> Vec<Value> {
    let mut out: Vec<Value> = std::fs::read_to_string(apps_json)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
        .unwrap_or_default();
    let mut files: Vec<_> = std::fs::read_dir(apps_d)
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    for f in files {
        let Some(v) = std::fs::read_to_string(&f)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        else {
            continue;
        };
        let list = match v {
            Value::Array(a) => a,
            o @ Value::Object(_) => vec![o],
            _ => continue,
        };
        for e in list {
            if !entry_ok(&e) {
                continue;
            }
            let id = e["id"].as_str().unwrap_or_default();
            if out.iter().any(|x| x["id"].as_str() == Some(id)) {
                continue;
            }
            out.push(e);
        }
    }
    out
}

#[cfg(test)]
mod tests {
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
}
