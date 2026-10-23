use canopy_desktop::{
    motion::{MotionPolicy, Transition, presets},
    state::workspace::{TabId, WorkspaceId},
};
use gpui_kit::{ScrollHandle, point, px};
use std::{
    hash::{Hash, Hasher},
    time::Instant,
};

/// The same measured widths drive rendering and reveal, including room for edge hints.
pub(super) struct TabMetrics {
    pub widths: Vec<f32>,
    pub viewport: f32,
    pub edge: f32,
}
impl TabMetrics {
    fn fingerprint(&self) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.viewport.to_bits().hash(&mut hash);
        self.edge.to_bits().hash(&mut hash);
        for width in &self.widths {
            width.to_bits().hash(&mut hash);
        }
        hash.finish()
    }
    fn reveal(&self, current: f32, index: usize) -> f32 {
        let Some(width) = self.widths.get(index) else {
            return 0.;
        };
        let start: f32 = self.widths[..index].iter().sum();
        let end = start + width;
        let maximum = (self.widths.iter().sum::<f32>() + self.edge * 2. - self.viewport).max(0.);
        let current = current.clamp(0., maximum);
        let target = if start < current + self.edge {
            start - self.edge
        } else if end > current + self.viewport - self.edge {
            end - self.viewport + self.edge
        } else {
            current
        };
        target.clamp(0., maximum)
    }
}

pub(in crate::ui) struct TabScroll {
    pub handle: ScrollHandle,
    target: Option<(WorkspaceId, TabId, usize, u64)>,
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
    pub(super) fn update(
        &mut self,
        workspace: WorkspaceId,
        active: Option<(TabId, usize)>,
        metrics: &TabMetrics,
        now: Instant,
        policy: MotionPolicy,
    ) -> bool {
        let Some((tab, index)) = active else {
            self.target = None;
            self.running = false;
            self.handle.set_offset(point(px(0.), px(0.)));
            return false;
        };
        let target = (workspace, tab, index, metrics.fingerprint());
        if self.target != Some(target) {
            self.target = Some(target);
            let current = -f32::from(self.handle.offset().x);
            self.motion = Transition::new(current, now);
            self.motion
                .retarget(metrics.reveal(current, index), presets::RESIZE, now, policy);
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
#[cfg(test)]
mod tests {
    use super::TabMetrics;
    #[test]
    fn reveals_whole_active_tab_and_close_control_outside_edge_hints() {
        let metrics = TabMetrics {
            widths: vec![260., 112., 112., 180.],
            viewport: 280.,
            edge: 20.,
        };
        assert_eq!(metrics.reveal(0., 2), 224.);
        assert_eq!(metrics.reveal(224., 0), 0.);
        assert_eq!(metrics.reveal(224., 2), 224.);
        let metrics = TabMetrics {
            widths: vec![112., 112.],
            viewport: 500.,
            edge: 20.,
        };
        assert_eq!(metrics.reveal(224., 1), 0.);
    }
}
