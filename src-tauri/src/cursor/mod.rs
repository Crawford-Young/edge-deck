//! Put the mouse cursor back after a touch on the Edge.
//!
//! Windows moves the cursor to the touch point. A low-level mouse hook
//! (`windows.rs`) remembers where the real mouse last was off the Edge
//! (events carrying the pen/touch signature never count), raw input from the
//! touchscreen tells that a finger is down, and once its reports stop for
//! `plan::RESTORE_DELAY_MS` the cursor goes back, if it is on the Edge. Every
//! decision is the pure `plan::Tracker`; elsewhere than Windows this is a
//! no-op.

// The listener is Windows-only; elsewhere only the tests use the tracker.
#![cfg_attr(not(windows), allow(dead_code))]

mod plan;
#[cfg(windows)]
mod windows;

pub use plan::edge_rect;

#[cfg(windows)]
pub use windows::Restorer;

/// No hook outside Windows (#7 tracks macOS).
#[cfg(not(windows))]
pub struct Restorer;

#[cfg(not(windows))]
impl Restorer {
    pub fn start(_edge: plan::Rect) -> Result<Self, String> {
        Ok(Restorer)
    }

    pub fn stop(&self) {}
}
