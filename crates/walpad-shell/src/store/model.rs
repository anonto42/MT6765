use serde_json::Value;
use walpad_proto::DEFAULT_ROLES;

/// Free space that must remain after an install (the core enforces it; shown here too).
pub const DEFAULT_RESERVE_KB: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct StoreEntry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub summary: String,
    pub size_kb: u64,
    pub installed: bool,
    pub system: bool,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Busy {
    pub op: String,
    pub id: String,
    pub percent: u8,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StoreView {
    /// Installed apps and default apps.
    #[default]
    Installed,
    /// The catalogue by category.
    Browse,
    /// Choose the app for one role (index into `DEFAULT_ROLES`).
    Role(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreTarget {
    Browse,
    Install(usize),
    Remove(usize),
    Role(usize),
    PickDefault(usize),
    DismissError,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StoreModel {
    pub view: StoreView,
    pub entries: Vec<StoreEntry>,
    pub loaded: bool,
    pub free_kb: Option<u64>,
    pub reserve_kb: u64,
    pub busy: Option<Busy>,
    /// Effective default app id per role (index = `DEFAULT_ROLES`).
    pub defaults: Vec<Option<String>>,
    pub error: Option<String>,
    /// An uninstall waiting for its confirming second tap (entry index).
    pub armed: Option<usize>,
}

pub fn role_label(role: &str) -> &'static str {
    match role {
        "browser" => "Web browser",
        "files" => "Files",
        "terminal" => "Terminal",
        "video" => "Video",
        "music" => "Music",
        "image" => "Images",
        "pdf" => "PDF documents",
        _ => "Other",
    }
}

pub fn fmt_size(kb: u64) -> String {
    let mb = kb as f64 / 1024.0;
    if kb < 1024 {
        format!("{kb} KB")
    } else if mb < 10.0 {
        format!("{mb:.1} MB")
    } else if mb < 1024.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{:.1} GB", mb / 1024.0)
    }
}

impl StoreModel {
    pub fn apply_catalogue(&mut self, v: &Value) {
        self.entries = v["entries"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|e| {
                        Some(StoreEntry {
                            id: e["id"].as_str()?.to_string(),
                            name: e["name"].as_str()?.to_string(),
                            category: e["category"].as_str().unwrap_or("Other").to_string(),
                            summary: e["summary"].as_str().unwrap_or("").to_string(),
                            size_kb: e["size_kb"].as_u64().unwrap_or(0),
                            installed: e["installed"].as_bool().unwrap_or(false),
                            system: e["system"].as_bool().unwrap_or(false),
                            roles: e["roles"]
                                .as_array()
                                .map(|r| {
                                    r.iter()
                                        .filter_map(|x| x.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.free_kb = v["free_kb"].as_u64();
        self.reserve_kb = v["reserve_kb"].as_u64().unwrap_or(DEFAULT_RESERVE_KB);
        self.defaults = DEFAULT_ROLES
            .iter()
            .map(|r| v["defaults"][*r].as_str().map(String::from))
            .collect();
        self.busy = v["busy"].as_object().map(|b| Busy {
            op: v["busy"]["op"].as_str().unwrap_or("install").to_string(),
            id: v["busy"]["id"].as_str().unwrap_or("").to_string(),
            percent: b
                .get("percent")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(100) as u8,
            text: v["busy"]["text"].as_str().unwrap_or("").to_string(),
        });
        self.loaded = true;
        self.armed = None;
    }

    pub fn name_of(&self, id: &str) -> String {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .map_or_else(|| id.to_string(), |e| e.name.clone())
    }

    /// Why an install would be refused for lack of space (the same rule the core applies).
    pub fn space_problem(&self, e: &StoreEntry) -> Option<String> {
        let free = self.free_kb?;
        let reserve = if self.reserve_kb == 0 {
            DEFAULT_RESERVE_KB
        } else {
            self.reserve_kb
        };
        (free.saturating_sub(e.size_kb) < reserve).then(|| {
            format!(
                "Not enough free space: {} needed, {} free, {} must stay free",
                fmt_size(e.size_kb),
                fmt_size(free),
                fmt_size(reserve)
            )
        })
    }

    pub(super) fn role_candidates(&self, role: &str) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.installed && e.roles.iter().any(|r| r == role))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn title(&self) -> &'static str {
        match self.view {
            StoreView::Installed => "Apps",
            StoreView::Browse => "App store",
            StoreView::Role(i) => DEFAULT_ROLES.get(i).map_or("Apps", |r| role_label(r)),
        }
    }
}
