//! Position shaped clusters on the terminal grid, without the shaper's width tolerance.
/// Each byte maps to its owning terminal column. Combining glyphs retain their
/// shaped offset within that cell; ordinary glyphs always start at column * width.
pub fn positions(glyphs: &[(usize, f32)], byte_columns: &[usize], width: f32) -> Vec<f32> {
    let mut anchors = vec![None; byte_columns.iter().copied().max().map_or(0, |n| n + 1)];
    glyphs
        .iter()
        .map(|&(index, x)| {
            let Some(&column) = byte_columns.get(index) else {
                return x;
            };
            let base = *anchors[column].get_or_insert(x);
            column as f32 * width + (x - base)
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn borders_do_not_depend_on_span_length_or_small_font_drift() {
        for column in 1..200 {
            let glyphs: Vec<_> = (0..=column).map(|i| (i, i as f32 * 7.8)).collect();
            let map: Vec<_> = (0..=column).collect();
            let x = positions(&glyphs, &map, 7.83)[column];
            assert_eq!(x, column as f32 * 7.83);
            // A separately colored border span has exactly the same absolute origin.
            assert_eq!(
                x,
                column as f32 * 7.83 + positions(&[(0, 0.)], &[0], 7.83)[0]
            );
        }
    }
    #[test]
    fn combining_marks_keep_offsets_and_utf8_uses_cell_mapping() {
        // UTF-8 bytes for a base + accent belong to column 0; next cell starts at byte 3.
        assert_eq!(
            positions(&[(0, 0.), (1, -2.), (3, 7.8)], &[0, 0, 0, 1], 7.83),
            vec![0., -2., 7.83]
        );
    }
    #[test]
    fn wide_cell_and_fallback_cluster_preserve_internal_positions() {
        assert_eq!(
            positions(&[(0, 0.4), (1, 0.4), (4, 15.6)], &[0, 0, 0, 0, 2], 7.83),
            vec![0., 0., 15.66]
        );
    }
}
