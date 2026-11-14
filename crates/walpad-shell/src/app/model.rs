//! Core UI model state, timing constants, and pure value helpers.

use crate::bluetooth;
use crate::config::DevdSettings;
use crate::core_client::ReplyError;
use crate::media;
use crate::mobile::MobileInfo;
use crate::network::NetModel;
use crate::overlay::Overlay;
use crate::phone_ui::PhoneModel;
use crate::settings_screen::{Page, SysInfo};
use crate::store;
use crate::volume_ui::Osd;
use walpad_proto::Status;
use walpad_ui::components::Tone;

use super::config::{WallTime, DEFAULT_PINNED};
use super::lock_flow::LockFlow;
use super::target::{Target, Tile};

pub type Ms = u64;

pub(super) const TOAST_MS: Ms = 3000;
pub(super) const POWER_ARM_MS: Ms = 3000;
/// The power menu closes itself this long after it opened.
pub(super) const POWER_MENU_MS: Ms = 20_000;
pub(super) const SPIN_EVERY_MS: Ms = 100;
pub(super) const SLIDER_SEND_EVERY_MS: Ms = 80;
pub(super) const ANIM_STEP_MS: Ms = 16;
pub(super) const TAB_ANIM_MS: Ms = 160;
pub(super) const OVERLAY_ANIM_MS: Ms = 150;

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub text: String,
    pub tone: Tone,
    pub until: Ms,
}

/// Everything the screens draw from.
pub struct Model {
    pub status: Option<Status>,
    pub connected: bool,
    /// Pinned apps (installed; at most four): the sidebar shows them.
    pub tiles: Vec<Tile>,
    /// Every installed app plus the Settings shortcut: the Home grid and the Apps panel.
    pub apps: Vec<Tile>,
    pub pinned: Vec<String>,
    pub wall: WallTime,
    pub pressed: Option<Target>,
    pub slider_active: bool,
    pub slider_value: f32,
    /// Which slider is being dragged: `Target::Slider` (brightness) or `Target::VolumeSlider`.
    pub slider_target: Target,
    /// The volume pill for a hardware key (None = hidden).
    pub osd: Option<Osd>,
    pub net: NetModel,
    pub overlay: Option<Overlay>,
    pub toast: Option<Toast>,
    pub spinner_deg: u16,
    pub devd: DevdSettings,
    pub clock_24h: bool,
    pub wallpaper: walpad_ui::art::WallpaperKind,
    pub wallpaper_path: String,
    pub live_phase_ms: u64,
    pub next_live_tick: Ms,
    /// Saved window layout (Settings > Input > Windows).
    pub windows: crate::windows_ui::WindowsPrefs,
    pub sys: SysInfo,
    /// Open Settings subpage (None = the category list).
    pub settings_page: Option<Page>,
    /// IMEI / ICCID from `mobile_info` (asked for when "Show" is tapped; never logged).
    pub mobile_info: Option<MobileInfo>,
    pub imei_shown: bool,
    /// Importable VPN profiles the core found (VPN screen).
    pub vpn_files: Vec<crate::vpn_screen::VpnFile>,
    /// Address of the Bluetooth connect / pair request in progress.
    pub bt_busy: Option<String>,
    /// The open "set / change / remove the screen lock" sequence (Settings > Security).
    pub lock_flow: Option<LockFlow>,
    /// App store screen state.
    pub store: store::StoreModel,
    /// Media screen state (change media-apps-capture).
    pub media: media::MediaModel,
    /// Phone screen: dial pad, recents, contacts, messages (owner request 2026-11-11).
    pub phone: PhoneModel,
    /// Notification centre list (owner request 2026-11-12, change `device-notifications`),
    /// fetched when the screen opens; the unread badge comes from `status.notifications` instead.
    pub notifications: Vec<walpad_proto::Notification>,
    /// Quick Settings "Screen off" requires two taps within 3 s so accidental taps don't sleep.
    pub screen_off_armed_until: Option<Ms>,
}

impl Model {
    pub fn blank(wall: WallTime) -> Model {
        Model {
            status: None,
            connected: false,
            tiles: Vec::new(),
            apps: Vec::new(),
            pinned: DEFAULT_PINNED.iter().map(|s| s.to_string()).collect(),
            wall,
            pressed: None,
            slider_active: false,
            slider_value: 0.0,
            slider_target: Target::Slider,
            osd: None,
            net: NetModel::default(),
            overlay: None,
            toast: None,
            spinner_deg: 0,
            devd: DevdSettings::default(),
            clock_24h: true,
            wallpaper: walpad_ui::art::WallpaperKind::Default,
            wallpaper_path: String::new(),
            live_phase_ms: 0,
            next_live_tick: 0,
            windows: Default::default(),
            sys: SysInfo::default(),
            settings_page: None,
            mobile_info: None,
            imei_shown: false,
            vpn_files: Vec::new(),
            bt_busy: None,
            lock_flow: None,
            store: store::StoreModel::default(),
            media: media::MediaModel::default(),
            phone: PhoneModel::default(),
            notifications: Vec::new(),
            screen_off_armed_until: None,
        }
    }

    /// Slider position 0..=1: the dragged value while dragging, else the core's brightness.
    pub fn brightness_value(&self) -> f32 {
        if self.slider_active && self.slider_target == Target::Slider {
            self.slider_value
        } else {
            self.status
                .as_ref()
                .map_or(0.0, |s| s.brightness as f32 / 255.0)
        }
    }

    pub(super) fn armed_until(&self) -> Option<Ms> {
        match &self.overlay {
            Some(Overlay::PowerConfirm { armed_until, .. }) => *armed_until,
            Some(Overlay::PowerMenu { armed, .. }) => armed.map(|(_, t)| t),
            _ => None,
        }
    }

    pub(super) fn spinning(&self) -> bool {
        self.net.scanning
            || self.media.scanning
            || self.net.connecting.is_some()
            || self.bt_busy.is_some()
            || bluetooth::scanning(&self.status)
            || matches!(&self.overlay, Some(Overlay::Password(s)) if s.connecting)
    }
}

pub(super) fn ease_out_offset(now: Ms, start: Ms, until: Ms, dist: i32) -> i32 {
    if until <= start || now >= until {
        return 0;
    }
    let dur = (until - start) as f32;
    let t = (now.saturating_sub(start) as f32 / dur).clamp(0.0, 1.0);
    let inv = 1.0 - t;
    (dist as f32 * inv * inv * inv).round() as i32
}

pub(super) fn percent(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 100.0).round() as u8
}

pub(super) fn native_brightness(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round().clamp(1.0, 255.0) as u8
}

pub(super) fn friendly(e: &ReplyError) -> String {
    match e.code.as_str() {
        "internal" if e.msg.contains("unavailable") => "Wi-Fi control is unavailable".into(),
        _ => e.msg.clone(),
    }
}
