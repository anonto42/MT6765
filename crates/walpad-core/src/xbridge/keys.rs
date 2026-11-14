use x11rb::protocol::xproto::Window;

/// X keysyms for the names used in keys.json (xdotool syntax).
pub fn keysym(name: &str) -> Option<u32> {
    let n = name.trim();
    let named = match n.to_ascii_lowercase().as_str() {
        "return" | "enter" => 0xff0d,
        "tab" => 0xff09,
        "escape" | "esc" => 0xff1b,
        "backspace" => 0xff08,
        "delete" => 0xffff,
        "home" => 0xff50,
        "left" => 0xff51,
        "up" => 0xff52,
        "right" => 0xff53,
        "down" => 0xff54,
        "prior" | "page_up" | "pageup" => 0xff55,
        "next" | "page_down" | "pagedown" => 0xff56,
        "end" => 0xff57,
        "space" => 0x20,
        "ctrl" | "control" => 0xffe3,
        "shift" => 0xffe1,
        "alt" => 0xffe9,
        "super" => 0xffeb,
        _ => 0,
    };
    if named != 0 {
        return Some(named);
    }
    if let Some(rest) = n.strip_prefix(['F', 'f']) {
        if let Ok(k) = rest.parse::<u32>() {
            if (1..=12).contains(&k) {
                return Some(0xffbd + k);
            }
        }
    }
    let mut c = n.chars();
    match (c.next(), c.next()) {
        (Some(ch), None) if ch.is_ascii_graphic() => Some(ch as u32),
        _ => None,
    }
}

/// Parse `ctrl+Tab` / `Prior` into (modifier keysyms, key keysym).
pub fn parse_key_spec(spec: &str) -> Option<(Vec<u32>, u32)> {
    let parts: Vec<&str> = spec.split('+').collect();
    let (key, mods) = parts.split_last()?;
    let mut m = Vec::new();
    for x in mods {
        let k = keysym(x)?;
        if !matches!(k, 0xffe3 | 0xffe1 | 0xffe9 | 0xffeb) {
            return None;
        }
        m.push(k);
    }
    Some((m, keysym(key)?))
}

/// WM_CLASS needles of the apps whose Back is Alt+Left (history / parent folder).
pub const BACK_ALT_LEFT: &[&str] = &["firefox", "pcmanfm"];

/// The key spec the Back control sends to a window of this class: Alt+Left in the browser and
/// the file manager, Esc anywhere else (also when no window is active).
pub fn back_spec(class: &[String]) -> &'static str {
    if BACK_ALT_LEFT.iter().any(|n| class_matches(class, n)) {
        "alt+Left"
    } else {
        "Escape"
    }
}

/// A top-level client window with the properties the process manager needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XWindow {
    pub id: Window,
    /// WM_CLASS components (instance, class), as written by the client.
    pub class: Vec<String>,
    /// _NET_WM_PID, if the client sets it.
    pub pid: Option<u32>,
    /// WM_PROTOCOLS lists WM_DELETE_WINDOW.
    pub deletable: bool,
}

/// apps.json `wm_class` matches a window when it equals the instance or class name (any case);
/// longer needles (4+ chars) may also be a substring (e.g. "firefox" for "firefox-esr").
pub fn class_matches(class: &[String], needle: &str) -> bool {
    let n = needle.trim().to_ascii_lowercase();
    if n.is_empty() {
        return false;
    }
    class.iter().any(|c| {
        let c = c.to_ascii_lowercase();
        c == n || (n.len() >= 4 && c.contains(&n))
    })
}

/// The top-level window under a touch (`XBridge::touch_hit`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TouchHit {
    /// Nothing with a class (the bare root, an unclassed window).
    None,
    /// A window of walpad-shell (WM_CLASS starting with `walpad`: shell, dock, keyboard, pad,
    /// selection toolbar, clipboard list).
    Shell,
    /// An app window, with its WM_CLASS components.
    App(Vec<String>),
}
