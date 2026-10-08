//! Pixel geometry shared by painting, pointer mapping and IME.
//! Edge backgrounds extend existing spans; no synthetic terminal cells are created.
use super::session::Size;
pub const CELL_WIDTH: f32 = 7.83;
pub const CELL_HEIGHT: f32 = 17.;
pub const PADDING: f32 = 8.;
#[derive(Clone, Copy, Debug)]
pub struct GridGeometry {
    pub width: f32,
    pub height: f32,
    pub left: f32,
    pub top: f32,
    pub size: Size,
}
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl GridGeometry {
    pub fn fit(width: f32, height: f32) -> Self {
        let columns = ((width - 2. * PADDING).max(0.) / CELL_WIDTH) as usize;
        let rows = ((height - 2. * PADDING).max(0.) / CELL_HEIGHT) as usize;
        Self::with_size(width, height, Size::bounded(columns, rows))
    }
    pub fn with_size(width: f32, height: f32, size: Size) -> Self {
        Self {
            width: width.max(0.),
            height: height.max(0.),
            left: ((width - size.columns as f32 * CELL_WIDTH) / 2.).max(0.),
            top: ((height - size.rows as f32 * CELL_HEIGHT) / 2.).max(0.),
            size,
        }
    }
    pub fn cell_at(self, x: f32, y: f32) -> (usize, usize) {
        let column = ((x - self.left) / CELL_WIDTH)
            .floor()
            .clamp(0., (self.size.columns - 1) as f32) as usize;
        let row = ((y - self.top) / CELL_HEIGHT)
            .floor()
            .clamp(0., (self.size.rows - 1) as f32) as usize;
        (row, column)
    }
    pub fn background(self, row: usize, column: usize, columns: usize) -> Rect {
        let x = if column == 0 {
            0.
        } else {
            self.left + column as f32 * CELL_WIDTH
        }
        .min(self.width);
        let y = if row == 0 {
            0.
        } else {
            self.top + row as f32 * CELL_HEIGHT
        }
        .min(self.height);
        let right = if column + columns >= self.size.columns {
            self.width
        } else {
            self.left + (column + columns) as f32 * CELL_WIDTH
        }
        .min(self.width);
        let bottom = if row + 1 >= self.size.rows {
            self.height
        } else {
            self.top + (row + 1) as f32 * CELL_HEIGHT
        }
        .min(self.height);
        Rect {
            x,
            y,
            width: (right - x).max(0.),
            height: (bottom - y).max(0.),
        }
    }
}
/// Hold the local origin during a live resize, then center once after it settles.
#[derive(Default)]
pub struct GridAnchor {
    origin: Option<(f32, f32)>,
}
impl GridAnchor {
    pub fn initialize(&mut self, g: GridGeometry) {
        if self.origin.is_none() {
            self.settle(g);
        }
    }
    pub fn settle(&mut self, g: GridGeometry) {
        self.origin = Some((g.left, g.top));
    }
    pub fn apply(&self, mut g: GridGeometry) -> GridGeometry {
        if let Some((left, top)) = self.origin {
            g.left = left;
            g.top = top;
        }
        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resize_holds_origin_until_one_final_centering() {
        let mut anchor = GridAnchor::default();
        let original = GridGeometry::fit(713., 487.);
        anchor.initialize(original);
        for width in 714..850 {
            let placed = anchor.apply(GridGeometry::fit(width as f32, 501.));
            assert_eq!((placed.left, placed.top), (original.left, original.top));
        }
        let final_geometry = GridGeometry::fit(849., 501.);
        anchor.settle(final_geometry);
        let placed = anchor.apply(final_geometry);
        assert_eq!(
            (placed.left, placed.top),
            (final_geometry.left, final_geometry.top)
        );
    }
    #[test]
    fn grid_is_centered_and_cell_coordinates_roundtrip() {
        let g = GridGeometry::fit(713., 487.);
        assert!((g.left - (g.width - g.left - g.size.columns as f32 * CELL_WIDTH)).abs() < 0.001);
        assert!((g.top - (g.height - g.top - g.size.rows as f32 * CELL_HEIGHT)).abs() < 0.001);
        assert_eq!(
            g.cell_at(g.left + 3.5 * CELL_WIDTH, g.top + 5.5 * CELL_HEIGHT),
            (5, 3)
        );
        assert_eq!(g.cell_at(-100., -100.), (0, 0));
        assert_eq!(
            g.cell_at(10000., 10000.),
            (g.size.rows - 1, g.size.columns - 1)
        );
    }
    #[test]
    fn edge_rectangles_tile_the_complete_pane_including_corners() {
        let g = GridGeometry::fit(713., 487.);
        let mut area = 0.;
        for row in 0..g.size.rows {
            let left = g.background(row, 0, 4);
            let right = g.background(row, 4, g.size.columns - 4);
            assert!((left.x + left.width - right.x).abs() < 0.001);
            assert!((right.x + right.width - g.width).abs() < 0.001);
            area += left.width * left.height + right.width * right.height;
        }
        assert!((area - g.width * g.height).abs() < 1.);
        assert_eq!(g.background(0, 0, 1).x, 0.);
        assert_eq!(g.background(0, 0, 1).y, 0.);
        let last = g.background(g.size.rows - 1, g.size.columns - 1, 1);
        assert!((last.y + last.height - g.height).abs() < 0.001);
    }
    #[test]
    fn tiny_panes_are_clipped_without_negative_rectangles() {
        let g = GridGeometry::fit(4., 3.);
        for col in 0..g.size.columns {
            let r = g.background(0, col, 1);
            assert!(r.width >= 0. && r.height >= 0. && r.x + r.width <= g.width);
        }
    }
}
