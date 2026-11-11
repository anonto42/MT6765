use crate::{sysfs::read_trim, Root};

const PS: &str = "/sys/class/power_supply/battery";

/// Raw power-supply readings; every field is None when unavailable.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RawBattery {
    /// Gauge capacity as reported (the stock gauge reads -1).
    pub capacity: Option<i32>,
    pub status: Option<String>,
    pub mv: Option<u32>,
    /// Tenths of a degree C as reported.
    pub temp_dc: Option<i32>,
    /// Battery current in microamps as reported (positive = charging on this kernel); None = unreadable.
    pub current_ua: Option<i64>,
    /// Some charger/USB/mains supply reports `online = 1` (the battery status alone lags or says
    /// "Discharging"/"Not charging" with chargers the kernel does not classify as standard).
    pub charger_online: bool,
    /// `charge_full` in microamp-hours (gauge-learned, a hint only).
    pub charge_full_uah: Option<u64>,
    /// Internal resistance in milliohm from the MediaTek gauge (`ptim_resist_get`), unchecked.
    pub r_int_mohm: Option<u32>,
}

/// Every supply that can only be a power source (not the battery or the fuel gauge).
const SOURCES: [&str; 4] = ["ac", "usb", "mtk_charger_type", "wireless"];

pub fn read_battery(root: &Root) -> RawBattery {
    let g = |n: &str| read_trim(root, &format!("{PS}/{n}"));
    RawBattery {
        capacity: g("capacity").and_then(|s| s.parse().ok()),
        status: g("status"),
        mv: g("voltage_now").and_then(|s| parse_voltage_mv(&s)),
        temp_dc: g("temp").and_then(|s| s.parse().ok()),
        current_ua: g("current_now").and_then(|s| s.parse().ok()),
        charge_full_uah: g("charge_full").and_then(|s| s.parse().ok()),
        r_int_mohm: read_trim(root, "/sys/class/power_supply/mtk-gauge/ptim_resist_get")
            .and_then(|s| s.parse().ok()),
        charger_online: SOURCES.iter().any(|n| {
            read_trim(root, &format!("/sys/class/power_supply/{n}/online")).as_deref() == Some("1")
        }),
    }
}

/// voltage_now is in uV (divide by 1000 when above 100000), already mV otherwise.
pub fn parse_voltage_mv(s: &str) -> Option<u32> {
    let v: u64 = s.trim().parse().ok()?;
    Some(if v > 100_000 {
        (v / 1000) as u32
    } else {
        v as u32
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{put, tmp};

    #[test]
    fn voltage_units() {
        assert_eq!(parse_voltage_mv("3920000"), Some(3920));
        assert_eq!(parse_voltage_mv("3920"), Some(3920));
        assert_eq!(parse_voltage_mv("x"), None);
    }
    #[test]
    fn reads_tree_and_missing() {
        let r = Root::new(tmp());
        assert_eq!(read_battery(&r), RawBattery::default());
        put(&r, &format!("{PS}/capacity"), "-1\n");
        put(&r, &format!("{PS}/status"), "Discharging\n");
        put(&r, &format!("{PS}/voltage_now"), "3880000\n");
        put(&r, &format!("{PS}/temp"), "340\n");
        put(&r, &format!("{PS}/current_now"), "685500\n");
        put(&r, "/sys/class/power_supply/usb/online", "0\n");
        put(&r, "/sys/class/power_supply/mtk_charger_type/online", "1\n");
        let b = read_battery(&r);
        assert_eq!(b.current_ua, Some(685500));
        assert_eq!((b.charge_full_uah, b.r_int_mohm), (None, None));
        put(&r, &format!("{PS}/charge_full"), "2946000\n");
        put(
            &r,
            "/sys/class/power_supply/mtk-gauge/ptim_resist_get",
            "283\n",
        );
        let b2 = read_battery(&r);
        assert_eq!(
            (b2.charge_full_uah, b2.r_int_mohm),
            (Some(2946000), Some(283))
        );
        assert!(b.charger_online);
        assert_eq!(b.capacity, Some(-1));
        assert_eq!(b.mv, Some(3880));
        assert_eq!(b.temp_dc, Some(340));
        assert_eq!(b.status.as_deref(), Some("Discharging"));
    }
}
