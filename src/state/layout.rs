use serde::{Deserialize, Serialize};
pub const LEFT_MIN_WIDTH: f32 = 180.;
pub const RIGHT_MIN_WIDTH: f32 = 220.;
pub const PANEL_MAX_WIDTH: f32 = 800.;
pub const MAIN_MIN_WIDTH: f32 = 160.;
// User widths change only on separator drags, never when the window resizes.
#[derive(Clone, Copy)]
pub enum PanelSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaneWidths {
    pub left: f32,
    pub right: f32,
}
pub struct Layout {
    /// Expanded content widths after fitting, before the reveal clips them.
    pub panels: PaneWidths,
    pub left: f32,
    pub right: f32,
    pub terminal: f32,
}
impl Default for PaneWidths {
    fn default() -> Self {
        Self {
            left: 220.,
            right: 280.,
        }
    }
}
impl PaneWidths {
    pub fn valid(&self) -> bool {
        self.left.is_finite()
            && self.right.is_finite()
            && (LEFT_MIN_WIDTH..=PANEL_MAX_WIDTH).contains(&self.left)
            && (RIGHT_MIN_WIDTH..=PANEL_MAX_WIDTH).contains(&self.right)
    }
    pub fn resize(&mut self, side: PanelSide, width: f32) {
        if !width.is_finite() {
            return;
        }
        match side {
            PanelSide::Left => self.left = width.clamp(LEFT_MIN_WIDTH, PANEL_MAX_WIDTH),
            PanelSide::Right => self.right = width.clamp(RIGHT_MIN_WIDTH, PANEL_MAX_WIDTH),
        }
    }
    pub fn layout(&self, window: f32, left_progress: f32, right_progress: f32) -> Layout {
        let mut left = self.left * left_progress;
        let mut right = self.right * right_progress;
        let budget = (window - MAIN_MIN_WIDTH).max(0.);
        if left + right > budget {
            // Preserve requested widths whenever they fit. Only distribute the
            // excess panel space when the viewport cannot accommodate them.
            let left_min = LEFT_MIN_WIDTH * left_progress;
            let right_min = RIGHT_MIN_WIDTH * right_progress;
            let minimum = left_min + right_min;
            if budget >= minimum {
                let extra = left + right - minimum;
                let share = (budget - minimum) / extra;
                left = left_min + (left - left_min) * share;
            } else {
                // Defensive fallback for a viewport below the native minimum.
                left = budget * left_min / minimum;
            }
            right = budget - left;
        }
        Layout {
            panels: PaneWidths {
                left: if left_progress > 0. {
                    left / left_progress
                } else {
                    self.left
                },
                right: if right_progress > 0. {
                    right / right_progress
                } else {
                    self.right
                },
            },
            left,
            right,
            terminal: (window - (left + right)).max(0.),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutState {
    pub widths: PaneWidths,
    pub sidebar_open: bool,
    pub inspector_open: bool,
    pub inspector_changes: bool,
    #[serde(default)]
    pub inspector_tasks: bool,
}
impl Default for LayoutState {
    fn default() -> Self {
        Self {
            widths: PaneWidths::default(),
            sidebar_open: true,
            inspector_open: true,
            inspector_changes: false,
            inspector_tasks: false,
        }
    }
}

impl LayoutState {
    pub fn resize(&mut self, side: PanelSide, width: f32, window: f32) {
        if !width.is_finite() || !window.is_finite() {
            return;
        }
        let current = self.widths.layout(
            window,
            if self.sidebar_open { 1. } else { 0. },
            if self.inspector_open { 1. } else { 0. },
        );
        // A drag is new user intent: keep the other visible panel at its actual
        // width so the dragged separator follows the pointer, even after a fit.
        let other = match side {
            PanelSide::Left => {
                if self.inspector_open {
                    self.widths.right = current.panels.right;
                }
                current.right
            }
            PanelSide::Right => {
                if self.sidebar_open {
                    self.widths.left = current.panels.left;
                }
                current.left
            }
        };
        self.widths
            .resize(side, width.min((window - MAIN_MIN_WIDTH - other).max(0.)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn window_resize_only_changes_terminal() {
        let widths = PaneWidths::default();
        let big = widths.layout(1400., 1., 1.);
        let small = widths.layout(900., 1., 1.);
        assert_eq!(big.left, small.left);
        assert_eq!(big.right, small.right);
        assert_eq!(big.terminal - small.terminal, 500.);
    }
    #[test]
    fn toggle_does_not_erase_user_widths() {
        let mut widths = PaneWidths::default();
        widths.resize(PanelSide::Right, 360.);
        assert_eq!(widths.layout(1000., 0., 0.).terminal, 1000.);
        assert_eq!(widths.layout(1000., 1., 1.).right, 360.);
        widths.resize(PanelSide::Left, 400.);
        assert!(widths.layout(800., 1., 1.).terminal >= 160.);
    }
    #[test]
    fn wide_panels_fit_small_windows_without_changing_preferences() {
        let widths = PaneWidths {
            left: 800.,
            right: 800.,
        };
        let wide = widths.layout(1920., 1., 1.);
        assert_eq!(wide.panels, widths);
        let small = widths.layout(800., 1., 1.);
        assert!(small.left >= LEFT_MIN_WIDTH && small.right >= RIGHT_MIN_WIDTH);
        assert!((small.terminal - MAIN_MIN_WIDTH).abs() < 0.01);
        assert!((small.left + small.right + small.terminal - 800.).abs() < 0.01);
        assert_eq!(widths.layout(1920., 1., 1.).panels, widths);
        assert_eq!(widths.layout(1200., 1., 0.).left, 800.);
        assert_eq!(widths.layout(1200., 0., 1.).right, 800.);
    }
    #[test]
    fn reveal_uses_available_space_and_keeps_content_inside_each_panel() {
        let widths = PaneWidths {
            left: 800.,
            right: 800.,
        };
        for step in 0..=100 {
            let progress = step as f32 / 100.;
            for (left, right) in [(1., progress), (progress, 1.), (progress, progress)] {
                let layout = widths.layout(800., left, right);
                assert!(layout.terminal >= MAIN_MIN_WIDTH - 0.01);
                assert!((layout.panels.left * left - layout.left).abs() < 0.01);
                assert!((layout.panels.right * right - layout.right).abs() < 0.01);
            }
        }
    }
    #[test]
    fn dragging_respects_the_current_window_and_only_visible_panels() {
        let mut state = LayoutState::default();
        state.resize(PanelSide::Left, 1200., 1920.);
        state.resize(PanelSide::Right, 1200., 1920.);
        assert_eq!(
            state.widths,
            PaneWidths {
                left: 800.,
                right: 800.
            }
        );

        let shown = state.widths.layout(1000., 1., 1.);
        state.resize(PanelSide::Left, shown.left - 40., 1000.);
        assert!((state.widths.left - (shown.left - 40.)).abs() < 0.01);
        assert_eq!(state.widths.right, shown.right);

        state.inspector_open = false;
        state.resize(PanelSide::Left, 800., 1200.);
        assert_eq!(state.widths.left, 800.);
        assert_eq!(state.widths.right, shown.right);
        state.inspector_open = true;
        state.resize(PanelSide::Left, 800., 1200.);
        assert!((state.widths.layout(1200., 1., 1.).terminal - MAIN_MIN_WIDTH).abs() < 0.01);
        assert!(state.widths.valid());
    }
}
