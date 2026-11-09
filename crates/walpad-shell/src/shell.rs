//! The shell event loop: X events -> gestures -> app -> render -> present. Redraws only when
//! something changed; writes /run/walpad-ok after the first frame is on screen (the boot ramdisk
//! health check).
use crate::app::App;
use crate::gesture::{Gesture, Recogniser};
use crate::window::{WinEvent, Window};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_ui::components::Painter;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts};

pub struct Options {
    pub root: Root,
    /// X socket, default <root>/tmp/.X11-unix/X0.
    pub x_socket: Option<PathBuf>,
    pub fonts_dir: Option<PathBuf>,
}

pub struct Shell {
    win: Window,
    canvas: Canvas,
    fonts: Fonts,
    pub app: App,
    rec: Recogniser,
    root: Root,
    started: Instant,
    dirty: bool,
    first_frame: bool,
    pub frames: u64,
}

/// Initial size from the orientation file (the window manager resizes it to the visible area).
pub fn initial_size(root: &Root) -> (i32, i32) {
    let o = std::fs::read_to_string(root.p("/run/walpad/orientation"))
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(0);
    if matches!(o, 90 | 270) {
        (1280, 800)
    } else {
        (800, 1280)
    }
}

impl Shell {
    pub fn new(opts: Options) -> Result<Shell, String> {
        let fonts = match &opts.fonts_dir {
            Some(d) => Fonts::load(d),
            None => Fonts::load_default(),
        }
        .map_err(|e| format!("fonts: {e:?}"))?;
        let (w, h) = initial_size(&opts.root);
        let sock = opts
            .x_socket
            .clone()
            .unwrap_or_else(|| opts.root.p("/tmp/.X11-unix/X0"));
        let win = Window::open(&sock, w, h).map_err(|e| format!("X: {}", e.0))?;
        let canvas =
            Canvas::new(w as u32, h as u32, BG).ok_or("cannot allocate the frame buffer")?;
        Ok(Shell {
            win,
            canvas,
            fonts,
            app: App::new(w, h),
            rec: Recogniser::new(),
            root: opts.root,
            started: Instant::now(),
            dirty: true,
            first_frame: true,
            frames: 0,
        })
    }

    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn apply_gestures(&mut self, gs: Vec<Gesture>) {
        for g in gs {
            if self.app.on_gesture(&g) {
                self.dirty = true;
            }
        }
    }

    fn resize(&mut self, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            return;
        }
        if let Some(c) = Canvas::new(w as u32, h as u32, BG) {
            self.canvas = c;
            self.app.resize(w, h);
            self.dirty = true;
        }
    }

    fn redraw(&mut self) -> Result<(), String> {
        self.app.render(&mut Painter {
            cv: &mut self.canvas,
            fonts: &mut self.fonts,
        });
        self.win
            .present(&self.canvas, self.canvas.bounds())
            .map_err(|e| format!("present: {}", e.0))?;
        self.frames += 1;
        if self.first_frame {
            self.first_frame = false;
            // the boot ramdisk kills the session and falls back to padui if this is missing after 150 s
            let _ = std::fs::write(self.root.p("/run/walpad-ok"), "ok\n");
        }
        self.dirty = false;
        Ok(())
    }

    /// One loop iteration. Returns Err when the X connection is gone (the supervisor restarts us).
    pub fn step(&mut self, max_wait: Duration) -> Result<(), String> {
        let events = self.win.poll_events().map_err(|e| e.0)?;
        let now = self.now_ms();
        for ev in events {
            match ev {
                WinEvent::Input(i) => {
                    let gs = self.rec.feed(i, now);
                    self.apply_gestures(gs);
                }
                WinEvent::Resize(w, h) => self.resize(w, h),
                WinEvent::Expose => self.dirty = true,
                WinEvent::Key(_) => {}
            }
        }
        let gs = self.rec.tick(now);
        self.apply_gestures(gs);
        if self.dirty {
            self.redraw()?;
        }
        let until_long = self
            .rec
            .next_deadline()
            .map(|d| d.saturating_sub(self.now_ms()));
        let wait = until_long
            .map_or(max_wait.as_millis() as u64, |d| {
                d.min(max_wait.as_millis() as u64)
            })
            .max(1);
        self.win.wait(Some(wait));
        Ok(())
    }

    pub fn run(&mut self, stop: &AtomicBool) -> Result<(), String> {
        while !stop.load(Ordering::Relaxed) {
            self.step(Duration::from_secs(5))?;
        }
        Ok(())
    }

    pub fn size(&self) -> (i32, i32) {
        self.win.size
    }
}
