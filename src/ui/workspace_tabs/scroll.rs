use canopy_desktop::{
    motion::{MotionPolicy, Transition, presets},
    state::workspace::{TabId, WorkspaceId},
};
use gpui_kit::{ScrollHandle, point, px};
use std::time::Instant;

pub(in crate::ui) struct TabScroll {
    pub handle: ScrollHandle,
    target: Option<(WorkspaceId, TabId, usize, usize, u32)>,
    motion: Transition,
    running: bool,
}
impl TabScroll {
    pub fn new() -> Self {
        Self {
            handle: ScrollHandle::new(),
            target: None,
            motion: Transition::new(0., Instant::now()),
            running: false,
        }
    }
    pub fn update(
        &mut self,
        workspace: WorkspaceId,
        active: Option<(TabId, usize)>,
        count: usize,
        width: f32,
        now: Instant,
        policy: MotionPolicy,
    ) -> bool {
        let Some((tab, index)) = active else {
            self.target = None;
            self.running = false;
            self.handle.set_offset(point(px(0.), px(0.)));
            return false;
        };
        let target = (workspace, tab, index, count, width.to_bits());
        if self.target != Some(target) {
            self.target = Some(target);
            let current = -f32::from(self.handle.offset().x);
            let destination = reveal_offset(current, index, count, width);
            self.motion = Transition::new(current, now);
            self.motion
                .retarget(destination, presets::RESIZE, now, policy);
            self.running = true;
        }
        if self.running {
            self.handle
                .set_offset(point(px(-self.motion.value(now)), px(0.)));
            self.running = self.motion.is_animating(now);
        }
        self.running
    }
    pub fn interrupt(&mut self) {
        self.running = false;
    }
}
fn reveal_offset(current: f32, index: usize, count: usize, width: f32) -> f32 {
    let width = width.max(0.);
    let tab_width = crate::ui::components::TAB_WIDTH;
    let start = index as f32 * tab_width;
    let end = start + tab_width;
    let maximum = (count as f32 * tab_width + 40. - width).max(0.);
    let current = current.clamp(0., maximum);
    let target = if start < current || width < tab_width {
        start
    } else if end > current + width {
        end - width
    } else {
        current
    };
    target.clamp(0., maximum)
}
#[cfg(test)]
mod tests {
    use super::reveal_offset;
    #[test]
    fn reveals_both_edges_without_moving_visible_tabs() {
        assert_eq!(reveal_offset(0., 9, 10, 720.), 1080.);
        assert_eq!(reveal_offset(1080., 0, 10, 720.), 0.);
        assert_eq!(reveal_offset(180., 2, 10, 720.), 180.);
    }
    #[test]
    fn resize_removal_and_narrow_viewports_stay_bounded() {
        assert_eq!(reveal_offset(1080., 1, 2, 720.), 0.);
        assert_eq!(reveal_offset(0., 3, 4, 360.), 360.);
        assert_eq!(reveal_offset(0., 2, 3, 100.), 360.);
    }
}
