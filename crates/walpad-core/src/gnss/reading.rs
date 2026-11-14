use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn field<'a>(out: &'a str, key: &str) -> Option<&'a str> {
    out.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

pub(super) fn clean_reason(s: &str) -> String {
    let t: String = s.chars().filter(|c| !c.is_control()).take(120).collect();
    t.trim().to_string()
}

pub(super) fn wall_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Round for display: 5 decimals is about one metre.
fn round5(v: f64) -> f64 {
    (v * 1e5).round() / 1e5
}

/// The part of gnss.json the status needs; `None` = unreadable or not a gnss document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reading {
    pub state: String,
    pub reason: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub alt_m: Option<i32>,
    pub accuracy_m: Option<u32>,
    pub used: Option<u32>,
    pub view: Option<u32>,
    pub age_s: Option<u32>,
    pub signals: Vec<u8>,
    pub updated_ms: i64,
}

pub fn parse_reading(text: &str, now_ms: i64) -> Option<Reading> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let state = v.get("state")?.as_str()?.to_string();
    let updated_ms = v.get("updated").and_then(|u| u.as_i64()).unwrap_or(0);
    let mut r = Reading {
        state,
        reason: v
            .get("reason")
            .and_then(|x| x.as_str())
            .map(clean_reason)
            .filter(|s| !s.is_empty()),
        updated_ms,
        ..Reading::default()
    };
    if let Some(f) = v.get("fix").filter(|f| f.is_object()) {
        let lat = f.get("lat").and_then(|x| x.as_f64());
        let lon = f.get("lon").and_then(|x| x.as_f64());
        if let (Some(la), Some(lo)) = (lat, lon) {
            if (-90.0..=90.0).contains(&la) && (-180.0..=180.0).contains(&lo) {
                r.lat = Some(round5(la));
                r.lon = Some(round5(lo));
                r.alt_m = f
                    .get("alt")
                    .and_then(|x| x.as_f64())
                    .filter(|a| a.is_finite() && a.abs() < 100_000.0)
                    .map(|a| a.round() as i32);
                r.accuracy_m = f
                    .get("accuracy")
                    .and_then(|x| x.as_f64())
                    .filter(|a| a.is_finite() && *a >= 0.0 && *a < 1.0e6)
                    .map(|a| a.round() as u32);
                let age_ms = f.get("age_ms").and_then(|x| x.as_i64()).unwrap_or(0).max(0);
                // the file is rewritten every second; time since it was written counts too
                let since = if updated_ms > 0 {
                    (now_ms - updated_ms).max(0)
                } else {
                    0
                };
                r.age_s = Some(((age_ms + since) / 1000).min(u32::MAX as i64) as u32);
            }
        }
    }
    if let Some(s) = v.get("satellites") {
        r.view = s
            .get("view")
            .and_then(|x| x.as_u64())
            .map(|n| n.min(999) as u32);
        r.used = s
            .get("used")
            .and_then(|x| x.as_u64())
            .map(|n| n.min(999) as u32);
        if let Some(list) = s.get("list").and_then(|x| x.as_array()) {
            let mut sig: Vec<u8> = list
                .iter()
                .map(|e| e.get("snr").and_then(|x| x.as_u64()).unwrap_or(0).min(99) as u8)
                .collect();
            sig.sort_unstable_by(|a, b| b.cmp(a));
            sig.truncate(16);
            r.signals = sig;
        }
    }
    Some(r)
}
