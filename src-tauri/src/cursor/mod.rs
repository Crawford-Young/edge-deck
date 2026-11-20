//! Put the mouse cursor back after a touch on the Edge.
//!
//! Windows moves the cursor to every touch point. A low-level mouse hook
//! (`windows.rs`) tells touch from a real mouse by the pointer signature in
//! each event's extra info, remembers where the real mouse last was off the
//! Edge, and once a touch on the Edge ends moves the cursor back there. Every
//! decision is the pure `plan::Tracker`; elsewhere than Windows this is a
//! no-op.

// The hook is Windows-only; elsewhere only the tests use the tracker.
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
