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
    put(
        &r,
        "/sys/devices/platform/charger/ADC_Charger_Voltage",
        "4950\n",
    );
    let b2 = read_battery(&r);
    assert_eq!(
        (b2.charge_full_uah, b2.r_int_mohm, b2.charger_mv),
        (Some(2946000), Some(283), Some(4950))
    );
    put(&r, MASTER_CHG_LIMIT, "-1\n");
    put(&r, MTK_CURRENT_CMD, "0 0\n");
    set_charger_input_limit_ua(&r, 1_000_000).unwrap();
    set_charger_cmd(&r, false, true).unwrap();
    assert_eq!(read_trim(&r, MASTER_CHG_LIMIT).as_deref(), Some("1000000"));
    assert_eq!(read_trim(&r, MTK_CURRENT_CMD).as_deref(), Some("0 1"));
    assert!(b.charger_online);
    assert_eq!(b.capacity, Some(-1));
    assert_eq!(b.mv, Some(3880));
    assert_eq!(b.temp_dc, Some(340));
    assert_eq!(b.status.as_deref(), Some("Discharging"));
}
