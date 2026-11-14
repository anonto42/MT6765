//! The shell event loop: X events + core events -> gestures -> app -> render -> present. Redraws
//! only when something changed (input, core status, the minute boundary); writes /run/walpad-ok
//! after the first frame is on screen (the boot ramdisk health check).

mod commands;
mod input_overlays;
mod lock_call;
mod step;

mod init;
mod poll;
use crate::app::App;
use crate::call_window::CallWindow;
use crate::clipboard::Keeper;
use crate::core_client::CoreClient;
use crate::cursor::CursorHider;
use crate::dock::Dock;
use crate::gesture::Recogniser;
use crate::ime::{FocusKbd, ImServer};
use crate::keyboard::SysKeyboard;
use crate::lock_window::LockWindow;
use crate::mousepad::MousePad;
use crate::nav::Nav;
use crate::seltools::{BarState, EditBar};
use crate::window::Window;
use crate::wmbar::WmBar;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::LockKind;
use walpad_ui::{Canvas, Fonts};

pub use init::{initial_size, Options};

pub struct Shell {
    pub(super) win: Window,
    /// The system dock (None when its X connection could not be opened or died).
    pub(super) dock: Option<Dock>,
    /// The window bar: terminal tabs, free-mode title bar and grip (None when it could not open).
    pub(super) wmbar: Option<WmBar>,
    /// The system keyboard (None when its X connection could not be opened or died).
    pub(super) kbd: Option<SysKeyboard>,
    /// The mouse pad (None when its X connection could not be opened or died).
    pub(super) pad: Option<MousePad>,
    /// Bottom-edge swipe (Home / Recents) and the Recents strip (None when its X connection could
    /// not be opened or died).
    pub(super) nav: Option<Nav>,
    /// `close_app` requests the Recents strip is waiting for: (request id, app id).
    pub(super) nav_close: Vec<(u64, String)>,
    /// The XIM server whose focus events drive the keyboard (None when it could not start or
    /// its X connection died: the terminal then gets the keyboard at start, as before).
    pub(super) ime: Option<ImServer>,
    /// The lock screen (None when its X connection could not be opened or died; retried while
    /// the core says the tablet is locked).
    pub(super) lock: Option<LockWindow>,
    /// The core says (or, before it answered, `/run/walpad/locked` says) the tablet is locked.
    pub(super) lock_wanted: bool,
    pub(super) lock_kind: LockKind,
    pub(super) lock_retry_in_s: Option<u32>,
    /// Id of the `unlock` request in flight.
    pub(super) lock_pending: Option<u64>,
    pub(super) lock_reopen_at: u64,
    /// The call screen: incoming / running call over every app and over the lock screen (None
    /// when its X connection could not be opened or died; retried while a call is on).
    pub(super) call: Option<CallWindow>,
    /// The last phone status of the core (a call is on while its state is not idle).
    pub(super) call_status: walpad_proto::PhoneStatus,
    pub(super) call_reopen_at: u64,
    pub(super) sock: PathBuf,
    /// Text-input focus -> keyboard visibility.
    pub(super) focus: FocusKbd,
    /// The selection toolbar and the clipboard list (None when their X connection died).
    pub(super) edit: Option<EditBar>,
    /// The clipboard keeper (None when XFixes or its connection is not available).
    pub(super) keeper: Option<Keeper>,
    /// When the selection toolbar shows.
    pub(super) bar: BarState,
    /// Keeps the X cursor hidden (cursor.rs); None when XFixes is not available.
    _cursor: Option<CursorHider>,
    pub(super) core: CoreClient,
    /// Folder scan and thumbnails for the Media screen (media_worker.rs).
    pub(super) media: crate::media_worker::Worker,
    pub(super) canvas: Canvas,
    pub(super) fonts: Fonts,
    pub app: App,
    pub(super) rec: Recogniser,
    pub(super) root: Root,
    pub(super) clock_24h: bool,
    started: Instant,
    pub(super) next_minute: Instant,
    pub(super) dirty: bool,
    pub(super) screen_off: bool,
    pub(super) need_refresh: bool,
    first_frame: bool,
    pub frames: u64,
}

impl Shell {
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
