//! `--fake`: populate a WALPAD_ROOT tree with plausible values so the daemon runs on a host.
use std::path::Path;

pub fn populate(root: &Path) -> std::io::Result<()> {
    let w = |rel: &str, content: &str| -> std::io::Result<()> {
        let p = root.join(rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        if !p.exists() {
            std::fs::write(p, content)?;
        }
        Ok(())
    };
    w("sys/class/leds/lcd-backlight/brightness", "120\n")?;
    let ps = "sys/class/power_supply/battery";
    w(&format!("{ps}/capacity"), "-1\n")?;
    w(&format!("{ps}/status"), "Discharging\n")?;
    w(&format!("{ps}/voltage_now"), "3920000\n")?;
    w(&format!("{ps}/temp"), "340\n")?;
    w("proc/uptime", "1234.56 4000.00\n")?;
    w("dev/gsensor", "00000000 00000fa0 00000000")?;
    w("run/walpad/wifi-state", "COMPLETED 192.168.0.100\n")?;
    w("sys/class/input/input4/name", "mtk-tpd\n")?;
    w("sys/class/input/input4/event4/dev", "13:68\n")?;
    w("dev/input/event4", "")?;
    std::fs::create_dir_all(root.join("etc/walpad"))?;
    Ok(())
}
