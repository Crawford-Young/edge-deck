//! Pure decisions for putting the cursor back after a touch on the Edge.
//!
//! The Win32 side feeds a `Tracker` every low-level mouse event and every
//! raw-input report from the touchscreen, and asks it, once the reports have
//! gone quiet for `RESTORE_DELAY_MS`, where the cursor should go. All
//! geometry is physical screen pixels (the process is per-monitor DPI aware).

use crate::placement::Placement;

/// Mouse messages Windows synthesises from pen or touch carry this signature
/// in their extra info (`MI_WP_SIGNATURE`), under `SIGNATURE_MASK`.
const POINTER_SIGNATURE: usize = 0xFF51_5700;
const SIGNATURE_MASK: usize = 0xFFFF_FF00;

/// How long the touchscreen must stay quiet before the touch counts as ended
/// and the cursor goes back. A touchscreen reports at 100 Hz or more while any
/// finger is down (a held finger every 10 ms, 17 ms on a 60 Hz panel), so 50 ms
/// outlasts about three missed reports. The thread timer rounds up to the
/// 15.6 ms system tick, so the restore lands at most about 63 ms after the
/// last report, inside the 100 ms it must land in.
pub const RESTORE_DELAY_MS: u32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// A screen rectangle: `x..x + width` by `y..y + height`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn contains(&self, p: Point) -> bool {
        let (dx, dy) = (
            i64::from(p.x) - i64::from(self.x),
            i64::from(p.y) - i64::from(self.y),
        );
        (0..i64::from(self.width)).contains(&dx) && (0..i64::from(self.height)).contains(&dy)
    }
}

/// Where a low-level mouse event came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A real mouse.
    Mouse,
    /// Pen or touch that Windows promoted to mouse messages (for another
    /// app; a touch on edge-deck's webview is never promoted).
    Pointer,
}

/// Something the Win32 side saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// A low-level mouse hook event, with the cursor point it carries.
    Hook { source: Source, point: Point },
    /// A raw-input report from a touchscreen: some finger is down.
    TouchReport,
}

/// Classify a low-level mouse event by its `dwExtraInfo`.
pub fn source_of(extra_info: usize) -> Source {
    if extra_info & SIGNATURE_MASK == POINTER_SIGNATURE {
        Source::Pointer
    } else {
        Source::Mouse
    }
}

/// The Edge's screen rectangle, when the window was placed on it.
pub fn edge_rect(placement: &Placement) -> Option<Rect> {
    match *placement {
        Placement::Fullscreen {
            x,
            y,
            width,
            height,
        } => Some(Rect {
            x,
            y,
            width,
            height,
        }),
        Placement::Windowed { .. } | Placement::Default => None,
    }
}

/// Remembers the last cursor position a real mouse gave off the Edge, and
/// whether a restore is due.
#[derive(Debug)]
pub struct Tracker {
    edge: Rect,
    last_off_edge: Option<Point>,
    pending: Option<Point>,
}

impl Tracker {
    pub fn new(edge: Rect) -> Self {
        Tracker {
            edge,
            last_off_edge: None,
            pending: None,
        }
    }

    /// Take in one input. True when the caller must (re)start its
    /// `RESTORE_DELAY_MS` timer, so the timer fires only once the reports stop.
    ///
    /// - A touchscreen report, with a known position, makes the restore due.
    /// - A real mouse event cancels any due restore (the mouse is in use)
    ///   and, off the Edge, becomes the known position. Moving the cursor
    ///   back reads as a mouse event at that same position: no change.
    /// - Promoted pen or touch events are ignored.
    pub fn on_input(&mut self, input: Input) -> bool {
        match input {
            Input::TouchReport => {
                self.pending = self.last_off_edge;
                self.pending.is_some()
            }
            Input::Hook {
                source: Source::Mouse,
                point,
            } => {
                self.pending = None;
                if !self.edge.contains(point) {
                    self.last_off_edge = Some(point);
                }
                false
            }
            Input::Hook {
                source: Source::Pointer,
                ..
            } => false,
        }
    }

