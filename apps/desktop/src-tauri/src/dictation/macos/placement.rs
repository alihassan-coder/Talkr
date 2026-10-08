//! Where the pill goes on a Mac screen, as pure arithmetic in Cocoa coordinates (points, origin
//! at the bottom left of the main display, y up).

use crate::dictation::settings::OverlayPosition;

/// Gap between the pill's window and the edge of the usable screen area, in points.
const MARGIN: f64 = 14.0;

/// A rectangle: x, y of its bottom-left corner, width, height.
pub type Frame = (f64, f64, f64, f64);

/// The pill window's bottom-left corner: centred across `visible` (a screen without its menu
/// bar and Dock), above its bottom edge or below its top.
pub fn pill_origin(visible: Frame, size: (f64, f64), position: OverlayPosition) -> (f64, f64) {
    let (x, y, width, height) = visible;
    let left = x + (width - size.0) / 2.0;
    let bottom = match position {
        OverlayPosition::Bottom => y + MARGIN,
        OverlayPosition::Top => y + height - size.1 - MARGIN,
    };
    (left.round(), bottom.round())
}

/// A point from Accessibility coordinates (origin at the top left of the main display, y down)
/// in Cocoa coordinates.
pub fn from_top_left(point: (f64, f64), main_height: f64) -> (f64, f64) {
    (point.0, main_height - point.1)
}

pub fn contains(frame: Frame, point: (f64, f64)) -> bool {
    let (x, y, width, height) = frame;
    point.0 >= x && point.0 < x + width && point.1 >= y && point.1 < y + height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pill_sits_centred_above_the_dock_or_below_the_menu_bar() {
        // A 1440 x 900 display with a 25 pt menu bar and a 70 pt Dock.
        let visible = (0.0, 70.0, 1440.0, 805.0);
        assert_eq!(pill_origin(visible, (460.0, 96.0), OverlayPosition::Bottom), (490.0, 84.0));
        assert_eq!(pill_origin(visible, (460.0, 96.0), OverlayPosition::Top), (490.0, 765.0));
        // A second display to the left of the main one, lower down.
        let left = (-1920.0, -300.0, 1920.0, 1055.0);
        assert_eq!(pill_origin(left, (460.0, 96.0), OverlayPosition::Bottom), (-1190.0, -286.0));
    }

    #[test]
    fn accessibility_points_are_flipped() {
        assert_eq!(from_top_left((100.0, 0.0), 900.0), (100.0, 900.0));
        assert_eq!(from_top_left((100.0, 850.0), 900.0), (100.0, 50.0));
        // Below the main display (a display arranged underneath it): negative in Cocoa.
        assert_eq!(from_top_left((10.0, 1200.0), 900.0), (10.0, -300.0));
    }

    #[test]
    fn screens_contain_their_points() {
        let main = (0.0, 0.0, 1440.0, 900.0);
        assert!(contains(main, (0.0, 0.0)));
        assert!(contains(main, (1439.0, 899.0)));
        assert!(!contains(main, (1440.0, 10.0)), "the right edge belongs to the next display");
        assert!(!contains(main, (-1.0, 10.0)));
    }
}
