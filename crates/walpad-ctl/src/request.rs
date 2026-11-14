use serde_json::{json, Value};

pub const USAGE: &str = "usage: walpad-ctl <command>
  status                          show device status (JSON)
  screen on|off|toggle
  brightness N|+N|-N              native units 0-255
  volume N|+N|-N                  output volume in percent (0-100)
  mute on|off
  jack off|headphones|headset     force the headphone/mic route (no live jack detection on this
                                  kernel build yet; see docs/45-headphone-mic.md)
  rotate auto|lock|toggle|0|90|180|270
  keyboard [show|hide|toggle]
  home | back | screenshot | restart-shell
  launch <app-id>
  new-instance <app-id>           one more window of an app that allows several (terminal)
  windows                         list the app windows (id, app, title, geometry)
  layout [single|split|free|33|50|67|swap]   show or set the window layout
  wifi-scan                       scan and print the networks (waits for the result)
  wifi-connect <ssid> [--open]    join a network; the passphrase is read from stdin (never argv)
  wifi-saved | wifi-disconnect | wifi-forget <id>
  wifi on|off                     the Wi-Fi switch (network level; the chip stays powered)
  hotspot on|off                  start/stop the hotspot (needs a password: hotspot-config)
  gps on|off                      the GPS switch (starts mnld + walpad-gnss; needs the Wi-Fi stack)
  stats cpu|ram|rom|net on|off    show or hide a live figure in the header bar
  hotspot-config [--name N] [--password]   set name and/or password (password read from stdin)
  lock-now                        lock the screen at once (needs a lock to be set)
  lock-set pin|password|pattern  set or change the screen lock; the new secret is read from stdin
                                  (line 1; pattern = dots 0-8 as digits), the current one, when a
                                  lock is set, from line 2 (never argv)
  lock-remove                     remove the lock; the current secret is read from stdin
  lock_clear                      RESCUE: remove the screen lock (needs no PIN; run it over SSH or
                                  the USB recovery shell; also works while the core is down)
  vpn on|off|confirm|files        VPN tunnel; confirm keeps the kill switch (else it reverts in 60 s)
  vpn import <path> [name] | remove <name> | profile <name> | kill-switch on|off
  vpn dns <ip>... | dns clear | proxy <http://host:port> | proxy clear
  power poweroff|reboot --yes     refuses without --yes
  key-held <name|code>            volumedown|volumeup|power or 114|115|116; exit 0 held, 1 not held,
                                  2 error/unknown device (never blocks, needs no core)";