    /// The timer fired with the cursor at `cursor`: where to move it, if
    /// anywhere. Clears the due restore either way. Nothing moves when the
    /// cursor is not on the Edge.
    pub fn due(&mut self, cursor: Point) -> Option<Point> {
        self.pending.take().filter(|_| self.edge.contains(cursor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // An Edge to the right of a 2560x1440 main monitor, offset down.
    const EDGE: Rect = Rect {
        x: 2560,
        y: 360,
        width: 2560,
        height: 720,
    };
    const ON_EDGE: Point = Point { x: 3000, y: 700 };
    const MAIN: Point = Point { x: 1200, y: 800 };
    // Synthetic extra-info values: the pointer signature with and without
    // the touch bit, plus an arbitrary low byte.
    const TOUCH_INFO: usize = 0xFF51_5780 | 0x01;
    const PEN_INFO: usize = 0xFF51_5700 | 0x01;

    fn mouse(point: Point) -> Input {
        Input::Hook {
            source: Source::Mouse,
            point,
        }
    }

    fn promoted(point: Point) -> Input {
        Input::Hook {
            source: Source::Pointer,
            point,
        }
    }

    /// A tracker that last saw the real mouse at `MAIN`.
    fn seen_at_main() -> Tracker {
        let mut t = Tracker::new(EDGE);
        assert!(!t.on_input(mouse(MAIN)));
        t
    }

    #[test]
    fn source_reads_the_pointer_signature() {
        assert_eq!(source_of(0), Source::Mouse);
        assert_eq!(source_of(TOUCH_INFO), Source::Pointer);
        assert_eq!(source_of(PEN_INFO), Source::Pointer);
        // Only the masked bits count, so high bits on 64-bit do not hide it.
        assert_eq!(
            source_of(0x1234_0000_0000_0000 | TOUCH_INFO),
            Source::Pointer
        );
        // A near miss in the signature is a real mouse.
        assert_eq!(source_of(0xFF51_5880), Source::Mouse);
        assert_eq!(source_of(0x0051_5780), Source::Mouse);
    }

    #[test]
    fn rect_contains_its_half_open_extent() {
        assert!(EDGE.contains(Point { x: 2560, y: 360 }));
        assert!(EDGE.contains(Point { x: 5119, y: 1079 }));
        assert!(!EDGE.contains(Point { x: 5120, y: 700 }));
        assert!(!EDGE.contains(Point { x: 3000, y: 1080 }));
        assert!(!EDGE.contains(Point { x: 2559, y: 700 }));
        assert!(!EDGE.contains(Point { x: 3000, y: 359 }));
    }

    #[test]
    fn rect_far_out_does_not_overflow() {
        let far = Rect {
            x: i32::MAX - 10,
            y: i32::MAX - 10,
            width: u32::MAX,
            height: u32::MAX,
        };
        assert!(far.contains(Point {
            x: i32::MAX,
            y: i32::MAX
        }));
        assert!(!far.contains(Point { x: 0, y: 0 }));
    }

    #[test]
    fn edge_rect_is_the_fullscreen_placement_only() {
        let full = Placement::Fullscreen {
            x: 2560,
            y: 360,
            width: 2560,
            height: 720,
        };
        assert_eq!(edge_rect(&full), Some(EDGE));
        let windowed = Placement::Windowed {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        };
        assert_eq!(edge_rect(&windowed), None);
        assert_eq!(edge_rect(&Placement::Default), None);
    }

    #[test]
    fn touch_activity_then_quiet_puts_the_cursor_back() {
        let mut t = seen_at_main();
        // Windows jumped the cursor onto the Edge; a report asks for the timer.
        assert!(t.on_input(Input::TouchReport));
        // The timer fired with no report since: the touch has ended.
        assert_eq!(t.due(ON_EDGE), Some(MAIN));
        // Once only.
        assert_eq!(t.due(ON_EDGE), None);
    }

    #[test]
    fn every_report_restarts_the_delay() {
        let mut t = seen_at_main();
        // A held finger, a drag or a pinch keeps reporting: each report asks
        // for the timer again, so it only fires after the last one.
        for _ in 0..20 {
            assert!(t.on_input(Input::TouchReport));
        }
        assert_eq!(t.due(ON_EDGE), Some(MAIN));
    }

    #[test]
    fn a_touch_after_a_restore_restores_again() {
        let mut t = seen_at_main();
        assert!(t.on_input(Input::TouchReport));
        assert_eq!(t.due(ON_EDGE), Some(MAIN));
        assert!(t.on_input(Input::TouchReport));
        assert_eq!(t.due(ON_EDGE), Some(MAIN));
    }

    #[test]
    fn the_restore_goes_to_the_latest_position_off_the_edge() {
        let mut t = seen_at_main();
        let later = Point { x: 10, y: 20 };
        t.on_input(mouse(later));
        // The mouse then crosses onto the Edge; that is not remembered.
        t.on_input(mouse(ON_EDGE));
        assert!(t.on_input(Input::TouchReport));
        assert_eq!(t.due(ON_EDGE), Some(later));
    }

    #[test]
    fn nothing_moves_without_a_known_position() {
        let mut t = Tracker::new(EDGE);
        assert!(!t.on_input(Input::TouchReport));
        assert_eq!(t.due(ON_EDGE), None);
        // A real mouse seen only on the Edge is still no known position.
        t.on_input(mouse(ON_EDGE));
        assert!(!t.on_input(Input::TouchReport));
        assert_eq!(t.due(ON_EDGE), None);
    }

    #[test]
    fn no_restore_when_the_cursor_is_off_the_edge_at_fire_time() {
        let mut t = seen_at_main();
        // A touch on another touchscreen, or the cursor already moved away.
        assert!(t.on_input(Input::TouchReport));
        assert_eq!(t.due(Point { x: 100, y: 100 }), None);
    }

    #[test]
    fn promoted_pen_or_touch_events_are_never_a_known_position() {
        let mut t = Tracker::new(EDGE);
        assert!(!t.on_input(promoted(MAIN)));
        assert!(!t.on_input(Input::TouchReport));
        assert_eq!(t.due(ON_EDGE), None);
    }

    #[test]
    fn promoted_events_neither_move_the_target_nor_cancel_it() {
        let mut t = seen_at_main();
        assert!(t.on_input(Input::TouchReport));
        // Another app's touch promoted to mouse messages, off the Edge and on.
        assert!(!t.on_input(promoted(Point { x: 10, y: 20 })));
        assert!(!t.on_input(promoted(ON_EDGE)));
        assert_eq!(t.due(ON_EDGE), Some(MAIN));
    }

    #[test]
    fn a_real_mouse_on_the_edge_is_left_alone() {
        let mut t = seen_at_main();
        assert!(!t.on_input(mouse(ON_EDGE)));
        assert!(!t.on_input(mouse(Point { x: 4000, y: 900 })));
        assert_eq!(t.due(ON_EDGE), None);
    }

    #[test]
    fn a_real_mouse_event_after_the_touch_cancels_the_restore() {
        let mut t = seen_at_main();
        assert!(t.on_input(Input::TouchReport));
        // The mouse is in use on the Edge before the timer fires.
        let grabbed = Point { x: 2600, y: 900 };
        assert!(!t.on_input(mouse(grabbed)));
        assert_eq!(t.due(grabbed), None);
        // A later touch still goes back to the last place off the Edge.
        assert!(t.on_input(Input::TouchReport));
        assert_eq!(t.due(ON_EDGE), Some(MAIN));
    }

    #[test]
    fn the_restore_reads_back_as_no_change() {
        let mut t = seen_at_main();
        assert!(t.on_input(Input::TouchReport));
        assert_eq!(t.due(ON_EDGE), Some(MAIN));
        // If moving the cursor shows up as a mouse event at the target, it
        // arms nothing and keeps the same known position.
        assert!(!t.on_input(mouse(MAIN)));
        assert_eq!(t.due(MAIN), None);
        assert!(t.on_input(Input::TouchReport));
        assert_eq!(t.due(ON_EDGE), Some(MAIN));
    }
}
