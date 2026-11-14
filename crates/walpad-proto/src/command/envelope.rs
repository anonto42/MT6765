//! IPC request envelope, topic/mode enums, argument structs, and shell input events.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct RawRequest {
    pub v: u32,
    pub id: u64,
    pub cmd: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    Status,
    Wifi,
    Apps,
    Notice,
    Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenMode {
    On,
    Off,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotateMode {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "lock")]
    Lock,
    #[serde(rename = "toggle")]
    Toggle,
    #[serde(rename = "0")]
    R0,
    #[serde(rename = "90")]
    R90,
    #[serde(rename = "180")]
    R180,
    #[serde(rename = "270")]
    R270,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardMode {
    Show,
    Hide,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerAction {
    Poweroff,
    Reboot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrightnessArgs {
    #[serde(default)]
    pub value: Option<i32>,
    #[serde(default)]
    pub delta: Option<i32>,
}

/// Optional argument of `launch`: one file to open.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LaunchArgs {
    pub path: String,
}

/// Hardware-key events published on the `input` topic for the shell (never touch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShellInput {
    /// Long power press: open the power menu.
    PowerMenuOpen,
    VolumeUp,
    VolumeDown,
    /// Short power press while the power menu is open: activate the focused item.
    PowerShort,
    /// Show / hide the shell's system keyboard (the `keyboard` command, the `keyboard` key
    /// action, Home). The core resolves `toggle` against the state the shell last reported.
    KeyboardShow,
    KeyboardHide,
}
