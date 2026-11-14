use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use walpad_hw::Root;
use walpad_proto::{valid_app_id, DEFAULT_ROLES};

pub const SCRIPT: &str = "/usr/local/sbin/walpad-pkg";
pub const CATALOGUE: &str = "/etc/walpad/catalogue.json";
pub const DEFAULTS: &str = "/etc/walpad/defaults.json";
pub const MIMEAPPS: &str = "/root/.config/mimeapps.list";
pub(super) const APK_DB: &str = "/lib/apk/db/installed";
pub(super) const OUT: &str = "/run/walpad/pkg-out";
pub(super) const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
/// Free space that must remain after an install (2 GiB).
pub const RESERVE_KB: u64 = 2 * 1024 * 1024;
pub(super) const INSTALL_TIMEOUT: Duration = Duration::from_secs(15 * 60 + 30);
pub(super) const REMOVE_TIMEOUT: Duration = Duration::from_secs(3 * 60);

/// File types each role opens (what mimeapps.list gets).
pub const ROLE_MIMES: &[(&str, &[&str])] = &[
    (
        "browser",
        &[
            "x-scheme-handler/http",
            "x-scheme-handler/https",
            "text/html",
        ],
    ),
    ("files", &["inode/directory"]),
    ("terminal", &[]),
    (
        "video",
        &[
            "video/mp4",
            "video/x-matroska",
            "video/webm",
            "video/x-msvideo",
            "video/mpeg",
            "video/quicktime",
            "video/ogg",
            "video/3gpp",
            "video/x-flv",
            "video/x-ms-wmv",
            "application/ogg",
        ],
    ),
    (
        "music",
        &[
            "audio/mpeg",
            "audio/flac",
            "audio/ogg",
            "audio/x-wav",
            "audio/mp4",
            "audio/x-vorbis+ogg",
            "audio/aac",
            "audio/x-flac",
            "audio/opus",
            "audio/x-opus+ogg",
            "audio/x-m4a",
            "audio/mp3",
            "audio/x-mp3",
            "audio/x-ms-wma",
        ],
    ),
    (
        "image",
        &[
            "image/png",
            "image/jpeg",
            "image/gif",
            "image/webp",
            "image/bmp",
            "image/tiff",
        ],
    ),
    ("pdf", &["application/pdf"]),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub packages: Vec<String>,
    pub exec: Vec<String>,
    pub desktop: String,
    pub icon: String,
    pub wm_class: String,
    pub size_kb: u64,
    pub summary: String,
    pub roles: Vec<String>,
    /// The app opens one file (`launch` with a path): `"open": true` (argv tail `-- {path}`) or an
    /// array with the argv tail. Written to the apps.d entry.
    pub open: Option<Vec<String>>,
    /// Shipped in the image: shown as built in, never uninstalled.
    pub system: bool,
}

/// Alpine package names: what the helper accepts too.
pub fn valid_package(p: &str) -> bool {
    let b = p.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'.' | b'_' | b'+' | b'-')
        })
        && !p.ends_with(".apk")
}

fn str_list(v: &Value) -> Option<Vec<String>> {
    v.as_array()?
        .iter()
        .map(|x| x.as_str().map(str::to_string))
        .collect()
}

impl Entry {
    pub(super) fn parse(v: &Value) -> Option<Entry> {
        let id = v["id"].as_str()?.to_string();
        if !valid_app_id(&id) || id.starts_with(['-', '_']) {
            return None;
        }
        let packages = str_list(&v["packages"])?;
        if packages.is_empty() || packages.len() > 8 || !packages.iter().all(|p| valid_package(p)) {
            return None;
        }
        let exec = str_list(&v["exec"]).unwrap_or_default();
        if exec.iter().any(|a| a.contains('\0')) {
            return None;
        }
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        Some(Entry {
            name: v["name"]
                .as_str()
                .filter(|n| !n.is_empty())
                .unwrap_or(&id)
                .to_string(),
            category: v["category"]
                .as_str()
                .filter(|n| !n.is_empty())
                .unwrap_or("Other")
                .to_string(),
            packages,
            exec,
            desktop: s("desktop"),
            icon: s("icon"),
            wm_class: s("wm_class"),
            size_kb: v["size_kb"].as_u64().unwrap_or(0),
            summary: s("summary"),
            roles: str_list(&v["roles"])
                .unwrap_or_default()
                .into_iter()
                .filter(|r| DEFAULT_ROLES.contains(&r.as_str()))
                .collect(),
            open: match &v["open"] {
                Value::Bool(true) => Some(vec!["--".to_string(), "{path}".to_string()]),
                a @ Value::Array(_) => str_list(a).filter(|l| {
                    l.iter().filter(|x| *x == "{path}").count() == 1
                        && l.iter().all(|x| !x.contains('\0'))
                }),
                _ => None,
            },
            system: v["system"].as_bool().unwrap_or(false),
            id,
        })
    }

    /// The apps.d entry of this app (None for package-only entries such as fonts).
    pub fn app_json(&self) -> Option<Value> {
        if self.exec.is_empty() {
            return None;
        }
        let mut j = json!({
            "id": self.id,
            "name": self.name,
            "icon": if self.icon.is_empty() { "apps" } else { self.icon.as_str() },
            "exec": self.exec,
            "env": {},
            "single_instance": !self.wm_class.is_empty(),
            "wm_class": self.wm_class,
        });
        if let Some(a) = &self.open {
            j["open"] = json!(true);
            j["open_args"] = json!(a);
        }
        Some(j)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalogue {
    pub entries: Vec<Entry>,
    pub reserve_kb: u64,
}

impl Catalogue {
    /// Entries that do not validate are dropped (a broken line never reaches apk).
    pub fn load(root: &Root) -> Catalogue {
        let v: Value = std::fs::read_to_string(root.p(CATALOGUE))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null);
        let mut entries: Vec<Entry> = Vec::new();
        for e in v["entries"].as_array().into_iter().flatten() {
            if let Some(e) = Entry::parse(e) {
                if !entries.iter().any(|x| x.id == e.id) {
                    entries.push(e);
                }
            }
        }
        Catalogue {
            entries,
            reserve_kb: v["reserve_kb"]
                .as_u64()
                .unwrap_or(RESERVE_KB)
                .max(RESERVE_KB),
        }
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

/// Names of the installed packages (`P:` lines of the apk database).
pub fn installed_packages(root: &Root) -> HashSet<String> {
    std::fs::read_to_string(root.p(APK_DB))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix("P:"))
        .map(|p| p.trim().to_string())
        .collect()
}

/// Is every package of the entry installed? (System entries are part of the image.)
pub fn is_installed(e: &Entry, pk: &HashSet<String>) -> bool {
    e.system || e.packages.iter().all(|p| pk.contains(p))
}

/// The free-space rule: an install may not leave less than `reserve` free.
pub fn check_space(free_kb: u64, need_kb: u64, reserve_kb: u64) -> Result<(), String> {
    if free_kb.saturating_sub(need_kb) >= reserve_kb {
        Ok(())
    } else {
        Err(format!(
            "Not enough free space: {} needed, {} free, {} must stay free",
            fmt_kb(need_kb),
            fmt_kb(free_kb),
            fmt_kb(reserve_kb)
        ))
    }
}

fn fmt_kb(kb: u64) -> String {
    let mb = kb as f64 / 1024.0;
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
