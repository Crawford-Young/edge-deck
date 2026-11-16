//! Pure window-placement choice over a plain list of monitor descriptions.

/// Physical size of the target display (width, height).
pub const EDGE_SIZE: (u32, u32) = (2560, 720);

/// Default window size in logical pixels, used for the windowed fallback.
pub const DEFAULT_LOGICAL_SIZE: (u32, u32) = (800, 600);

/// A monitor as seen by the placement logic. All geometry is physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorInfo {
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

/// Where and how the main window should open. Geometry is physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Borderless fullscreen covering the matching display.
    Fullscreen {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    /// Normal decorated window centred on the primary display.
    Windowed {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    /// No monitors known: keep the OS default.
    Default,
}

/// Choose a placement.
///
/// The target is the display whose physical size is exactly 2560x720
/// (horizontal only). If several match, the leftmost wins, then the topmost.
/// With no match the window is centred on the primary display (the first
/// monitor if none is flagged primary); with no monitors at all the OS decides.
pub fn choose_placement(monitors: &[MonitorInfo]) -> Placement {
    if let Some(m) = monitors
        .iter()
        .filter(|m| (m.width, m.height) == EDGE_SIZE)
        .min_by_key(|m| (m.x, m.y))
    {
        return Placement::Fullscreen {
            x: m.x,
            y: m.y,
            width: m.width,
            height: m.height,
        };
    }
    match monitors
        .iter()
        .find(|m| m.is_primary)
        .or_else(|| monitors.first())
    {
        Some(m) => windowed_on(m),
        None => Placement::Default,
    }
}

fn windowed_on(m: &MonitorInfo) -> Placement {
    let scale = m.scale_factor;
    let width = (f64::from(DEFAULT_LOGICAL_SIZE.0) * scale).round();
    let height = (f64::from(DEFAULT_LOGICAL_SIZE.1) * scale).round();
    let x = f64::from(m.x) + (f64::from(m.width) - width) / 2.0;
    let y = f64::from(m.y) + (f64::from(m.height) - height) / 2.0;
    // Rounded values of window-sized quantities always fit these types.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Placement::Windowed {
        x: x.round() as i32,
        y: y.round() as i32,
        width: width as u32,
        height: height as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(width: u32, height: u32, x: i32, y: i32, scale: f64, primary: bool) -> MonitorInfo {
        MonitorInfo {
            width,
            height,
            x,
            y,
            scale_factor: scale,
            is_primary: primary,
        }
    }

    fn edge_at(x: i32, y: i32) -> Placement {
        Placement::Fullscreen {
            x,
            y,
            width: 2560,
            height: 720,
        }
    }

    #[test]
    fn edge_among_several_monitors_is_fullscreen_on_it() {
        let ms = [
            mon(2560, 1440, 0, 0, 1.0, true),
            mon(1920, 1080, 2560, 0, 1.0, false),
            mon(2560, 720, 7000, -300, 1.0, false),
        ];
        assert_eq!(choose_placement(&ms), edge_at(7000, -300));
    }

    #[test]
    fn edge_found_regardless_of_list_order_and_primary_flag() {
        let ms = [
            mon(2560, 720, -2560, 100, 1.0, true),
            mon(1920, 1080, 0, 0, 1.0, false),
        ];
        assert_eq!(choose_placement(&ms), edge_at(-2560, 100));
    }

    #[test]
    fn no_edge_centres_windowed_on_primary() {
        let ms = [
            mon(1920, 1080, 1920, 0, 1.0, false),
            mon(1920, 1080, 0, 0, 1.0, true),
        ];
        assert_eq!(
            choose_placement(&ms),
            Placement::Windowed {
                x: 560,
                y: 240,
                width: 800,
                height: 600
            }
        );
    }

    #[test]
    fn no_primary_flag_falls_back_to_first_monitor() {
        let ms = [
            mon(1920, 1080, 100, 50, 1.0, false),
            mon(1280, 1024, 0, 0, 1.0, false),
        ];
        assert_eq!(
            choose_placement(&ms),
            Placement::Windowed {
                x: 660,
                y: 290,
                width: 800,
                height: 600
            }
        );
    }

    #[test]
    fn two_matching_displays_pick_leftmost_then_topmost() {
        let ms = [
            mon(2560, 720, 4000, 0, 1.0, false),
            mon(2560, 720, 0, 900, 1.0, true),
            mon(2560, 720, 0, 100, 1.0, false),
        ];
        assert_eq!(choose_placement(&ms), edge_at(0, 100));
        let reversed = [ms[2], ms[1], ms[0]];
        assert_eq!(choose_placement(&reversed), edge_at(0, 100));
    }

    #[test]
    fn scaled_monitor_uses_physical_size_for_match() {
        // 2560x720 physical at 150% scale still matches.
        let ms = [
            mon(1920, 1080, 0, 0, 1.0, true),
            mon(2560, 720, 1920, 0, 1.5, false),
        ];
        assert_eq!(choose_placement(&ms), edge_at(1920, 0));
    }

    #[test]
    fn logical_size_match_is_not_enough() {
        // 1706x480 physical at 1.5 is ~2560x720 logical but not physical.
        let ms = [mon(3840, 1080, 0, 0, 1.5, true)];
        assert!(matches!(choose_placement(&ms), Placement::Windowed { .. }));
    }

    #[test]
    fn vertical_orientation_does_not_match() {
        let ms = [mon(720, 2560, 0, 0, 1.0, true)];
        assert!(matches!(choose_placement(&ms), Placement::Windowed { .. }));
    }

    #[test]
    fn windowed_size_scales_with_primary_scale_factor() {
        let ms = [mon(3840, 2160, 0, 0, 2.0, true)];
        assert_eq!(
            choose_placement(&ms),
            Placement::Windowed {
                x: 1120,
                y: 480,
                width: 1600,
                height: 1200
            }
        );
    }

    #[test]
    fn empty_list_keeps_os_default() {
        assert_eq!(choose_placement(&[]), Placement::Default);
    }
}
