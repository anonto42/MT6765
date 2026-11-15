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
mod tests;
