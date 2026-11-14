//! Power key (parity: walpad-devd.py power button handling).
use super::Ms;

const LONG_MS: Ms = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    ToggleScreen,
    /// A short press while the power menu is open activates its focused item.
    ActivateMenuItem,
    WakeScreen,
    OpenPowerMenu,
}

#[derive(Debug, Default)]
pub struct PowerKey {
    down_at: Option<Ms>,
    long: bool,
}

impl PowerKey {
    /// Feed a KEY_POWER event value (1 press, 0 release; repeats are ignored).
    pub fn on_key(&mut self, value: i32, now: Ms, menu_open: bool) -> Vec<Act> {
        match value {
            1 => {
                self.down_at = Some(now);
                self.long = false;
                Vec::new()
            }
            0 if self.down_at.is_some() => {
                let act = if self.long {
                    None
                } else if menu_open {
                    Some(Act::ActivateMenuItem)
                } else {
                    Some(Act::ToggleScreen)
                };
                self.down_at = None;
                act.into_iter().collect()
            }
            _ => Vec::new(),
        }
    }

    /// Rule 1: held >= 1.0 s -> wake the screen and open the menu unless it is open.
    pub fn tick(&mut self, now: Ms, menu_open: bool, screen_on: bool) -> Vec<Act> {
        let mut out = Vec::new();
        if let Some(t) = self.down_at {
            if !self.long && now.saturating_sub(t) >= LONG_MS {
                self.long = true;
                if !screen_on {
                    out.push(Act::WakeScreen);
                }
                if !menu_open {
                    out.push(Act::OpenPowerMenu);
                }
            }
        }
        out
    }

    /// Rule 3: poll every 100 ms while the key is down, otherwise 400 ms.
    pub fn poll_interval_ms(&self) -> Ms {
        if self.down_at.is_some() {
            100
        } else {
            400
        }
    }
}

#[cfg(test)]
mod tests;
