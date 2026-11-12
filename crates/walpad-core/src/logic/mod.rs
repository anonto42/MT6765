//! Behaviour-parity logic ported from the Python daemons (docs/rust plan, section 5).
//! Modules take monotonic milliseconds (`Ms`) as a parameter and return actions.

pub mod battery;
pub mod gesture;
pub mod jack;
pub mod key_gestures;
pub mod notifications;
pub mod power_key;
pub mod rotation;
pub mod screen_timeout;
pub mod soc;
pub mod touch_map;

/// Monotonic milliseconds.
pub type Ms = u64;
