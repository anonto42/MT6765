//! Brain effect execution, key-gesture actions, and display wake for `Daemon`.

use crate::brain::Effect;
use crate::logic::{key_gestures::Action, touch_map as tm};
use std::io::Write;
use std::time::Instant;
use walpad_hw::sysfs;
use walpad_proto::{encode_line, parse_devd_line, Event, KeyboardMode, ShellInput, Topic};

use super::{Daemon, SHELL_MENU_MAX_MS};

impl Daemon {
    /// A shell client has subscribed to the `input` topic.
    pub(super) fn shell_listening(&self) -> bool {
        self.clients
            .iter()
            .any(|c| c.hello && c.topics.contains(&Topic::Input))
    }

    /// The shell's power menu is open (it told us, or we opened it, and it is not stale).
    pub(super) fn shell_menu_active(&self) -> bool {
        self.shell_menu_until.is_some_and(|t| self.now_ms() < t) && self.shell_listening()
    }

    pub(super) fn publish_input(&mut self, k: ShellInput) {
        let ev = encode_line(&Event::new(
            "input",
            serde_json::to_value(k).unwrap_or_default(),
        ));
        for c in &mut self.clients {
            if c.hello && c.topics.contains(&Topic::Input) {
                c.queue(ev.clone());
            }
        }
    }

    /// Long power press / `powermenu` key action: the shell draws the menu when it listens for
    /// hardware keys, otherwise the Tk fallback runs.
    fn open_power_menu(&mut self) {
        if self.shell_listening() {
            self.shell_menu_until = Some(self.now_ms() + SHELL_MENU_MAX_MS);
            self.publish_input(ShellInput::PowerMenuOpen);
        } else {
            self.spawn_simple("walpad-powermenu.py", &[]);
        }
    }

    fn run_action(&mut self, a: Action) {
        // a locked tablet takes no key shortcut that reaches an app or the shell
        if self.lock.locked()
            && matches!(
                a,
                Action::Key(_)
                    | Action::Launch(_)
                    | Action::Run(_)
                    | Action::Home
                    | Action::Keyboard
                    | Action::PowerMenu
                    | Action::Screenshot
            )
        {
            return;
        }
        match a {
            Action::None => {}
            Action::Key(spec) => {
                self.x_try(|b| b.send_key(&spec));
            }
            Action::Ctl(line) => match parse_devd_line(&line) {
                Ok(Some(cmd)) => {
                    let device_live = self.live.as_ref().is_some_and(|l| l.device);
                    if device_live {
                        if let Err((_, m)) = self.execute(cmd) {
                            eprintln!("walpad-core: ctl:{line}: {m}");
                        }
                    } else if let Err(e) = std::fs::OpenOptions::new()
                        .write(true)
                        .open(self.root.p("/run/walpad/devd.cmd"))
                        .and_then(|mut f| writeln!(f, "{line}"))
                    {
                        eprintln!("walpad-core: devd.cmd not reachable: {e}");
                    }
                }
                Ok(None) => self.reload_cfg(),
                Err(e) => eprintln!("walpad-core: bad ctl action {line:?}: {e}"),
            },
            Action::Launch(id) => match self.launch(&id, None) {
                Ok(how) => eprintln!("walpad-core: launch {id}: {how}"),
                Err((_, m)) => eprintln!("walpad-core: launch {id}: {m}"),
            },
            Action::Run(c) => eprintln!("walpad-core: run:{c} rejected (use launch:<app-id>)"),
            Action::Home => {
                self.x_try(|b| b.focus_title("WalPad Home"));
                if self.keyboard_shown {
                    self.keyboard(KeyboardMode::Hide);
                }
            }
            Action::Back => {
                self.x_try(|b| b.back());
            }
            Action::Keyboard => {
                self.keyboard(KeyboardMode::Toggle);
            }
            Action::PowerMenu => self.open_power_menu(),
            Action::Screenshot => {
                if let Err((_, m)) = self.screenshot() {
                    eprintln!("walpad-core: {m}");
                }
            }
            Action::Volume(v) => {
                let now = Instant::now();
                let a = self.volume.delta(v, now);
                self.publish_audio("key", a);
            }
            Action::Unknown(s) => eprintln!("walpad-core: unknown key action {s:?}"),
        }
    }

