//! Main event loop, FIFO command ingestion, live input pumping, and `poll()` step.
use super::client::Client;
use super::{Daemon, CLIENT_STALL};
use rustix::event::{poll, PollFd, PollFlags, Timespec};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use walpad_hw::{battery::read_battery, gsensor::read_gsensor};
use walpad_proto::{parse_devd_line, LineDecoder};

impl Daemon {
    fn handle_fifo(&mut self) {
        let Some(f) = self.fifo.as_mut() else { return };
        let mut buf = [0u8; 512];
        loop {
            match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => self.fifo_buf.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }
        while let Some(pos) = self.fifo_buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.fifo_buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line).trim().to_string();
            match parse_devd_line(&line) {
                Ok(Some(cmd)) => {
                    if let Err((_, m)) = self.execute(cmd) {
                        eprintln!("walpad-core: devd.cmd {line:?}: {m}");
                    }
                }
                Ok(None) if line == "reload" => self.reload_cfg(),
                Ok(None) => {}
                Err(e) => eprintln!("walpad-core: devd.cmd {line:?}: {e}"),
            }
        }
    }

    /// Feed real input and timers to the Brain and execute what it decides (live mode).
    fn pump_live(&mut self) {
        let now = self.now_ms();
        let shell_menu = self.shell_menu_active();
        self.brain.shell_keys = shell_menu;
        self.brain.menu_open = shell_menu || self.root.p("/run/walpad/powermenu-open").exists();
        let evs = match self.live.as_mut() {
            Some(l) => l.inputs.poll(&self.root),
            None => return,
        };
        if !evs.is_empty() {
            self.wake_display();
        }
        let mut fx = Vec::new();
        {
            // one X query per touch: shell windows and `touch: mouse` apps keep the mouse
            // behaviour, everything else gets the touchscreen gestures
            let xb = self.xb.as_ref();
            let apps = &self.apps;
            let mut probe = |x: i32, y: i32| match xb.and_then(|b| b.touch_hit(x, y).ok()) {
                Some(crate::xbridge::TouchHit::App(class)) => apps.touch_mode(&class),
                _ => crate::logic::gesture::Mode::Mouse,
            };
            for (role, ev) in evs {
                fx.extend(self.brain.on_event_probe(role, &ev, now, &mut probe));
            }
        }
        let root = self.root.clone();
        let mut g = || read_gsensor(&root);
        let mut b = || read_battery(&root);
        fx.extend(self.brain.tick(now, &mut g, &mut b));
        self.apply(fx, false);
        self.x_maintain();
    }

    /// One iteration: wait for I/O (bounded), then service everything that is ready.
    pub fn step(&mut self, max_wait: Duration) {
        enum Slot {
            Listener,
            Fifo,
            Client(usize),
            Input,
        }
        let mut slots = Vec::new();
        let ready: Vec<(usize, bool, bool)>;
        let input_fds: Vec<_> = self
            .live
            .as_ref()
            .map(|l| l.inputs.poll_fds().collect())
            .unwrap_or_default();
        {
            let mut fds = Vec::new();
            fds.push(PollFd::new(&self.listener, PollFlags::IN));
            slots.push(Slot::Listener);
            if let Some(f) = &self.fifo {
                fds.push(PollFd::new(f, PollFlags::IN));
                slots.push(Slot::Fifo);
            }
            for fd in &input_fds {
                fds.push(PollFd::new(fd, PollFlags::IN));
                slots.push(Slot::Input);
            }
            for (i, c) in self.clients.iter().enumerate() {
                let mut fl = PollFlags::IN;
                if !c.out.is_empty() {
                    fl |= PollFlags::OUT;
                }
                fds.push(PollFd::new(&c.stream, fl));
                slots.push(Slot::Client(i));
            }
            let ms = max_wait.as_millis().min(1000) as i64;
            let t = Timespec {
                tv_sec: ms / 1000,
                tv_nsec: (ms % 1000) * 1_000_000,
            };
            let _ = poll(&mut fds, Some(&t));
            ready = fds
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let r = f.revents();
                    (
                        i,
                        r.intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR),
                        r.contains(PollFlags::OUT),
                    )
                })
                .collect();
        }
        let mut did = false;
        for (i, readable, writable) in ready {
            match slots[i] {
                Slot::Listener if readable => {
                    while let Ok((s, _)) = self.listener.accept() {
                        let _ = s.set_nonblocking(true);
                        self.clients.push(Client {
                            stream: s,
                            dec: LineDecoder::new(),
                            hello: false,
                            topics: vec![],
                            out: vec![],
                            stalled_since: None,
                            dead: false,
                            sync_pending: false,
                        });
                    }
                }
                Slot::Fifo if readable => {
                    self.handle_fifo();
                    did = true;
                }
                Slot::Client(ci) => {
                    if writable {
                        self.clients[ci].flush();
                    }
                    if readable {
                        let mut buf = [0u8; 4096];
                        let mut lines = Vec::new();
                        loop {
                            match self.clients[ci].stream.read(&mut buf) {
                                Ok(0) => {
                                    self.clients[ci].dead = true;
                                    break;
                                }
                                Ok(n) => match self.clients[ci].dec.feed(&buf[..n]) {
                                    Ok(l) => lines.extend(l),
                                    Err(_) => {
                                        self.clients[ci].dead = true;
                                        break;
                                    }
                                },
                                Err(_) => break,
                            }
                        }
                        for l in lines {
                            self.handle_line(ci, &l);
                            did = true;
                        }
                    }
                }
                _ => {}
            }
        }
        // drop dead or stalled clients (a client that does not read for 5 s is disconnected)
        let now = Instant::now();
        self.clients.retain(|c| {
            !c.dead
                && c.stalled_since
                    .is_none_or(|t| now.duration_since(t) < CLIENT_STALL)
        });
        self.tick_subsystems(did);
    }

    pub fn run(&mut self, stop: &AtomicBool) {
        while !stop.load(Ordering::Relaxed) {
            if let Some(sh) = self.shadow.as_mut() {
                sh.pump(&self.root);
                self.step(Duration::from_millis(40));
            } else if self.live.is_some() {
                self.pump_live();
                let cap = if self.real_root() { 1000 } else { 40 };
                let wait = self.brain.poll_interval_ms().clamp(10, cap);
                self.step(Duration::from_millis(wait));
            } else {
                self.step(Duration::from_millis(200));
            }
        }
        if let Some(mut b) = self.xb.take() {
            b.release_if_down();
        }
        let _ = std::fs::remove_file(&self.sock_path);
    }
}
