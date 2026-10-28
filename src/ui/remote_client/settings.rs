//! Host-bound feature owners. Views are created by navigation, never by render.
use super::*;
use canopy_desktop::remote::client::rpc_error;
mod controls;
mod device;
mod editor;
#[path = "tasks/mod.rs"]
mod tasks;
mod view;
use tasks::TasksView;
use view::SettingsView;
#[derive(Default)]
pub(super) struct FeatureViews {
    settings: Option<Entity<SettingsView>>,
    tasks: Option<Entity<TasksView>>,
    device: Option<Entity<device::DevicePreferences>>,
    subscriptions: Vec<Subscription>,
}
impl FeatureViews {
    pub(super) fn apply_device_preferences(&self, window: &mut Window, cx: &App) {
        if let Some(device) = &self.device {
            device.read(cx).apply(window, cx);
        }
    }
}
impl RemoteWindow {
    pub(super) fn load_settings(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        if let Some(view) = &self.features.settings {
            view.update(cx, |v, cx| v.refresh(cx));
            return;
        }
        let runtime = self.global.runtime.clone();
        let device = self
            .features
            .device
            .get_or_insert_with(|| {
                cx.new(|_| device::DevicePreferences::new(host.clone(), runtime.clone()))
            })
            .clone();
        let view = self.window.update(cx, |_, window, cx| {
            cx.new(|cx| SettingsView::new(host, runtime, device, window, cx))
        });
        match view {
            Ok(view) => self.features.settings = Some(view),
            Err(e) => self.error = Some(e.to_string()),
        }
        cx.notify();
    }
    pub(super) fn load_tasks(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        if let Some(view) = &self.features.tasks {
            view.update(cx, |v, cx| v.set_workspace(self.workspace.clone(), cx));
            return;
        }
        let runtime = self.global.runtime.clone();
        let workspace = self.workspace.clone();
        self.features.device.get_or_insert_with(|| {
            cx.new(|_| device::DevicePreferences::new(host.clone(), runtime.clone()))
        });
        let view = self.window.update(cx, |_, window, cx| {
            cx.new(|cx| TasksView::new(host, runtime, workspace, window, cx))
        });
        match view {
            Ok(view) => {
                self.features.subscriptions.push(cx.subscribe(
                    &view,
                    |s, _, event: &tasks::OpenWorkspace, cx| {
                        s.workspace = Some(event.0.clone());
                        s.navigate(Page::Files, cx);
                        s.load_directory(cx);
                    },
                ));
                self.features.tasks = Some(view);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        cx.notify();
    }
    pub(super) fn render_settings(&mut self, _: &mut Context<Self>) -> AnyElement {
        self.features
            .settings
            .as_ref()
            .map(|v| v.clone().into_any_element())
            .unwrap_or_else(|| {
                chrome::empty(
                    "Settings not loaded",
                    "Connect to this computer to view and change its settings.",
                )
                .into_any_element()
            })
    }
    pub(super) fn render_tasks(&mut self, _: &mut Context<Self>) -> AnyElement {
        self.features
            .tasks
            .as_ref()
            .map(|v| v.clone().into_any_element())
            .unwrap_or_else(|| {
                chrome::empty(
                    "Tasks not loaded",
                    "Connect to this computer to browse its tasks.",
                )
                .into_any_element()
            })
    }
}