/// Translate CLI words into (cmd, args). Legacy syntax is kept as-is.
pub fn build_request(args: &[String]) -> Result<(String, Value), String> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let need = |n: usize| {
        if a.len() > n {
            Ok(a[n])
        } else {
            Err(USAGE.to_string())
        }
    };
    Ok(match a.first().copied() {
        Some("status") => ("get_status".into(), Value::Null),
        Some("screen") => ("screen".into(), json!({"mode": need(1)?})),
        Some("rotate") => ("rotate".into(), json!({"mode": need(1)?})),
        Some("brightness") => {
            let s = need(1)?;
            let n: i32 = s.parse().map_err(|_| format!("bad number {s:?}"))?;
            if s.starts_with('+') || s.starts_with('-') {
                ("set_brightness".into(), json!({"delta": n}))
            } else {
                ("set_brightness".into(), json!({"value": n}))
            }
        }
        Some("volume") => {
            let s = need(1)?;
            let n: i32 = s.parse().map_err(|_| format!("bad number {s:?}"))?;
            if s.starts_with('+') || s.starts_with('-') {
                ("volume_delta".into(), json!({"delta": n}))
            } else {
                ("volume_set".into(), json!({"percent": n}))
            }
        }
        Some("mute") => {
            let on = match need(1)? {
                "on" => true,
                "off" => false,
                _ => return Err(USAGE.to_string()),
            };
            ("mute".into(), json!({"on": on}))
        }
        Some("jack") => {
            let route = match need(1)? {
                "off" => Value::Null,
                "headphones" => json!("headphones"),
                "headset" => json!("headset_with_mic"),
                _ => return Err(USAGE.to_string()),
            };
            ("jack_force".into(), json!({"route": route}))
        }
        Some("keyboard") => (
            "keyboard".into(),
            json!({"mode": a.get(1).copied().unwrap_or("toggle")}),
        ),
        Some("home") => ("home".into(), Value::Null),
        Some("back") => ("back".into(), Value::Null),
        Some("screenshot") => ("screenshot".into(), Value::Null),
        Some("lock-now" | "lock_now") => ("lock_now".into(), Value::Null),
        Some("restart-shell") => ("restart_shell".into(), Value::Null),
        Some("launch") => ("launch".into(), json!({"app": need(1)?})),
        Some("new-instance") => ("new_instance".into(), json!({"app": need(1)?})),
        Some("windows") => ("windows".into(), Value::Null),
        Some("layout") => match a.get(1).copied() {
            None => ("layout".into(), Value::Null),
            Some("swap") => ("layout".into(), json!({"swap": true})),
            Some(m @ ("single" | "split" | "free")) => ("layout".into(), json!({"mode": m})),
            Some(r) => {
                let n: u8 = r.parse().map_err(|_| USAGE.to_string())?;
                ("layout".into(), json!({"ratio": n}))
            }
        },
        Some("wifi-saved") => ("wifi_list_saved".into(), Value::Null),
        Some("wifi-disconnect") => ("wifi_disconnect".into(), Value::Null),
        Some("wifi-forget") => {
            let id: u32 = need(1)?
                .parse()
                .map_err(|_| "saved id must be a number".to_string())?;
            ("wifi_forget".into(), json!({"saved_id": id}))
        }
        Some(w @ ("wifi" | "hotspot" | "gps")) => {
            let on = match need(1)? {
                "on" => true,
                "off" => false,
                _ => return Err(USAGE.to_string()),
            };
            let cmd = match w {
                "wifi" => "wifi_enable",
                "gps" => "gps",
                _ => "hotspot",
            };
            (cmd.into(), json!({"on": on}))
        }
        Some("stats") => {
            let key = match need(1)? {
                k @ ("cpu" | "ram" | "rom" | "net") => k,
                _ => return Err(USAGE.to_string()),
            };
            let on = match need(2)? {
                "on" => true,
                "off" => false,
                _ => return Err(USAGE.to_string()),
            };
            ("stats_config".into(), json!({ key: on }))
        }
        Some("vpn") => match need(1)? {
            w @ ("on" | "off") => ("vpn".into(), json!({"on": w == "on"})),
            "confirm" => ("vpn_confirm".into(), Value::Null),
            "files" => ("vpn_files".into(), Value::Null),
            "import" => {
                let mut r = json!({"path": need(2)?});
                if let Some(n) = a.get(3) {
                    r["name"] = json!(n);
                }
                ("vpn_import".into(), r)
            }
            "remove" => ("vpn_remove".into(), json!({"name": need(2)?})),
            "profile" => ("vpn_config".into(), json!({"profile": need(2)?})),
            "kill-switch" => match need(2)? {
                "on" => ("vpn_config".into(), json!({"kill_switch": true})),
                "off" => ("vpn_config".into(), json!({"kill_switch": false})),
                _ => return Err(USAGE.to_string()),
            },
            "dns" => {
                let d: Vec<&str> = if need(2)? == "clear" {
                    Vec::new()
                } else {
                    a[2..].to_vec()
                };
                ("vpn_config".into(), json!({"dns": d}))
            }
            "proxy" => {
                let p = if need(2)? == "clear" { "" } else { a[2] };
                ("vpn_config".into(), json!({"proxy": p}))
            }
            _ => return Err(USAGE.to_string()),
        },
        Some("power") => (
            "power".into(),
            json!({"action": need(1)?, "confirmed": a.contains(&"--yes")}),
        ),
        _ => return Err(USAGE.to_string()),
    })
}
