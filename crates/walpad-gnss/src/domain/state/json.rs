use super::{Counters, Snapshot};
use serde_json::{json, Value};

/// The published document /run/walpad/gnss.json.  Coordinates are private: the daemon never logs
/// this text, and the file is for walpad-core and local tools.
pub fn to_json(s: &Snapshot, wall_ms: i64, c: &Counters) -> String {
    let fix = s.fix.map(|f| {
        json!({
            "lat": f.lat, "lon": f.lon, "alt": f.alt, "speed": f.speed,
            "bearing": f.bearing, "accuracy": f.accuracy, "time": f.time_ms,
            "age_ms": s.fix_age_ms,
        })
    });
    let list: Vec<Value> = s
        .sats
        .iter()
        .map(|t| {
            json!({
                "sys": t.sys.name(), "id": t.info.id, "snr": t.info.snr,
                "elev": t.info.elev, "az": t.info.az, "used": t.used,
            })
        })
        .collect();
    let v = json!({
        "v": 1,
        "state": s.state.name(),
        "engine": s.engine,
        "reason": s.reason,
        "updated": wall_ms,
        "fix": fix,
        "satellites": {"view": s.view, "used": s.used, "list": list},
        "counters": {
            "location": c.location, "status": c.status, "sv": c.sv, "nmea": c.nmea,
            "bad_nmea": c.bad_nmea, "utc_requests": c.utc_requests, "other": c.other,
        },
    });
    v.to_string()
}
