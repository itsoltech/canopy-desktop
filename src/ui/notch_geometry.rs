//! Pure input geometry shared by native notch adapters.
use gpui_kit::{Pixels, Point};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeEvent {
    Pointer(bool),
    #[cfg(target_os = "windows")]
    Failed(&'static str),
    #[cfg(target_os = "windows")]
    Recovered,
}

/// Visible island in the fixed native canvas, including rounded bottom corners.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputRegion {
    pub canvas_width: f32,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
}

impl InputRegion {
    pub fn contains(self, position: Point<Pixels>) -> bool {
        self.contains_xy(f32::from(position.x), f32::from(position.y))
    }

    pub fn contains_xy(self, canvas_x: f32, canvas_y: f32) -> bool {
        let x = canvas_x - (self.canvas_width - self.width) / 2.;
        let y = canvas_y;
        if x < 0. || x >= self.width || y < 0. || y >= self.height {
            return false;
        }
        let radius = self.radius.min(self.width / 2.).min(self.height);
        if y < self.height - radius || (x >= radius && x <= self.width - radius) {
            return true;
        }
        let center_x = if x < radius {
            radius
        } else {
            self.width - radius
        };
        (x - center_x).powi(2) + (y - (self.height - radius)).powi(2) <= radius.powi(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, px};

    #[test]
    fn transparent_canvas_and_corners_do_not_receive_clicks() {
        let region = InputRegion {
            canvas_width: 480.,
            width: 295.,
            height: 39.,
            radius: 16.,
        };
        assert!(region.contains(point(px(112.), px(19.))));
        assert!(!region.contains(point(px(200.), px(60.))));
        assert!(!region.contains(point(px(20.), px(19.))));
        assert!(!region.contains(point(px(94.), px(38.))));
    }

    #[test]
    fn empty_region_never_contains_pointer() {
        let region = InputRegion {
            canvas_width: 480.,
            ..Default::default()
        };
        assert!(!region.contains_xy(240., 0.));
    }
}
