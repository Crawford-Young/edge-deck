//! Pure decisions for putting the cursor back after a touch on the Edge.
//!
//! The Win32 side feeds every low-level mouse event to a `Tracker` and asks
//! it, a short delay after a touch ends, where the cursor should go. All
//! geometry is physical screen pixels (the process is per-monitor DPI aware).

use crate::placement::Placement;

/// Mouse messages Windows synthesises from pen or touch carry this signature
/// in their extra info (`MI_WP_SIGNATURE`), under `SIGNATURE_MASK`.
const POINTER_SIGNATURE: usize = 0xFF51_5700;
const SIGNATURE_MASK: usize = 0xFFFF_FF00;
/// Within a pointer signature, this bit marks touch rather than pen.
const TOUCH_BIT: usize = 0x80;

/// How long after a touch ends the cursor is put back. Long enough for the
/// burst of mouse messages Windows promotes from one tap to finish, well
/// inside the 100 ms the restore must land in.
pub const RESTORE_DELAY_MS: u32 = 40;

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

/// Where a mouse event came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Mouse,
    Pen,
    Touch,
}

/// What a mouse event did. Touch promotes a tap to left down/up and a
/// press-and-hold to right down/up; anything else reads as a move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Move,
    Down,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Input {
    pub source: Source,
    pub kind: Kind,
    pub point: Point,
}

