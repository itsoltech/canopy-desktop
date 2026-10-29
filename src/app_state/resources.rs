//! Owner of the opt-in CPU and memory sampler (`perf.hud.enabled`). Sampling runs
//! once per second on the background executor and only while the setting is on;
//! views read the latest sample.
use super::SettingsState;
use canopy_desktop::platform::resources::{Sample, Sampler};
use gpui_kit::*;
use std::time::Duration;

const INTERVAL: Duration = Duration::from_secs(1);

pub struct ResourceUsage {
    /// `None` while off and until the first full interval was measured.
    pub sample: Option<Sample>,
    sampling: Option<Task<()>>,
    _settings: Option<Subscription>,
}

impl ResourceUsage {
    pub fn new() -> Self {
        Self {
            sample: None,
            sampling: None,
            _settings: None,
        }
    }

    /// Follows the setting from now on.
    pub fn bind(&mut self, settings: &Entity<SettingsState>, cx: &mut Context<Self>) {
        self.set_enabled(settings.read(cx).values.resource_usage, cx);
        self._settings = Some(cx.observe(settings, |this, settings, cx| {
            this.set_enabled(settings.read(cx).values.resource_usage, cx)
        }));
    }

    pub fn enabled(&self) -> bool {
        self.sampling.is_some()
    }

    fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if enabled == self.enabled() {
            return;
        }
        self.sample = None;
        // Dropping the task stops the loop; an in-flight measurement is discarded.
        self.sampling = enabled.then(|| Self::sample_every_second(cx));
        cx.notify();
    }

    fn sample_every_second(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let mut sampler = Sampler::new();
            // The first measurement only records process times to diff against.
            let mut first = true;
            loop {
                let (returned, sample) = cx
                    .background_executor()
                    .spawn(async move {
                        let sample = sampler.sample();
                        (sampler, sample)
                    })
                    .await;
                sampler = returned;
                if !first
                    && this
                        .update(cx, |this, cx| {
                            this.sample = Some(sample);
                            cx.notify();
                        })
                        .is_err()
                {
                    return;
                }
                first = false;
                cx.background_executor().timer(INTERVAL).await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::app_state::AppState;
    use canopy_desktop::state::workspace::Workspace;
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    fn samples_only_while_the_setting_is_on(cx: &mut TestAppContext) {
        let app = cx.update(|cx| AppState::install_test(Workspace::new(), cx));
        let resources = app.resources.clone();
        assert!(!resources.read_with(cx, |r, _| r.enabled()));

        app.settings.update(cx, |settings, cx| {
            settings.values.resource_usage = true;
            cx.notify();
        });
        assert!(resources.read_with(cx, |r, _| r.enabled()));

        app.settings.update(cx, |settings, cx| {
            settings.values.resource_usage = false;
            cx.notify();
        });
        resources.read_with(cx, |r, _| {
            assert!(!r.enabled());
            assert!(r.sample.is_none());
        });
    }
}
