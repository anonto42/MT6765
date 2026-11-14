//! System information gathering and formatting helpers for Settings pages.

use crate::config::read_gestures;
use walpad_hw::{sysfs, sysinfo, Root};
use walpad_ui::components::Tone;

/// Facts gathered from the system (refreshed when a page opens and every minute).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SysInfo {
    pub alpine: Option<String>,
    pub kernel: Option<String>,
    pub storage: Option<(u64, u64)>,
    pub memory_kb: Option<(u64, u64)>,
    pub uptime_s: Option<u64>,
    pub ssh_port_open: bool,
    pub ssh_keys: usize,
    pub listening: Vec<(String, u16)>,
    pub dns: Vec<String>,
    pub gateway: Option<String>,
    pub mac: Option<String>,
    pub hostname: Option<String>,
    pub logs: Vec<String>,
    pub gestures: Vec<(String, String)>,
}

pub fn gather(root: &Root) -> SysInfo {
    let listening = sysinfo::listening_tcp(root);
    let mac = sysfs::read_trim(root, "/sys/class/net/wlan0/address")
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_uppercase());
    let hostname = sysfs::read_trim(root, "/proc/sys/kernel/hostname")
        .or_else(|| sysfs::read_trim(root, "/etc/hostname"))
        .filter(|s| !s.is_empty());
    SysInfo {
        alpine: sysinfo::alpine_release(root),
        kernel: sysinfo::kernel_release(root),
        storage: sysinfo::disk_usage(root),
        memory_kb: sysinfo::meminfo(root),
        uptime_s: sysfs::uptime_s(root).map(|u| u as u64),
        ssh_port_open: listening.iter().any(|(_, p)| *p == 22),
        ssh_keys: sysinfo::authorized_key_count(root),
        listening,
        dns: sysinfo::dns_servers(root),
        gateway: sysinfo::default_gateway(root),
        mac,
        hostname,
        logs: sysinfo::tail_lines(root, "/var/log/walpad-core.log", 200),
        gestures: read_gestures(root),
    }
}

pub fn fmt_bytes(b: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let f = b as f64;
    if f >= GB {
        format!("{:.1} GB", f / GB)
    } else {
        format!("{:.0} MB", f / (1024.0 * 1024.0))
    }
}

pub fn fmt_uptime(s: u64) -> String {
    let (d, h, m) = (s / 86_400, s % 86_400 / 3600, s % 3600 / 60);
    match (d, h) {
        (0, 0) => format!("{m} min"),
        (0, _) => format!("{h} h {m} min"),
        _ => format!("{d} d {h} h"),
    }
}

/// Telnet is only acceptable on the USB addresses.
pub fn telnet_scope(listening: &[(String, u16)]) -> (&'static str, Tone) {
    let t: Vec<&String> = listening
        .iter()
        .filter(|(_, p)| *p == 23)
        .map(|(a, _)| a)
        .collect();
    if t.is_empty() {
        ("Not running", Tone::Info)
    } else if t
        .iter()
        .all(|a| a.as_str() == "192.168.42.1" || a.as_str() == "192.168.43.1")
    {
        ("USB only", Tone::Info)
    } else {
        ("Open on the network", Tone::Danger)
    }
}
