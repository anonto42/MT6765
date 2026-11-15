//! The core poll loop: IPC socket, devd.cmd FIFO, input devices and timers in one thread, no async
//! runtime. The `Brain` decides; this module executes (sysfs writes, X requests, spawns).
use crate::apps::Apps;
use crate::bluetooth::Bluetooth;
use crate::brain::Brain;
use crate::gnss::Gnss;
use crate::hotspot::Hotspot;
use crate::inputs::Inputs;
use crate::jack::Jack;
use crate::lock::Lock;
use crate::logic::soc;
use crate::modem::Modem;
use crate::notifications::Notifications;
use crate::pkg::Pkg;
use crate::stats::Stats;
use crate::volume::Volume;
use crate::vpn::Vpn;
use crate::wpa::manager::Wifi;
use crate::xbridge::XBridge;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use walpad_hw::{sysfs, Root};
use walpad_proto::Status;

mod actions;
mod client;
mod errors;
mod execute;
mod execute_net_phone;
mod loop_step;
mod status;
mod system;
mod tick;
mod windows;
mod windows_layout;
mod x11;
mod x11_apps;

use client::{open_fifo, Client};
pub use errors::parse_wifi_state;
use errors::{read_file, wall_ms};
use x11_apps::ExtClose;

pub(super) const STATUS_EVERY: Duration = Duration::from_secs(2);
pub(super) const BATTERY_EVERY: Duration = Duration::from_secs(10);
pub(super) const CLIENT_STALL: Duration = Duration::from_secs(5);
pub(super) const X_RETRY: Duration = Duration::from_secs(2);
pub(super) const X_PING_EVERY: Duration = Duration::from_secs(5);
pub(super) const APPS_X_EVERY: Duration = Duration::from_millis(700);
pub(super) const MAX_OUT: usize = 256 * 1024;
/// Safety net: the shell closes its power menu after 20 s and says so (`power_menu` closed); if it
/// never does (crash), the core stops routing keys to it after this long.
pub(super) const SHELL_MENU_MAX_MS: u64 = 25_000;

/// The owner's Wi-Fi switch (owner request 2026-11-10); read by walpad-wifi's loop.
pub const WIFI_ENABLED_FILE: &str = "/var/lib/walpad/wifi-enabled";

/// Which subsystems this core runs live (flags come from walpad-runtime via the init script).
pub(super) struct LiveState {
    pub(super) inputs: Inputs,
    pub(super) device: bool,
    pub(super) input: bool,
}

pub struct Daemon {
    pub(super) root: Root,
    pub(super) started: Instant,
    pub(super) brain: Brain,
    pub(super) listener: UnixListener,
    pub(super) fifo: Option<std::fs::File>,
    pub(super) fifo_buf: Vec<u8>,
    clients: Vec<Client>,
    pub(super) last_pushed: Option<Status>,
    pub(super) next_status: Instant,
    pub(super) next_battery: Instant,
    /// Smoothed battery percent / time estimator (docs/44-battery-soc.md), stepped every 10 s.
    pub(super) soc: soc::Estimator,
    pub(super) soc_out: soc::Output,
    pub(super) next_soc: Instant,
    pub(super) soc_saved: Option<(Instant, u8)>,
    pub(super) sock_path: PathBuf,
    pub(super) shadow: Option<crate::shadow_live::Live>,
    pub(super) live: Option<LiveState>,
    pub(super) apps: Apps,
    pub(super) profile: walpad_hw::domain::device_profile::DeviceProfile,
    pub(super) wifi: Wifi,

    pub(super) hotspot: Hotspot,
    pub(super) gnss: Gnss,
    pub(super) stats: Stats,
    /// Notification centre (owner request 2026-11-12, change `device-notifications`).
    pub(super) notifications: Notifications,
    pub(super) bluetooth: Bluetooth,
    pub(super) pkg: Pkg,
    pub(super) vpn: Vpn,
    pub(super) modem: Modem,
    pub(super) volume: Volume,
    pub(super) jack: Jack,
    pub(super) lock: Lock,
    pub(super) xb: Option<XBridge>,
    pub(super) x_retry: Instant,
    pub(super) x_ping: Instant,
    /// The shell's system keyboard is up (what the shell last reported or we last asked for).
    pub(super) keyboard_shown: bool,
    pub(super) apps_x_at: Instant,
    ext_close: Vec<ExtClose>,
    pub(super) close_grace: Duration,
    /// Helper processes started by `spawn_simple` (keyboard, mixer, ...), reaped in `app_tick`.
    pub(super) helpers: std::cell::RefCell<Vec<std::process::Child>>,
    /// Until when (ms) the shell's power menu owns the volume and power keys.
    pub(super) shell_menu_until: Option<u64>,
    /// Window layout mode and the engine behind it (change multitasking-windows).
    pub(super) layout: crate::layout::Layout,
    pub(super) layout_at: Instant,
    win_closes: Vec<windows_layout::WinClose>,
}

