//! Live system figures for the shell's header (owner request 2026-11-11, change `top-bar-stats`):
//! CPU use from /proc/stat, RAM from /proc/meminfo, storage of `/` (statvfs) and network
//! throughput from /proc/net/dev. They are sampled every 2 s, and only while at least one item is
//! switched on; a figure that cannot be read is `None` (shown as unavailable), never guessed. The
//! switches persist in /var/lib/walpad/statusbar.json. Nothing here blocks: /proc reads only.
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use walpad_hw::{sysinfo, Root};
use walpad_proto::{StatsItems, StatsStatus};

pub const CONFIG_FILE: &str = "/var/lib/walpad/statusbar.json";
const EVERY: Duration = Duration::from_secs(2);
/// A previous reading older than this is not used for a rate (the loop stalled).
const MAX_GAP: Duration = Duration::from_secs(30);
const MAX_FILE: u64 = 1 << 20;

/// `(interface, rx_bytes, tx_bytes)` per counted interface.
pub type NetCounters = Vec<(String, u64, u64)>;

/// Cumulative CPU time: `(busy, total)` jiffies over all CPUs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

/// The aggregate `cpu ` line of /proc/stat. Idle and iowait count as idle; guest time is already
/// inside user/nice and is not added twice.
pub fn parse_cpu(text: &str) -> Option<CpuTimes> {
    let line = text.lines().find(|l| l.starts_with("cpu "))?;
    let v: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(|f| f.parse().ok())
        .collect::<Option<_>>()?;
    // user nice system idle iowait irq softirq steal: at least the first four exist
    if v.len() < 4 {
        return None;
    }
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    let total: u64 = v.iter().sum();
    Some(CpuTimes {
        busy: total.saturating_sub(idle),
        total,
    })
}

/// Busy share between two readings, 0-100. `None` when no time passed or the counters went back.
pub fn cpu_pct(prev: CpuTimes, cur: CpuTimes) -> Option<u8> {
    let dt = cur.total.checked_sub(prev.total).filter(|d| *d > 0)?;
    let db = cur.busy.checked_sub(prev.busy)?;
    Some(((db * 100 + dt / 2) / dt).min(100) as u8)
}

/// `(total_kb, used_kb)`: used is total minus MemAvailable (what the kernel could hand out).
pub fn parse_meminfo(text: &str) -> Option<(u64, u64)> {
    let get = |k: &str| {
        text.lines().find_map(|l| {
            l.strip_prefix(k)?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })
    };
    let total = get("MemTotal:").filter(|t| *t > 0)?;
    let avail = get("MemAvailable:")?;
    Some((total, total.saturating_sub(avail)))
}

/// Interfaces that count as "the network": Wi-Fi, mobile data, USB tethering. Never `lo`.
pub fn counted_iface(name: &str) -> bool {
    matches!(name, "wlan0" | "rndis0" | "usb0")
        || name
            .strip_prefix("ccmni")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// `(interface, rx_bytes, tx_bytes)` of every counted interface in /proc/net/dev.
pub fn parse_net_dev(text: &str) -> Vec<(String, u64, u64)> {
    text.lines()
        .filter_map(|l| {
            let (name, rest) = l.split_once(':')?;
            let name = name.trim();
            if !counted_iface(name) {
                return None;
            }
            let f: Vec<&str> = rest.split_whitespace().collect();
            // rx bytes is field 0, tx bytes field 8
            Some((
                name.to_string(),
                f.first()?.parse().ok()?,
                f.get(8)?.parse().ok()?,
            ))
        })
        .collect()
}

/// `(down, up)` bytes per second between two readings. Only interfaces present in both count (one
/// that came or went would otherwise show as a burst); a counter that went back (reset) is skipped.
pub fn net_rate(
    prev: &[(String, u64, u64)],
    cur: &[(String, u64, u64)],
    dt: Duration,
) -> Option<(u64, u64)> {
    let ms = dt.as_millis() as u64;
    if ms == 0 {
        return None;
    }
    let (mut rx, mut tx, mut seen) = (0u64, 0u64, false);
    for (n, r, t) in cur {
        if let Some((_, pr, pt)) = prev.iter().find(|(pn, _, _)| pn == n) {
            if let (Some(dr), Some(dtx)) = (r.checked_sub(*pr), t.checked_sub(*pt)) {
                rx += dr;
                tx += dtx;
                seen = true;
            }
        }
    }
    seen.then(|| (rx * 1000 / ms, tx * 1000 / ms))
}

fn read(path: PathBuf) -> Option<String> {
    use std::io::Read;
    let mut s = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_FILE)
        .read_to_string(&mut s)
        .ok()?;
    Some(s)
}

