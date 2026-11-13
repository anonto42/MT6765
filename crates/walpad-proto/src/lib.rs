//! walpad-proto: IPC protocol v1 types shared by core, shell and ctl.
//! Newline-delimited JSON over /run/walpad/control.sock (see docs/rust/PLAN section 6).
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod appdefs;
mod codec;
mod command;
mod devd;
mod secret;
mod status;
pub mod wm;

pub use codec::{encode_line, LineDecoder, LineError, MAX_LINE};
pub use command::{
    lock_secret_rule, same_number, valid_apn, valid_app_id, valid_attempt, valid_bt_address,
    valid_contact_name, valid_datetime, valid_dial_number, valid_dns_server, valid_dtmf,
    valid_hostname, valid_hotspot_secret, valid_hotspot_ssid, valid_lock_secret, valid_open_path,
    valid_proxy, valid_sim_pin, valid_sms_number, valid_sms_text, valid_store_id, valid_vcard_path,
    valid_vpn_import_path, valid_vpn_name, BrightnessArgs, Command, KeyboardMode, LaunchArgs,
    PowerAction, RawRequest, RotateMode, ScreenMode, ShellInput, Topic, ALLOWED_SETTINGS,
    COMMAND_NAMES, DEFAULT_ROLES, KEY_ACTIONS, OPEN_ROOTS,
};
pub use devd::parse_devd_line;
pub use secret::{redact_hex_ssid, Secret};
pub use status::{
    AppState, AudioStatus, Battery, BatterySource, BluetoothController, BluetoothDevice,
    BluetoothStatus, CallState, GpsState, GpsStatus, Hardware, HotspotInternet, HotspotState,
    HotspotStatus, JackKind, JackStatus, LegacyStatus, LockKind, LockStatus, MobileData, MobileReg,
    MobileStatus, ModemState, Notification, NotificationCategory, NotificationsStatus, PhoneStatus,
    ScreenState, SimState, StatsItems, StatsStatus, Status, VpnKind, VpnProfile, VpnState,
    VpnStatus, WifiRadio, WifiStatus,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTO_VERSION: u32 = 1;
pub const SOCKET_PATH: &str = "/run/walpad/control.sock";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidArgs,
    UnknownCommand,
    Version,
    Timeout,
    Busy,
    NotHello,
    Refused,
    TooLarge,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub msg: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub v: u32,
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<ErrorBody>,
}

impl Response {
    pub fn ok(id: u64, data: Value) -> Self {
        Response {
            v: PROTO_VERSION,
            id,
            ok: true,
            data: Some(data),
            error: None,
        }
    }
    pub fn err(id: u64, code: ErrorCode, msg: impl Into<String>) -> Self {
        Response {
            v: PROTO_VERSION,
            id,
            ok: false,
            data: None,
            error: Some(ErrorBody {
                code,
                msg: msg.into(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub v: u32,
    pub event: String,
    pub data: Value,
}

impl Event {
    pub fn new(event: &str, data: Value) -> Self {
        Event {
            v: PROTO_VERSION,
            event: event.to_string(),
            data,
        }
    }
}

/// Parse one request line into (id, Command), or the error response to send back.
pub fn parse_request(line: &[u8]) -> Result<(u64, Command), Response> {
    let raw: RawRequest = serde_json::from_slice(line)
        .map_err(|e| Response::err(0, ErrorCode::InvalidArgs, format!("bad request: {e}")))?;
    if raw.v != PROTO_VERSION {
        return Err(Response::err(
            raw.id,
            ErrorCode::Version,
            "unsupported protocol version",
        ));
    }
    match Command::from_raw(&raw) {
        Ok(c) => Ok((raw.id, c)),
        Err((code, msg)) => Err(Response::err(raw.id, code, msg)),
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod wm_tests;
