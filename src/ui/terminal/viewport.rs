//! Resize/reflow and settling; independent of the render composition.
use super::*;
impl TerminalView {
    pub(super) fn grid_geometry(&self) -> canopy_desktop::terminal::geometry::GridGeometry {
        self.anchor
            .apply(canopy_desktop::terminal::geometry::GridGeometry::with_size(
                f32::from(self.bounds.size.width),
                f32::from(self.bounds.size.height),
                self.frame.as_ref().map(|f| f.size).unwrap_or(self.columns),
            ))
    }

    pub(super) fn resize(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let changed = self.bounds.size != bounds.size;
        self.bounds = bounds;
        let geometry = canopy_desktop::terminal::geometry::GridGeometry::fit(
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
        );
        self.anchor.initialize(geometry);
        if changed {
            self.settle_deadline =
                std::time::Instant::now() + canopy_desktop::motion::duration::QUICK;
            if self.settle_task.is_none() {
                self.settle_task = Some(cx.spawn(async |this, cx| {
                    loop {
                        let Ok(deadline) = this.update(cx, |this, _| this.settle_deadline) else {
                            return;
                        };
                        cx.background_executor()
                            .timer(deadline.saturating_duration_since(std::time::Instant::now()))
                            .await;
                        let done = this
                            .update(cx, |this, cx| {
                                if std::time::Instant::now() < this.settle_deadline {
                                    return false;
                                }
                                let g = canopy_desktop::terminal::geometry::GridGeometry::fit(
                                    f32::from(this.bounds.size.width),
                                    f32::from(this.bounds.size.height),
                                );
                                this.anchor.settle(g);
                                this.settle_task = None;
                                if this.visible {
                                    cx.notify();
                                }
                                true
                            })
                            .unwrap_or(true);
                        if done {
                            break;
                        }
                    }
                }));
            }
        }
        if let Some(session) = self.session.as_ref() {
            if !session.resize(geometry.size) {
                return;
            }
            // Reflow and geometry can be reflected in this frame, without waiting for PTY output.
            if changed && let Some(frame) = session.frame() {
                self.set_resized_frame(frame, cx);
            }
        }
        self.columns = geometry.size;
    }
}
