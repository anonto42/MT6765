use super::reading::{clean_reason, field};
use super::*;
use serde_json::json;

fn doc(state: &str, fix: bool, updated: i64) -> String {
    json!({
        "v": 1, "state": state, "updated": updated,
        "fix": if fix { json!({"lat": 23.7512345, "lon": 90.4, "alt": 12.4, "accuracy": 6.6,
                               "age_ms": 3000}) } else { json!(null) },
        "satellites": {"view": 3, "used": 2, "list": [
            {"snr": 20}, {"snr": null}, {"snr": 41}]},
    })
    .to_string()
}

#[test]
fn reading_rounds_for_display_and_orders_signals() {
    let r = parse_reading(&doc("fix", true, 1_000_000), 1_002_000).unwrap();
    assert_eq!(r.state, "fix");
    assert_eq!(r.lat, Some(23.75123));
    assert_eq!(r.lon, Some(90.4));
    assert_eq!(r.alt_m, Some(12));
    assert_eq!(r.accuracy_m, Some(7));
    // 3000 ms at the fix + 2000 ms since the file was written
    assert_eq!(r.age_s, Some(5));
    assert_eq!((r.view, r.used), (Some(3), Some(2)));
    assert_eq!(r.signals, vec![41, 20, 0]);
}

#[test]
fn reading_without_fix_or_with_bad_numbers() {
    let r = parse_reading(&doc("searching", false, 5), 5).unwrap();
    assert!(r.lat.is_none() && r.age_s.is_none());
    let bad = json!({"state":"fix","fix":{"lat":123.0,"lon":1.0}}).to_string();
    assert!(parse_reading(&bad, 0).unwrap().lat.is_none());
    assert!(parse_reading("not json", 0).is_none());
    assert!(parse_reading("{}", 0).is_none());
    assert!(parse_reading("[1]", 0).is_none());
}

#[test]
fn script_fields() {
    let out = "[gps] x\nstate=failed\nreason=turn Wi-Fi on first\x07\n";
    assert_eq!(field(out, "state"), Some("failed"));
    assert_eq!(
        clean_reason(field(out, "reason").unwrap()),
        "turn Wi-Fi on first"
    );
    assert_eq!(clean_reason(&"y".repeat(400)).len(), 120);
}