impl Daemon {
    pub fn new(root: Root) -> std::io::Result<Self> {
        std::fs::create_dir_all(root.p("/run/walpad"))?;
        let sock_path = root.p(walpad_proto::SOCKET_PATH);
        let _ = std::fs::remove_file(&sock_path);
        let listener = UnixListener::bind(&sock_path)?;
        listener.set_nonblocking(true)?;
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o600))?;
        }
        let fifo = open_fifo(&root)
            .map_err(|e| eprintln!("walpad-core: devd.cmd unavailable: {e}"))
            .ok();
        let now = Instant::now();
        let brain = Brain::new(
            &read_file(&root, "/etc/walpad/devd.json"),
            &read_file(&root, "/etc/walpad/keys.json"),
            &read_file(&root, "/etc/walpad/touch.json"),
            sysfs::read_backlight(&root),
            0,
        );
        let apps = Apps::load(&root);
        let profile = walpad_hw::domain::device_profile::DeviceProfile::detect(&root);
        let mut d = Daemon {
            brain,
            listener,
            fifo,
            fifo_buf: Vec::new(),
            clients: Vec::new(),
            last_pushed: None,
            next_status: now,
            next_battery: now,
            soc: soc::Estimator::new(
                std::fs::read_to_string(root.p(soc::PERSIST_PATH))
                    .ok()
                    .and_then(|t| soc::parse_persisted(&t, wall_ms() / 1000)),
            ),
            soc_out: soc::Output::default(),
            next_soc: now,
            soc_saved: None,
            started: now,
            sock_path,
            shadow: None,
            live: None,
            apps,
            profile,
            wifi: Wifi::new(&root),

            hotspot: Hotspot::new(&root),
            gnss: Gnss::new(&root),
            stats: Stats::new(&root),
            notifications: Notifications::new(&root),
            bluetooth: Bluetooth::new(&root),
            pkg: Pkg::new(&root),
            vpn: Vpn::new(&root),
            modem: Modem::new(&root),
            volume: Volume::new(&root, now),
            jack: Jack::new(&root, now),
            lock: Lock::new(&root, wall_ms()),
            xb: None,
            x_retry: now,
            x_ping: now,
            keyboard_shown: false,
            apps_x_at: now,
            ext_close: Vec::new(),
            close_grace: Duration::from_secs(3),
            helpers: Default::default(),
            shell_menu_until: None,
            layout: windows_layout::load_layout(&root),
            layout_at: now,
            win_closes: Vec::new(),
            root,
        };
        // orientation is 0 at startup and the file is always written (struts need X: later)
        let init = d.brain.rotation.init();
        let mut fx = Vec::new();
        Brain::rotation_effects(init, &mut fx);
        d.apply(fx, true);
        d.soc_tick(now);
        // mimeapps.list follows what is installed (app store, change app-catalogue-manager)
        let _ = d.pkg.apply_defaults();
        // a hotspot can outlive a core restart: pick up its real state
        d.hotspot.probe(now);
        // GPS (owner request 2026-11-11): a switched-on GPS comes up again after a boot
        d.gnss.probe(now);
        // VPN: pick up a running tunnel, apply the stored DNS and proxy (owner request 2026-11-10)
        d.vpn.startup(now);
        let px = d.vpn.proxy().map(str::to_string);
        d.apps.set_proxy(px.as_deref());
        // the modem: pick up a running one, or boot it once per boot (owner request 2026-11-10)
        d.modem.startup(now);
        Ok(d)
    }

    pub fn socket_path(&self) -> &std::path::Path {
        &self.sock_path
    }
    pub fn profile(&self) -> &walpad_hw::domain::device_profile::DeviceProfile {
        &self.profile
    }

    pub(super) fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
    pub(super) fn real_root(&self) -> bool {
        self.root.base() == std::path::Path::new("/")
    }

    /// Shadow mode: read the real inputs, log `WOULD ...`, write nothing by policy (M2.2).
    pub fn enable_shadow(&mut self) {
        self.shadow = Some(crate::shadow_live::Live::new(&self.root));
    }

    /// Live mode: run the device and/or input subsystem from the real inputs (M3.2/M3.3).
    pub fn enable_live(&mut self, device: bool, input: bool) {
        if !(device || input) {
            return;
        }
        self.live = Some(LiveState {
            inputs: Inputs::new(&self.root, "walpad-core"),
            device,
            input,
        });
        let init = self.brain.rotation.init();
        let mut fx = Vec::new();
        Brain::rotation_effects(init, &mut fx);
        self.apply(fx, true);
    }

    /// Test hook: cheaper PBKDF2 for the screen lock's new secrets.
    pub fn lock_mut(&mut self) -> &mut Lock {
        &mut self.lock
    }

    /// Test hook: shorter VPN script timeouts.
    pub fn vpn_mut(&mut self) -> &mut Vpn {
        &mut self.vpn
    }

    /// Test hook: a shorter stats sampling interval.
    pub fn stats_mut(&mut self) -> &mut Stats {
        &mut self.stats
    }

    /// Test hook: shorter GPS script timeouts.
    pub fn gnss_mut(&mut self) -> &mut Gnss {
        &mut self.gnss
    }

    /// Test hook: shorter hotspot script timeouts.
    pub fn hotspot_mut(&mut self) -> &mut Hotspot {
        &mut self.hotspot
    }

    /// Test hook: shorter Bluetooth limits.
    pub fn bluetooth_mut(&mut self) -> &mut Bluetooth {
        &mut self.bluetooth
    }

    /// Test hook: walpad-volume timeout.
    pub fn volume_mut(&mut self) -> &mut Volume {
        &mut self.volume
    }

    /// Test hook: app store reserve and job timeouts.
    pub fn pkg_mut(&mut self) -> &mut Pkg {
        &mut self.pkg
    }

    /// Test hook: modem timeouts and poll periods.
    pub fn modem_mut(&mut self) -> &mut Modem {
        &mut self.modem
    }

    /// Test hook: how long a closing app gets before it is killed.
    pub fn set_close_grace(&mut self, d: Duration) {
        self.close_grace = d;
        self.apps.set_close_grace(d);
    }
}
