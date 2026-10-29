//! Where the main window was on screen, restored at the next launch
//! (Electron: `windowBounds.ts`). Kept in its own file so it applies even when
//! Canopy does not reopen the last workspace.
use serde::{Deserialize, Serialize};
use std::path::Path;

const FILE: &str = "window.json";
/// A restored window keeps at least this much of itself on its display.
const MIN_VISIBLE: f32 = 100.;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowState {
    #[default]
    Normal,
    Maximized,
    Fullscreen,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowGeometry {
    /// Stable display identifier; `bounds` are relative to that display's top-left.
    pub display: Option<String>,
    /// The windowed frame, also the restore frame of a maximized or full-screen window.
    pub bounds: Rect,
    pub state: WindowState,
}

impl WindowGeometry {
    /// A missing, unreadable or malformed file means the default placement.
    pub fn load(dir: &Path) -> Option<Self> {
        let bytes = std::fs::read(dir.join(FILE)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Replaces the file atomically; a crash leaves the previous geometry.
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let mut file = tempfile::NamedTempFile::new_in(dir)?;
        serde_json::to_writer(&mut file, self)?;
        file.as_file().sync_all()?;
        file.persist(dir.join(FILE)).map_err(|error| error.error)?;
        Ok(())
    }

    /// The saved size on a display of `display` size: no larger than the display,
    /// no smaller than `min`.
    pub fn size_on(&self, display: (f32, f32), min: (f32, f32)) -> Option<(f32, f32)> {
        let Rect { width, height, .. } = self.bounds;
        [width, height, display.0, display.1]
            .iter()
            .all(|value| value.is_finite())
            .then(|| {
                (
                    width.max(min.0).min(display.0),
                    height.max(min.1).min(display.1),
                )
            })
    }

    /// The saved frame sized by [`Self::size_on`], title bar on screen. `None` when
    /// too little of it would be visible, e.g. after the display layout changed.
    pub fn fit(&self, display: (f32, f32), min: (f32, f32)) -> Option<Rect> {
        let (width, height) = self.size_on(display, min)?;
        let Rect { x, y, .. } = self.bounds;
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let y = y.max(0.);
        let visible_x = (x + width).min(display.0) - x.max(0.);
        let visible_y = (y + height).min(display.1) - y;
        (visible_x >= MIN_VISIBLE && visible_y >= MIN_VISIBLE).then_some(Rect {
            x,
            y,
            width,
            height,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(x: f32, y: f32, width: f32, height: f32) -> WindowGeometry {
        WindowGeometry {
            display: Some("display".into()),
            bounds: Rect {
                x,
                y,
                width,
                height,
            },
            state: WindowState::Normal,
        }
    }

    #[test]
    fn a_frame_that_left_the_display_layout_falls_back_and_an_oversized_one_shrinks() {
        let display = (1512., 982.);
        let min = (800., 500.);
        // Saved on a larger display: shrinks to this one, keeps its position.
        assert_eq!(
            geometry(40., 30., 2560., 1440.).fit(display, min),
            Some(Rect {
                x: 40.,
                y: 30.,
                width: 1512.,
                height: 982.
            })
        );
        // Mostly off the right edge, or above the top: back to the default placement
        // and the title bar never sits above the screen.
        assert_eq!(geometry(1450., 30., 900., 600.).fit(display, min), None);
        assert_eq!(
            geometry(100., -80., 900., 600.)
                .fit(display, min)
                .unwrap()
                .y,
            0.
        );
        assert_eq!(geometry(f32::NAN, 0., 900., 600.).fit(display, min), None);
    }

    #[test]
    fn geometry_survives_a_save_and_a_damaged_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let mut saved = geometry(12., 34., 1200., 800.);
        saved.state = WindowState::Fullscreen;
        saved.save(dir.path()).unwrap();
        assert_eq!(WindowGeometry::load(dir.path()), Some(saved));
        std::fs::write(dir.path().join(FILE), b"{").unwrap();
        assert_eq!(WindowGeometry::load(dir.path()), None);
    }
}
