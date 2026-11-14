//! Pure headphone jack / mic routing logic (owner request 2026-11-12), tested without any I/O.
//! See `walpad_hw::jack` for the honest hardware-read limits (no live detection on this kernel
//! build) and `crate::jack` for the module that drives the script and holds the persisted state.
use walpad_proto::JackKind;

/// The analog output route the core should apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Speaker,
    Headphone,
}

/// Which mic input should feed the capture PCM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicSource {
    BuiltIn,
    HeadsetMic,
}

/// Live accdet detection wins when present; otherwise the owner's manual override; otherwise
/// neither is in effect (speaker + built-in mic).
pub fn effective(detected: Option<JackKind>, forced: Option<JackKind>) -> Option<JackKind> {
    detected.or(forced)
}

pub fn route_for(kind: Option<JackKind>) -> Route {
    match kind {
        Some(_) => Route::Headphone,
        None => Route::Speaker,
    }
}

/// Only a confirmed mic-equipped headset moves the mic off the built-in path: plain
/// `Headphones` (no confirmed mic) leaves the built-in mic recording, matching a phone that
/// plugged in simple wired earbuds with no mic.
pub fn mic_source_for(kind: Option<JackKind>) -> MicSource {
    match kind {
        Some(JackKind::HeadsetWithMic) => MicSource::HeadsetMic,
        _ => MicSource::BuiltIn,
    }
}

#[cfg(test)]
mod tests;