pub struct Stats {
    root: Root,
    file: PathBuf,
    st: StatsStatus,
    every: Duration,
    next: Instant,
    prev_cpu: Option<CpuTimes>,
    prev_net: Option<(Instant, NetCounters)>,
}

impl Stats {
    pub fn new(root: &Root) -> Stats {
        let file = root.p(CONFIG_FILE);
        let items = read(file.clone())
            .and_then(|t| serde_json::from_str::<StatsItems>(&t).ok())
            .unwrap_or_default();
        Stats {
            root: root.clone(),
            file,
            st: StatsStatus {
                items,
                ..Default::default()
            },
            every: EVERY,
            next: Instant::now(),
            prev_cpu: None,
            prev_net: None,
        }
    }

    pub fn status(&self) -> &StatsStatus {
        &self.st
    }

    /// Test hook: a shorter sampling interval.
    pub fn set_every(&mut self, d: Duration) {
        self.every = d;
    }

    /// Change some switches and persist them. A switched-off item loses its figure at once and
    /// its previous reading, so a later switch-on starts a fresh interval.
    pub fn configure(
        &mut self,
        cpu: Option<bool>,
        ram: Option<bool>,
        rom: Option<bool>,
        net: Option<bool>,
        now: Instant,
    ) -> Result<StatsItems, String> {
        let mut it = self.st.items;
        it.cpu = cpu.unwrap_or(it.cpu);
        it.ram = ram.unwrap_or(it.ram);
        it.rom = rom.unwrap_or(it.rom);
        it.net = net.unwrap_or(it.net);
        if it != self.st.items {
            self.save(&it)
                .map_err(|e| format!("could not save the status bar items: {e}"))?;
            self.st.items = it;
            self.clear_off_items();
            // sample soon: instant figures appear at once, rates after the next interval
            self.next = now;
        }
        Ok(it)
    }

    fn save(&self, it: &StatsItems) -> std::io::Result<()> {
        if let Some(d) = self.file.parent() {
            std::fs::create_dir_all(d)?;
        }
        let mut tmp = self.file.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(serde_json::to_string(it).unwrap_or_default().as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, &self.file)
    }

    fn clear_off_items(&mut self) {
        let it = self.st.items;
        if !it.cpu {
            self.st.cpu_pct = None;
            self.prev_cpu = None;
        }
        if !it.ram {
            self.st.ram_used_kb = None;
            self.st.ram_total_kb = None;
        }
        if !it.rom {
            self.st.rom_used_b = None;
            self.st.rom_total_b = None;
        }
        if !it.net {
            self.st.net_down_bps = None;
            self.st.net_up_bps = None;
            self.prev_net = None;
        }
    }

    /// Sample when due and an item is on. True when the figures changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        let it = self.st.items;
        if !it.any() || now < self.next {
            return false;
        }
        self.next = now + self.every;
        let before = self.st.clone();
        if it.cpu {
            let cur = read(self.root.p("/proc/stat")).and_then(|t| parse_cpu(&t));
            self.st.cpu_pct = match (self.prev_cpu, cur) {
                (Some(p), Some(c)) => cpu_pct(p, c),
                _ => None,
            };
            self.prev_cpu = cur;
        }
        if it.ram {
            let m = read(self.root.p("/proc/meminfo")).and_then(|t| parse_meminfo(&t));
            self.st.ram_total_kb = m.map(|(t, _)| t);
            self.st.ram_used_kb = m.map(|(_, u)| u);
        }
        if it.rom {
            let d = sysinfo::disk_usage(&self.root).filter(|(t, _)| *t > 0);
            self.st.rom_total_b = d.map(|(t, _)| t);
            self.st.rom_used_b = d.map(|(t, f)| t.saturating_sub(f));
        }
        if it.net {
            let cur = read(self.root.p("/proc/net/dev")).map(|t| parse_net_dev(&t));
            let rate = match (&self.prev_net, &cur) {
                (Some((t0, p)), Some(c)) if now.duration_since(*t0) <= MAX_GAP => {
                    net_rate(p, c, now.duration_since(*t0))
                }
                _ => None,
            };
            self.st.net_down_bps = rate.map(|r| r.0);
            self.st.net_up_bps = rate.map(|r| r.1);
            self.prev_net = cur.map(|c| (now, c));
        }
        self.st != before
    }
}

#[cfg(test)]
mod tests;
