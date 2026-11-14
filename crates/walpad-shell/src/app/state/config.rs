//! Shell configuration readers, catalog loading, and wall-clock time formatting.

use serde_json::Value;
use walpad_hw::Root;
use walpad_ui::Icon;

#[derive(Debug, Clone, PartialEq)]
pub struct WallTime {
    pub hour: u8,
    pub minute: u8,
    /// "AM"/"PM" in 12-hour mode.
    pub suffix: Option<&'static str>,
    /// e.g. "Monday, 09 November".
    pub date: String,
}

impl WallTime {
    pub fn new(hour: u8, minute: u8, date: &str, clock_24h: bool) -> WallTime {
        if clock_24h {
            WallTime {
                hour,
                minute,
                suffix: None,
                date: date.into(),
            }
        } else {
            let h12 = if hour.is_multiple_of(12) {
                12
            } else {
                hour % 12
            };
            WallTime {
                hour: h12,
                minute,
                suffix: Some(if hour >= 12 { "PM" } else { "AM" }),
                date: date.into(),
            }
        }
    }

    /// Local time now and the milliseconds until the next minute boundary.
    pub fn now(clock_24h: bool) -> (WallTime, u64) {
        let z = jiff::Zoned::now();
        let ms_in_minute = z.second() as u64 * 1000 + (z.subsec_nanosecond() as u64) / 1_000_000;
        let date = z.strftime("%A, %d %B").to_string();
        (
            WallTime::new(z.hour() as u8, z.minute() as u8, &date, clock_24h),
            60_000u64.saturating_sub(ms_in_minute).max(1),
        )
    }
}

/// An app known from /etc/walpad/apps.json.
#[derive(Debug, Clone, PartialEq)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub icon: Icon,
}

pub fn load_catalog(root: &Root) -> Vec<AppEntry> {
    // apps.json plus the apps the app store registered in apps.d
    walpad_proto::appdefs::entries(
        &root.p(walpad_proto::appdefs::APPS_JSON),
        &root.p(walpad_proto::appdefs::APPS_D),
    )
    .iter()
    .filter_map(|e| {
        let id = e["id"].as_str()?.to_string();
        let name = e["name"].as_str().unwrap_or(&id).to_string();
        let icon = Icon::from_name(e["icon"].as_str().unwrap_or("")).unwrap_or(Icon::Apps);
        Some(AppEntry { id, name, icon })
    })
    .collect()
}

fn shell_json(root: &Root) -> Value {
    std::fs::read_to_string(root.p("/etc/walpad/shell.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null)
}

pub fn read_clock_24h(root: &Root) -> bool {
    shell_json(root)["clock_24h"].as_bool().unwrap_or(true)
}

pub fn read_theme(root: &Root) -> walpad_ui::tokens::ThemeMode {
    shell_json(root)["theme"]
        .as_str()
        .map(walpad_ui::tokens::ThemeMode::from_str_or_dark)
        .unwrap_or_default()
}

pub fn read_wallpaper(root: &Root) -> walpad_ui::art::WallpaperKind {
    shell_json(root)["wallpaper"]
        .as_str()
        .map(walpad_ui::art::WallpaperKind::parse)
        .unwrap_or_default()
}

pub fn read_wallpaper_path(root: &Root) -> String {
    shell_json(root)["wallpaper_path"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

pub const DEFAULT_PINNED: [&str; 4] = ["browser", "terminal", "files", "settings"];

pub fn read_pinned(root: &Root) -> Vec<String> {
    shell_json(root)["pinned_apps"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_else(|| DEFAULT_PINNED.iter().map(|s| s.to_string()).collect())
}