    /// Execute effects. `forced` = explicit client request (always applied); otherwise only the
    /// subsystems this core runs live.
    pub(super) fn apply(&mut self, effects: Vec<Effect>, forced: bool) {
        let (dev, inp) = match &self.live {
            Some(l) => (l.device, l.input),
            None => (false, false),
        };
        for e in effects {
            if !forced && ((e.is_device() && !dev) || (!e.is_device() && !inp)) {
                continue;
            }
            match e {
                Effect::Backlight(v) => {
                    if let Err(err) = sysfs::write_backlight(&self.root, v) {
                        eprintln!("walpad-core: backlight write failed: {err}");
                    }
                    let off = v == 0;
                    if let Err(err) = sysfs::write_fb_blank(&self.root, off) {
                        eprintln!("walpad-core: fb blank failed: {err}");
                    }
                    // Screen off → powersave governor; screen on → restore schedutil.
                    // Best-effort: ignore if the kernel has no cpufreq or the path is read-only.
                    let gov = if off { "powersave" } else { "schedutil" };
                    let n = sysfs::write_cpufreq_governor(&self.root, gov);
                    if n > 0 {
                        eprintln!("walpad-core: cpufreq governor → {gov} ({n} policies)");
                    }
                    if !off {
                        self.wake_display();
                    }
                }
                Effect::Orientation(o) => {
                    if let Err(err) =
                        std::fs::write(self.root.p("/run/walpad/orientation"), o.to_string())
                    {
                        eprintln!("walpad-core: orientation write failed: {err}");
                    }
                }
                Effect::Struts(o) => {
                    self.x_try(|b| b.set_struts(o));
                    // tile again for the new orientation
                    self.layout_at = Instant::now();
                }
                Effect::KeyboardDock(o) => {
                    self.x_try(|b| b.dock_keyboard(o).map(|_| ()));
                }
                Effect::PowerMenuOpen if self.lock.locked() => {}
                Effect::PowerMenuOpen => self.open_power_menu(),
                Effect::LockNow => {
                    self.lock.lock_now();
                }
                Effect::PowerMenuActivate => {
                    if self.shell_menu_active() {
                        self.publish_input(ShellInput::PowerShort);
                    } else {
                        self.x_try(|b| b.send_key("Return"));
                    }
                }
                Effect::ShellKey(k) => self.publish_input(k),
                Effect::Cover(c) => {
                    eprintln!("walpad-core: cover {}", if c { "closed" } else { "open" })
                }
                Effect::LowBattery(l) => eprintln!("walpad-core: low battery flag -> {l}"),
                Effect::CriticalPoweroff(mv) => {
                    eprintln!("walpad-core: CRITICAL battery {mv} mV: safe power-off");
                    self.halt("poweroff");
                }
                Effect::Pointer(tm::Act::Move(x, y)) => {
                    self.x_try(|b| b.pointer_move(x, y));
                }
                Effect::Pointer(tm::Act::Press) => {
                    self.x_try(|b| b.button(true));
                }
                Effect::Pointer(tm::Act::Release) => {
                    self.x_try(|b| b.button(false));
                }
                Effect::Pointer(tm::Act::Pair(..)) => {}
                Effect::Button(n, d) => {
                    self.x_try(|b| b.button_n(n, d));
                }
                Effect::Wheel { button, ctrl } => {
                    self.x_try(|b| b.wheel(button, ctrl));
                }
                Effect::Action(a) => self.run_action(a),
            }
        }
    }

    /// Poke fbmirror through its FIFO so it leaves its idle polling at once and the reaction to a
    /// touch reaches the panel without the idle delay. Never blocks: non-blocking open/write, every
    /// error (no fbmirror, FIFO full) is ignored.
    pub(super) fn wake_display(&self) {
        use rustix::fs::{Mode, OFlags};
        if let Ok(fd) = rustix::fs::open(
            self.root.p("/run/walpad/fbwake"),
            OFlags::WRONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            let _ = rustix::io::write(&fd, &[1u8]);
        }
    }
}
