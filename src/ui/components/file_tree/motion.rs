//! Stable-order animated rows; only rows intersecting the viewport are rendered.
use super::super::super::theme::ROW;
use super::VisibleRow;
use canopy_desktop::motion::{self, MotionPolicy, Transition};
use gpui_kit::SharedString;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    time::Instant,
};

pub(super) struct AnimatedRow {
    pub entry: VisibleRow,
    from_rotation: f32,
    to_rotation: f32,
    from_y: f32,
    to_y: f32,
    from_alpha: f32,
    to_alpha: f32,
    pub interactive: bool,
}
pub(super) struct TreeMotion {
    pub rows: Vec<AnimatedRow>,
    movement: Transition,
    fade: Transition,
    from_height: f32,
    to_height: f32,
    removed: bool,
}
impl TreeMotion {
    pub fn new(rows: &[VisibleRow], now: Instant) -> Self {
        Self {
            rows: rows
                .iter()
                .enumerate()
                .map(|(i, r)| AnimatedRow {
                    entry: r.clone(),
                    from_rotation: if r.expanded { 1. } else { 0. },
                    to_rotation: if r.expanded { 1. } else { 0. },
                    from_y: i as f32 * ROW,
                    to_y: i as f32 * ROW,
                    from_alpha: 1.,
                    to_alpha: 1.,
                    interactive: true,
                })
                .collect(),
            movement: Transition::new(1., now),
            fade: Transition::new(1., now),
            from_height: rows.len() as f32 * ROW,
            to_height: rows.len() as f32 * ROW,
            removed: false,
        }
    }
    pub fn height(&self, now: Instant) -> f32 {
        mix(self.from_height, self.to_height, self.movement.value(now))
    }
    pub fn active(&self, now: Instant) -> bool {
        self.movement.is_animating(now) || self.fade.is_animating(now)
    }
    pub fn y(&self, i: usize, now: Instant) -> f32 {
        let r = &self.rows[i];
        mix(r.from_y, r.to_y, self.movement.value(now))
    }
    pub fn rotation(&self, i: usize, now: Instant) -> f32 {
        let r = &self.rows[i];
        mix(r.from_rotation, r.to_rotation, self.movement.value(now))
    }
    pub fn alpha(&self, i: usize, now: Instant) -> f32 {
        let r = &self.rows[i];
        mix(r.from_alpha, r.to_alpha, self.fade.value(now))
    }
    pub fn retarget(
        &mut self,
        visible: &[VisibleRow],
        opening: bool,
        now: Instant,
        policy: MotionPolicy,
    ) {
        let old_height = self.height(now);
        let old: HashMap<SharedString, (f32, f32, f32)> = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                (
                    r.entry.id.clone(),
                    (self.y(i, now), self.alpha(i, now), self.rotation(i, now)),
                )
            })
            .collect();
        let target: HashMap<SharedString, f32> = visible
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id.clone(), i as f32 * ROW))
            .collect();
        let target_ids = visible.iter().map(|r| r.id.clone()).collect::<HashSet<_>>();
        let old_ids = self
            .rows
            .iter()
            .map(|r| r.entry.id.clone())
            .collect::<HashSet<_>>();
        let common_old = self
            .rows
            .iter()
            .filter(|r| target_ids.contains(&r.entry.id))
            .map(|r| &r.entry.id)
            .collect::<Vec<_>>();
        let common_new = visible
            .iter()
            .filter(|r| old_ids.contains(&r.id))
            .map(|r| &r.id)
            .collect::<Vec<_>>();
        // Crossed rows break viewport binary search; file reorderings settle immediately.
        if common_old != common_new {
            *self = Self::new(visible, now);
            return;
        }
        let positions = visible
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id.clone(), i))
            .collect::<HashMap<_, _>>();
        let mut removed = BTreeMap::<usize, Vec<VisibleRow>>::new();
        let mut anchor = visible.len();
        for row in self.rows.iter().rev() {
            if let Some(i) = positions.get(&row.entry.id) {
                anchor = *i;
            } else {
                removed.entry(anchor).or_default().push(row.entry.clone());
            }
        }
        let mut union = Vec::new();
        for (i, row) in visible.iter().enumerate() {
            if let Some(rows) = removed.remove(&i) {
                union.extend(rows.into_iter().rev());
            }
            union.push(row.clone());
        }
        if let Some(rows) = removed.remove(&visible.len()) {
            union.extend(rows.into_iter().rev());
        }
        let target_height = visible.len() as f32 * ROW;
        let (mut next_old, mut next_new) = (old_height, target_height);
        let mut rows = Vec::with_capacity(union.len());
        for entry in union.into_iter().rev() {
            let (from_y, from_alpha, from_rotation) =
                old.get(&entry.id).copied().unwrap_or((next_old, 0., 0.));
            let to_y = target.get(&entry.id).copied().unwrap_or(next_new);
            let interactive = target.contains_key(&entry.id);
            next_old = from_y;
            next_new = to_y;
            rows.push(AnimatedRow {
                to_rotation: if entry.expanded { 1. } else { 0. },
                from_rotation,
                entry,
                from_y,
                to_y,
                from_alpha,
                to_alpha: if interactive { 1. } else { 0. },
                interactive,
            });
        }
        rows.reverse();
        self.rows = rows;
        self.from_height = old_height;
        self.to_height = target_height;
        self.movement = Transition::new(0., now);
        self.fade = Transition::new(0., now);
        self.movement.retarget(
            1.,
            if opening {
                motion::presets::PANEL.enter
            } else {
                motion::presets::PANEL.exit
            },
            now,
            policy,
        );
        self.fade.retarget(
            1.,
            if opening {
                motion::presets::CONTENT_REVEAL.enter
            } else {
                motion::presets::CONTENT_REVEAL.exit
            },
            now,
            policy,
        );
        self.removed = false;
    }
    pub fn update_entries(&mut self, visible: &[VisibleRow]) {
        let by_id = visible
            .iter()
            .map(|r| (&r.id, r))
            .collect::<HashMap<_, _>>();
        for row in &mut self.rows {
            if let Some(entry) = by_id.get(&row.entry.id) {
                row.entry = (*entry).clone();
            }
        }
    }
    pub fn retire_exiting(&mut self, now: Instant) {
        if !self.removed && !self.fade.is_animating(now) {
            self.rows.retain(|r| r.interactive);
            self.removed = true;
        }
    }
    pub fn range(&self, top: f32, height: f32, now: Instant) -> std::ops::Range<usize> {
        // GPUI clamps ScrollHandle during prepaint, after this range is selected.
        // Use the same bounded offset so a wheel impulse beyond either edge
        // cannot cull rows that prepaint will place back in the viewport.
        let height = height.max(0.);
        let top = top.clamp(0., (self.height(now) - height).max(0.));
        let progress = self.movement.value(now);
        let y = |r: &AnimatedRow| mix(r.from_y, r.to_y, progress);
        let start = self.rows.partition_point(|r| y(r) + ROW < top);
        let end = self.rows.partition_point(|r| y(r) < top + height);
        start.min(end)..end
    }
}
fn mix(a: f32, b: f32, p: f32) -> f32 {
    a + (b - a) * p
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn rows() -> Vec<VisibleRow> {
        ["src", "src/a", "tail"]
            .iter()
            .enumerate()
            .map(|(i, id)| VisibleRow {
                expanded: i == 0,
                id: (*id).into(),
                label: (*id).into(),
                depth: 0,
                folder: i == 0,
                git_status: None,
                ignored: false,
                loading: false,
                error: None,
            })
            .collect()
    }
    #[test]
    fn collapse_keeps_exiting_rows_but_disables_them_and_removes_after_fade() {
        let now = Instant::now();
        let all = rows();
        let mut m = TreeMotion::new(&all, now);
        m.retarget(
            &[all[0].clone(), all[2].clone()],
            false,
            now,
            MotionPolicy::Full,
        );
        assert_eq!(m.rows.len(), 3);
        assert!(!m.rows[1].interactive);
        assert_eq!(m.height(now), 84.);
        m.retire_exiting(now + Duration::from_millis(150));
        assert_eq!(m.rows.len(), 2);
        assert_eq!(m.height(now + Duration::from_millis(350)), 56.);
    }
    #[test]
    fn rapid_reversal_preserves_positions_and_height() {
        let now = Instant::now();
        let all = rows();
        let mut m = TreeMotion::new(&all, now);
        m.retarget(
            &[all[0].clone(), all[2].clone()],
            false,
            now,
            MotionPolicy::Full,
        );
        let middle = now + Duration::from_millis(80);
        let y = m.y(2, middle);
        let height = m.height(middle);
        m.retarget(&all, true, middle, MotionPolicy::Full);
        assert_eq!(m.y(2, middle), y);
        assert_eq!(m.height(middle), height);
    }
    #[test]
    fn viewport_limits_settled_rows() {
        let now = Instant::now();
        let all: Vec<_> = (0..10_000)
            .map(|i| VisibleRow {
                expanded: false,
                id: format!("f{i}").into(),
                label: "file".into(),
                depth: 0,
                folder: false,
                git_status: None,
                ignored: false,
                loading: false,
                error: None,
            })
            .collect();
        let m = TreeMotion::new(&all, now);
        assert!(m.range(14000., 364., now).len() <= 15);
    }
    #[test]
    fn overscroll_does_not_cull_rows_when_content_fits() {
        let now = Instant::now();
        let m = TreeMotion::new(&rows(), now);
        assert_eq!(m.range(-500., 84., now), 0..3);
        assert_eq!(m.range(500., 84., now), 0..3);
    }
    #[test]
    fn overscroll_clamps_to_each_edge_for_long_content() {
        let now = Instant::now();
        let m = TreeMotion::new(&rows(), now);
        assert_eq!(m.range(-500., 28., now), m.range(0., 28., now));
        assert_eq!(m.range(500., 28., now), m.range(56., 28., now));
    }
    #[test]
    fn asynchronous_children_do_not_replace_sibling_rows() {
        let now = Instant::now();
        let all = rows();
        let initial = vec![all[0].clone(), all[2].clone()];
        let mut m = TreeMotion::new(&initial, now);
        m.retarget(&all, true, now, MotionPolicy::Full);
        assert_eq!(
            m.rows
                .iter()
                .map(|r| r.entry.id.as_ref())
                .collect::<Vec<_>>(),
            vec!["src", "src/a", "tail"]
        );
        assert_eq!(m.y(2, now), ROW);
        assert_eq!(m.y(2, now + Duration::from_secs(1)), 2. * ROW);
        assert_eq!(m.height(now + Duration::from_secs(1)), 3. * ROW);
    }
    #[test]
    fn metadata_updates_do_not_restart_an_expansion() {
        let now = Instant::now();
        let all = rows();
        let mut m = TreeMotion::new(&[all[0].clone(), all[2].clone()], now);
        m.retarget(&all, true, now, MotionPolicy::Full);
        let at = now + Duration::from_millis(80);
        let height = m.height(at);
        let mut updated = all.clone();
        updated[1].ignored = true;
        updated[1].git_status = Some('M');
        m.update_entries(&updated);
        assert_eq!(m.height(at), height);
        assert!(m.rows[1].entry.ignored);
        assert_eq!(m.rows[1].entry.git_status, Some('M'));
        assert!(!m.active(now + Duration::from_secs(1)));
    }
}
