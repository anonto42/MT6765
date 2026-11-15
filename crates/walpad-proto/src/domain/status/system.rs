use serde::{Deserialize, Serialize};

/// Output volume (owner request 2026-11-10). `available` = something can change the volume (the
/// sound server or the codec mixer); until the first probe answers it is false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioStatus {
    pub volume: u8,
    pub muted: bool,
    pub available: bool,
}

impl Default for AudioStatus {
    fn default() -> Self {
        AudioStatus {
            volume: 60,
            muted: false,
            available: false,
        }
    }
}

/// Headphone jack route / microphone source (owner request 2026-11-12). Honest limits: this kernel
/// build does not run the MT6357 accdet module (present at /vendor/lib/modules/mt6357-accdet.ko
/// but not insmod'd; see docs/45-headphone-mic.md), so there is no live jack-insert interrupt.
/// `detected` stays `None` until a future, explicitly-approved boot image loads it; `forced` is the
/// owner's manual Settings override, applied the same way either source would be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JackKind {
    /// Headphones with no mic (or a mic path the detector could not confirm).
    Headphones,
    /// A headset whose mic should be used instead of the built-in one.
    HeadsetWithMic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct JackStatus {
    /// Live accdet detection; always `None` on this kernel build (never guessed).
    pub detected: Option<JackKind>,
    /// The owner's manual override (Settings "force headphone output" / "use headset mic").
    pub forced: Option<JackKind>,
    /// `detected` when present, else `forced`, else `None` (speaker + built-in mic): what the
    /// core actually applied.
    pub route: Option<JackKind>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hardware {
    pub touch: String,
    pub gsensor: String,
    pub audio: String,
    pub bluetooth: String,
    pub sd_card: String,
}

/// What the screen lock is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockKind {
    Pin,
    Password,
    /// 3x3 dot pattern, stored as the visited dots 0-8 (row-major) written as digits.
    Pattern,
}

/// The screen lock (owner request 2026-11-10). Never carries a secret or the hash.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LockStatus {
    /// A PIN / password / pattern is set.
    pub configured: bool,
    pub kind: Option<LockKind>,
    /// The tablet is locked now (the shell shows the lock screen).
    pub locked: bool,
    /// Seconds until attempts are accepted again after too many wrong secrets.
    pub retry_in_s: Option<u32>,
    /// Lock this long after the screen went off (0 = as soon as it turns off).
    pub auto_lock_s: u32,
}

/// What generated a notification (owner request 2026-11-12, change `device-notifications`: "notify
/// every event like charging, any OTG or other things was connected"). `Storage` is the SD card /
/// USB mass-storage mount-remove signal `walpad-storage` already handles; `UsbDevice` is a generic
/// OTG peripheral attach-detach (new minimal sysfs detection: nothing tracked this before).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationCategory {
    Charging,
    Storage,
    UsbDevice,
    MobileData,
    Bluetooth,
    Gps,
    Hotspot,
}

/// One entry in the notification centre's bounded history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub id: u64,
    /// Unix milliseconds.
    pub ts_ms: u64,
    pub category: NotificationCategory,
    pub title: String,
    pub body: String,
    pub read: bool,
}

/// Badge on the bell icon (owner request 2026-11-12).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationsStatus {
    pub unread_count: u32,
}
