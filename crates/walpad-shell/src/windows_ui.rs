//! Windows mode in Settings (change multitasking-windows): the saved layout (single, split, free)
//! and its three buttons on the Input page. Pure: the core saves the mode in
//! /etc/walpad/shell.json (`windows_mode`, `split_ratio`, `split_swap`); the shell only reads it
//! and sends `layout`.
use crate::app::{Cmd, Target};
use crate::settings_screen::Row;
use walpad_hw::Root;
use walpad_proto::wm::{self, LayoutMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsPrefs {
    pub mode: LayoutMode,
    pub ratio: u8,
}

impl Default for WindowsPrefs {
    fn default() -> Self {
        WindowsPrefs {
            mode: LayoutMode::Single,
            ratio: 50,
        }
    }
}

/// The saved layout (single / 50 when the file or the keys are missing or wrong).
pub fn read_prefs(root: &Root) -> WindowsPrefs {
    let v: serde_json::Value = std::fs::read_to_string(root.p("/etc/walpad/shell.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    let ratio = v["split_ratio"].as_u64().unwrap_or(50).min(255) as u8;
    WindowsPrefs {
        mode: v["windows_mode"]
            .as_str()
            .and_then(LayoutMode::parse)
            .unwrap_or_default(),
        ratio: if wm::valid_ratio(ratio) { ratio } else { 50 },
    }
}

pub fn mode_name(m: LayoutMode) -> &'static str {
    match m {
        LayoutMode::Single => "Single",
        LayoutMode::Split => "Split",
        LayoutMode::Free => "Free",
    }
}

pub fn ratio_name(r: u8) -> &'static str {
    match r {
        33 => "1/3 + 2/3",
        67 => "2/3 + 1/3",
        _ => "1/2 + 1/2",
    }
}

/// The rows appended to Settings > Input: the mode button always, the split buttons in split mode.
pub fn rows(p: &WindowsPrefs) -> Vec<Row> {
    let mut v = vec![Row::ActionText {
        label: format!("Windows: {} (tap to change)", mode_name(p.mode)),
        target: Target::WindowsMode,
        danger: false,
    }];
    if p.mode == LayoutMode::Split {
        v.push(Row::ActionText {
            label: format!("Split sizes: {}", ratio_name(p.ratio)),
            target: Target::WindowsRatio,
            danger: false,
        });
        v.push(Row::ActionText {
            label: "Swap the two sides".into(),
            target: Target::WindowsSwap,
            danger: false,
        });
    }
    v
}

/// What a tap on one of the rows does: the new local state and the core command.
pub fn tap(p: &WindowsPrefs, t: Target) -> Option<(WindowsPrefs, Cmd)> {
    match t {
        Target::WindowsMode => {
            let mode = p.mode.next();
            Some((
                WindowsPrefs { mode, ..*p },
                Cmd::Layout {
                    mode: Some(mode.as_str()),
                    ratio: None,
                    swap: false,
                },
            ))
        }
        Target::WindowsRatio => {
            let ratio = wm::next_ratio(p.ratio);
            Some((
                WindowsPrefs { ratio, ..*p },
                Cmd::Layout {
                    mode: None,
                    ratio: Some(ratio),
                    swap: false,
                },
            ))
        }
        Target::WindowsSwap => Some((
            *p,
            Cmd::Layout {
                mode: None,
                ratio: None,
                swap: true,
            },
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