/// Classify a low-level mouse event by its `dwExtraInfo`.
pub fn source_of(extra_info: usize) -> Source {
    if extra_info & SIGNATURE_MASK != POINTER_SIGNATURE {
        Source::Mouse
    } else if extra_info & TOUCH_BIT != 0 {
        Source::Touch
    } else {
        Source::Pen
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

    /// Take in one mouse event. True when a restore became due, so the
    /// caller (re)starts its `RESTORE_DELAY_MS` timer.
    ///
    /// - A real mouse event cancels any due restore (the mouse is in use)
    ///   and, off the Edge, becomes the known position. Moving the cursor
    ///   back reads as a mouse event at that same position: no change.
    /// - A touch button-up on the Edge, with a known position, makes the
    ///   restore due. Any other touch event (a move, a new contact) cancels
    ///   it, so the cursor never moves while a touch is down.
    /// - Pen input is ignored.
    pub fn on_input(&mut self, input: Input) -> bool {
        match input.source {
            Source::Mouse => {
                self.pending = None;
                if !self.edge.contains(input.point) {
                    self.last_off_edge = Some(input.point);
                }
                false
            }
            Source::Touch => {
                let ended = input.kind == Kind::Up && self.edge.contains(input.point);
                self.pending = if ended { self.last_off_edge } else { None };
                self.pending.is_some()
            }
            Source::Pen => false,
        }
    }

    /// The timer fired with the cursor at `cursor`: where to move it, if
    /// anywhere. Clears the due restore either way. Nothing moves when the
    /// cursor has already left the Edge.
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

    fn input(source: Source, kind: Kind, point: Point) -> Input {
        Input {
            source,
            kind,
            point,
        }
    }

    fn mouse_move(point: Point) -> Input {
        input(Source::Mouse, Kind::Move, point)
    }

    fn touch(kind: Kind, point: Point) -> Input {
        input(Source::Touch, kind, point)
    }

    /// A tracker that last saw the real mouse at `MAIN`.
    fn seen_at_main() -> Tracker {
        let mut t = Tracker::new(EDGE);
        assert!(!t.on_input(mouse_move(MAIN)));
        t
    }

    /// Feed a whole tap on the Edge; true if the last event made a restore due.
    fn tap(t: &mut Tracker, at: Point) -> bool {
        t.on_input(touch(Kind::Move, at));
        t.on_input(touch(Kind::Down, at));
        t.on_input(touch(Kind::Up, at))
    }

    #[test]
    fn source_reads_the_pointer_signature() {
        assert_eq!(source_of(0), Source::Mouse);
        assert_eq!(source_of(TOUCH_INFO), Source::Touch);
        assert_eq!(source_of(PEN_INFO), Source::Pen);
        // Only the masked bits count, so high bits on 64-bit do not hide it.
        assert_eq!(source_of(0x1234_0000_0000_0000 | TOUCH_INFO), Source::Touch);
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
    fn a_tap_on_the_edge_puts_the_cursor_back_where_the_mouse_was() {
        let mut t = seen_at_main();
        assert!(tap(&mut t, ON_EDGE));
        assert_eq!(t.due(ON_EDGE), Some(MAIN));
        // Once only.
        assert_eq!(t.due(ON_EDGE), None);
    }

    #[test]
    fn the_restore_goes_to_the_latest_position_off_the_edge() {
        let mut t = seen_at_main();
        let later = Point { x: 10, y: 20 };
        t.on_input(mouse_move(later));
        // The mouse then crosses onto the Edge; that is not remembered.
        t.on_input(mouse_move(ON_EDGE));
        assert!(tap(&mut t, ON_EDGE));
        assert_eq!(t.due(ON_EDGE), Some(later));
    }

    #[test]
    fn press_and_hold_ends_with_a_right_up_and_restores() {
        let mut t = seen_at_main();
        t.on_input(touch(Kind::Down, ON_EDGE));
        assert!(t.on_input(touch(Kind::Up, ON_EDGE)));
        assert_eq!(t.due(ON_EDGE), Some(MAIN));
    }

    #[test]
    fn a_real_mouse_on_the_edge_is_left_alone() {
        let mut t = seen_at_main();
        assert!(!t.on_input(mouse_move(ON_EDGE)));
        assert!(!t.on_input(input(Source::Mouse, Kind::Down, ON_EDGE)));
        assert!(!t.on_input(input(Source::Mouse, Kind::Up, ON_EDGE)));
        assert_eq!(t.due(ON_EDGE), None);
    }

    #[test]
    fn pen_input_never_restores_and_is_not_remembered() {
        let mut t = Tracker::new(EDGE);
        assert!(!t.on_input(input(Source::Pen, Kind::Move, MAIN)));
        assert!(!t.on_input(input(Source::Pen, Kind::Up, ON_EDGE)));
        // The pen's off-Edge point was not taken as a known position.
        assert!(!tap(&mut t, ON_EDGE));
        assert_eq!(t.due(ON_EDGE), None);
    }

    #[test]
    fn no_restore_while_a_touch_is_down() {
        let mut t = seen_at_main();
        t.on_input(touch(Kind::Down, ON_EDGE));
        for x in [3000, 3100, 3200] {
            assert!(!t.on_input(touch(Kind::Move, Point { x, y: 700 })));
            assert_eq!(t.due(Point { x, y: 700 }), None);
        }
        assert!(t.on_input(touch(Kind::Up, Point { x: 3200, y: 700 })));
        assert_eq!(t.due(Point { x: 3200, y: 700 }), Some(MAIN));
    }

    #[test]
    fn a_new_touch_inside_the_delay_cancels_the_restore() {
        let mut t = seen_at_main();
        assert!(tap(&mut t, ON_EDGE));
        // A second finger-down (a double tap, or a drag starting) before the
        // timer fires: the first restore must not land mid-gesture.
        assert!(!t.on_input(touch(Kind::Move, ON_EDGE)));
        assert!(!t.on_input(touch(Kind::Down, ON_EDGE)));
        assert_eq!(t.due(ON_EDGE), None);
        // The second touch's own end restores.
        assert!(t.on_input(touch(Kind::Up, ON_EDGE)));
        assert_eq!(t.due(ON_EDGE), Some(MAIN));
    }

    #[test]
    fn a_real_mouse_move_inside_the_delay_cancels_the_restore() {
        let mut t = seen_at_main();
        assert!(tap(&mut t, ON_EDGE));
        let grabbed = Point { x: 2600, y: 900 };
        t.on_input(mouse_move(grabbed));
        assert_eq!(t.due(grabbed), None);
    }

    #[test]
    fn nothing_moves_without_a_known_earlier_position() {
        let mut t = Tracker::new(EDGE);
        assert!(!tap(&mut t, ON_EDGE));
        assert_eq!(t.due(ON_EDGE), None);
        // A real mouse seen only on the Edge is still no known position.
        t.on_input(mouse_move(ON_EDGE));
        assert!(!tap(&mut t, ON_EDGE));
        assert_eq!(t.due(ON_EDGE), None);
    }

    #[test]
    fn a_touch_ending_off_the_edge_does_not_restore() {
        let mut t = seen_at_main();
        let other = Point { x: 500, y: 500 };
        assert!(!tap(&mut t, other));
        assert_eq!(t.due(other), None);
        // Nor is that touch point taken as a known position.
        assert!(tap(&mut t, ON_EDGE));
        assert_eq!(t.due(ON_EDGE), Some(MAIN));
    }

    #[test]
    fn no_restore_when_the_cursor_already_left_the_edge() {
        let mut t = seen_at_main();
        assert!(tap(&mut t, ON_EDGE));
        // Something else moved the cursor off the Edge before the timer.
        assert_eq!(t.due(Point { x: 100, y: 100 }), None);
    }

    #[test]
    fn the_restore_reads_back_as_no_change() {
        let mut t = seen_at_main();
        assert!(tap(&mut t, ON_EDGE));
        let target = t.due(ON_EDGE);
        assert_eq!(target, Some(MAIN));
        // If moving the cursor shows up as a mouse event at the target, it
        // arms nothing and keeps the same known position.
        assert!(!t.on_input(mouse_move(MAIN)));
        assert_eq!(t.due(MAIN), None);
        assert!(tap(&mut t, ON_EDGE));
        assert_eq!(t.due(ON_EDGE), Some(MAIN));
    }
}
