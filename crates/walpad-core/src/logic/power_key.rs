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
mod tests {
    use super::*;

    #[test]
    fn rule2_short_press_toggles_or_activates() {
        let mut k = PowerKey::default();
        k.on_key(1, 0, false);
        assert_eq!(k.on_key(0, 999, false), vec![Act::ToggleScreen]);
        k.on_key(1, 2000, true);
        assert_eq!(k.on_key(0, 2100, true), vec![Act::ActivateMenuItem]);
    }

    #[test]
    fn rule1_long_press_opens_menu_once() {
        let mut k = PowerKey::default();
        k.on_key(1, 0, false);
        assert!(k.tick(999, false, true).is_empty());
        assert_eq!(k.tick(1000, false, true), vec![Act::OpenPowerMenu]);
        assert!(k.tick(1100, false, true).is_empty(), "only once");
        assert!(
            k.on_key(0, 1200, true).is_empty(),
            "release after long does nothing"
        );
    }

    #[test]
    fn rule1_long_press_wakes_screen_and_respects_open_menu() {
        let mut k = PowerKey::default();
        k.on_key(1, 0, false);
        assert_eq!(
            k.tick(1000, false, false),
            vec![Act::WakeScreen, Act::OpenPowerMenu]
        );
        let mut k = PowerKey::default();
        k.on_key(1, 0, true);
        assert!(k.tick(1000, true, true).is_empty());
    }

    #[test]
    fn rule3_poll_interval() {
        let mut k = PowerKey::default();
        assert_eq!(k.poll_interval_ms(), 400);
        k.on_key(1, 0, false);
        assert_eq!(k.poll_interval_ms(), 100);
        k.on_key(0, 10, false);
        assert_eq!(k.poll_interval_ms(), 400);
    }

    #[test]
    fn stray_release_and_repeat_ignored() {
        let mut k = PowerKey::default();
        assert!(k.on_key(0, 5, false).is_empty());
        k.on_key(1, 0, false);
        assert!(k.on_key(2, 50, false).is_empty());
    }
}
