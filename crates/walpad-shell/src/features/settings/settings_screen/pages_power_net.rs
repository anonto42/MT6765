//! Power and Network settings page row builders.

use super::pages::{info, opt};
use super::{Ctx, Page, Row};
use crate::app::Target;
use walpad_ui::components::Tone;

fn secs(s: u32) -> String {
    match s {
        0 => "Never".into(),
        s if s < 60 => format!("{s} s"),
        s => format!("{} min", s / 60),
    }
}

pub(super) fn power_rows(c: &Ctx) -> Vec<Row> {
    let b = c.status.as_ref().map(|s| &s.battery);
    let on_charger = b
        .and_then(|b| b.state.as_deref())
        .is_some_and(|s| s == "Charging" || s == "Full");
    let power_path = if on_charger {
        if c.devd.charge_mode == "bypass" {
            "Charger powering device (Bypass)".into()
        } else if c.devd.charge_limit_pct < 100
            && b.and_then(|b| b.percent)
                .is_some_and(|p| u32::from(p) >= c.devd.charge_limit_pct)
        {
            format!(
                "Charger powering device ({}% held)",
                c.devd.charge_limit_pct
            )
        } else {
            "Charger powering device + charging".into()
        }
    } else {
        "Running on battery".into()
    };
    let charge_mode = match c.devd.charge_mode.as_str() {
        "bypass" => "Bypass (Run on Charger)",
        "eco" => "Eco (Cool 1000 mA)",
        "fast" => "Fast (2000 mA)",
        _ => "Smart (Charger + Safe Charge)",
    };
    let charge_limit = if c.devd.charge_limit_pct >= 100 {
        "100% (Full)".into()
    } else {
        format!("{}% (Protect)", c.devd.charge_limit_pct)
    };
    vec![
        Row::Info("Power path".into(), power_path, Tone::Info),
        Row::Choice(
            "Charging mode".into(),
            charge_mode.into(),
            Target::ChargeModeCycle,
        ),
        Row::Choice(
            "Charge limit".into(),
            charge_limit,
            Target::ChargeLimitCycle,
        ),
        Row::Choice(
            "Max charger current".into(),
            format!("{} mA", c.devd.charge_current_ma),
            Target::ChargeCurrentCycle,
        ),
        Row::Choice(
            "Screen dims after".into(),
            secs(c.devd.dim_after_s),
            Target::PowerDimCycle,
        ),
        Row::Choice(
            "Dimmed brightness".into(),
            format!("{}%", (c.devd.dim_level * 100 / 255).max(1)),
            Target::PowerDimLevelCycle,
        ),
        Row::Choice(
            "Screen turns off after".into(),
            secs(c.devd.off_after_s),
            Target::PowerOffCycle,
        ),
        Row::Toggle(
            "Turn off when cover closes",
            c.devd.lid_off,
            Target::LidToggle,
        ),
        Row::Choice(
            "Low battery below".into(),
            format!("{} mV", c.devd.low_batt_mv),
            Target::LowBattCycle,
        ),
        Row::Choice(
            "Shuts down below".into(),
            format!("{} mV", c.devd.critical_batt_mv),
            Target::CritBattCycle,
        ),
        Row::Info(
            "Power button · tap".into(),
            "Screen off / on".into(),
            Tone::Info,
        ),
        Row::Info(
            "Power button · hold".into(),
            "Power menu".into(),
            Tone::Info,
        ),
        Row::Action {
            label: "Power menu",
            target: Target::PowerMenu,
            danger: false,
        },
    ]
}

pub(super) fn network_rows(c: &Ctx) -> Vec<Row> {
    let w = c.status.as_ref().map(|s| &s.wifi);
    let non_empty = |s: &str| (!s.is_empty()).then(|| s.to_string());
    let mac_mode = match c.devd.net_mac_mode.as_str() {
        "random" => "Randomized",
        "custom" => "Custom",
        _ => "Hardware",
    };
    let ip_mode = if c.devd.net_ip_mode == "manual" {
        "Manual (Static)"
    } else {
        "Automatic (DHCP)"
    };
    let proxy = c
        .status
        .as_ref()
        .and_then(|s| s.vpn.proxy.clone())
        .unwrap_or_else(|| "Off".into());
    vec![
        Row::Toggle(
            "Wi-Fi",
            crate::network::wifi_enabled(c.status),
            Target::WifiSwitch,
        ),
        Row::Toggle(
            "Hotspot",
            crate::network::hotspot_on(c.status),
            Target::HotspotSwitch,
        ),
        Row::Action {
            label: "Wi-Fi networks & Hotspot",
            target: Target::WifiCard,
            danger: false,
        },
        Row::Action {
            label: "Join hidden network",
            target: Target::WifiManualSsid,
            danger: false,
        },
        Row::Action {
            label: "VPN, DNS and proxy",
            target: Target::Page(Page::Vpn),
            danger: false,
        },
        info("Wi-Fi state", opt(w.and_then(|w| w.state.clone()))),
        info("Network", opt(w.and_then(|w| w.ssid.clone()))),
        info(
            "Signal",
            opt(w.and_then(|w| w.signal_dbm).map(|d| format!("{d} dBm"))),
        ),
        Row::Choice(
            "MAC address mode".into(),
            mac_mode.into(),
            Target::NetMacMode,
        ),
        info(
            "MAC address",
            opt(non_empty(&c.devd.net_mac).or_else(|| c.sys.mac.clone())),
        ),
        Row::Choice("IPv4 mode".into(), ip_mode.into(), Target::NetIpMode),
        info(
            "IP address",
            opt(non_empty(&c.devd.net_ip).or_else(|| w.and_then(|w| w.ip.clone()))),
        ),
        Row::Choice(
            "Subnet mask".into(),
            c.devd.net_mask.clone(),
            Target::NetMaskCycle,
        ),
        info(
            "Gateway",
            opt(non_empty(&c.devd.net_gateway).or_else(|| c.sys.gateway.clone())),
        ),
        info(
            "DNS",
            if c.sys.dns.is_empty() {
                ("Unknown".into(), Tone::Warning)
            } else {
                (c.sys.dns.join(", "), Tone::Info)
            },
        ),
        Row::Choice("HTTP proxy".into(), proxy, Target::VpnProxy),
        info(
            "Hostname",
            opt(non_empty(&c.devd.net_hostname).or_else(|| c.sys.hostname.clone())),
        ),
    ]
}
