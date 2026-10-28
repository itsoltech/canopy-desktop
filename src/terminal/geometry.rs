//! Pixel geometry shared by painting, pointer mapping and IME.
//! Edge backgrounds extend existing spans; no synthetic terminal cells are created.
use super::session::Size;
/// Cell of the default font (JetBrains Mono at 13 px).
pub const CELL_WIDTH: f32 = 7.83;
pub const CELL_HEIGHT: f32 = 17.;
/// Font size the default cell was designed for.
const DEFAULT_FONT_SIZE: f32 = 13.;
pub const PADDING: f32 = 8.;

/// Pixel size of one terminal cell, derived from the terminal font.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellSize {
    pub width: f32,
    pub height: f32,
}
impl CellSize {
    pub const DEFAULT: Self = Self {
        width: CELL_WIDTH,
        height: CELL_HEIGHT,
    };
    /// The cell of a monospace font at `font_size` px whose glyph advance is
    /// `advance` px. Width is the advance (unknown: the default cell's ratio);
    /// height keeps the default line height ratio, rounded to whole pixels.
    pub fn for_font(font_size: f32, advance: Option<f32>) -> Self {
        let font_size = font_size.max(1.);
        let width = advance
            .filter(|width| width.is_finite() && *width > 0.)
            .unwrap_or(font_size * CELL_WIDTH / DEFAULT_FONT_SIZE);
        Self {
            width,
            height: (font_size * CELL_HEIGHT / DEFAULT_FONT_SIZE)
                .round()
                .max(1.),
        }
    }
}
impl Default for CellSize {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GridGeometry {
    pub width: f32,
    pub height: f32,
    pub left: f32,
    pub top: f32,
    pub size: Size,
    pub cell: CellSize,
}
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl GridGeometry {
    /// The largest grid of `cell`s that fits `width` × `height` inside the padding.
    pub fn fit(width: f32, height: f32, cell: CellSize) -> Self {
        let columns = ((width - 2. * PADDING).max(0.) / cell.width) as usize;
        let rows = ((height - 2. * PADDING).max(0.) / cell.height) as usize;
        Self::with_size(width, height, Size::bounded(columns, rows), cell)
    }
    pub fn with_size(width: f32, height: f32, size: Size, cell: CellSize) -> Self {
        Self {
            width: width.max(0.),
            height: height.max(0.),
            left: ((width - size.columns as f32 * cell.width) / 2.).max(0.),
            top: ((height - size.rows as f32 * cell.height) / 2.).max(0.),
            size,
            cell,
        }
    }
    pub fn cell_at(self, x: f32, y: f32) -> (usize, usize) {
        let column = ((x - self.left) / self.cell.width)
            .floor()
            .clamp(0., (self.size.columns - 1) as f32) as usize;
        let row = ((y - self.top) / self.cell.height)
            .floor()
            .clamp(0., (self.size.rows - 1) as f32) as usize;
        (row, column)
    }
    pub fn background(self, row: usize, column: usize, columns: usize) -> Rect {
        let x = if column == 0 {
            0.
        } else {
            self.left + column as f32 * self.cell.width
        }
        .min(self.width);
        let y = if row == 0 {
            0.
        } else {
            self.top + row as f32 * self.cell.height
        }
        .min(self.height);
        let right = if column + columns >= self.size.columns {
            self.width
        } else {
            self.left + (column + columns) as f32 * self.cell.width
        }
        .min(self.width);
        let bottom = if row + 1 >= self.size.rows {
            self.height
        } else {
            self.top + (row + 1) as f32 * self.cell.height
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
        let original = GridGeometry::fit(713., 487., CellSize::DEFAULT);
        anchor.initialize(original);
        for width in 714..850 {
            let placed = anchor.apply(GridGeometry::fit(width as f32, 501., CellSize::DEFAULT));
            assert_eq!((placed.left, placed.top), (original.left, original.top));
        }
        let final_geometry = GridGeometry::fit(849., 501., CellSize::DEFAULT);
        anchor.settle(final_geometry);
        let placed = anchor.apply(final_geometry);
        assert_eq!(
            (placed.left, placed.top),
            (final_geometry.left, final_geometry.top)
        );
    }
    #[test]
    fn grid_is_centered_and_cell_coordinates_roundtrip() {
        let g = GridGeometry::fit(713., 487., CellSize::DEFAULT);
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
        let g = GridGeometry::fit(713., 487., CellSize::DEFAULT);
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
    fn cell_metrics_follow_the_font_size() {
        // The default size keeps the default line height; others scale it.
        assert_eq!(CellSize::for_font(13., Some(7.8)).height, CELL_HEIGHT);
        assert_eq!(CellSize::for_font(13., Some(7.8)).width, 7.8);
        assert_eq!(CellSize::for_font(24., Some(14.4)).height, 31.);
        assert_eq!(CellSize::for_font(8., Some(4.8)).height, 10.);
        // A font that reports no advance falls back to the default ratio.
        let unknown = CellSize::for_font(26., None);
        assert!((unknown.width - 2. * CELL_WIDTH).abs() < 0.001);
        let nan = CellSize::for_font(13., Some(f32::NAN));
        assert!((nan.width - CELL_WIDTH).abs() < 0.001 && nan.height == CELL_HEIGHT);
        // A larger cell gives a smaller grid in the same pane, still centered.
        let small = GridGeometry::fit(713., 487., CellSize::for_font(10., Some(6.)));
        let large = GridGeometry::fit(713., 487., CellSize::for_font(20., Some(12.)));
        assert!(large.size.columns < small.size.columns && large.size.rows < small.size.rows);
        assert_eq!(large.size.columns, ((713. - 2. * PADDING) / 12.) as usize);
        assert!((large.left - (713. - large.size.columns as f32 * 12.) / 2.).abs() < 0.001);
        assert_eq!(
            large.cell_at(large.left + 2.5 * 12., large.top + 1.5 * large.cell.height),
            (1, 2)
        );
    }
    #[test]
    fn tiny_panes_are_clipped_without_negative_rectangles() {
        let g = GridGeometry::fit(4., 3., CellSize::DEFAULT);
        for col in 0..g.size.columns {
            let r = g.background(0, col, 1);
            assert!(r.width >= 0. && r.height >= 0. && r.x + r.width <= g.width);
        }
    }
}
