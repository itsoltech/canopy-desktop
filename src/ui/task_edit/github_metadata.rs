use super::picker::{OptionChanged, OptionPicker};
use crate::{
    app_state::{AppState, WriteFinished},
    ui::{components::integrations::integration_message, components::*},
};
use canopy_desktop::integrations::{OptionKind, TaskItem, TaskWrite};
use gpui_kit::*;
pub struct MetadataEditor {
    task: TaskItem,
    labels: Entity<OptionPicker>,
    assignees: Entity<OptionPicker>,
    milestone: Entity<OptionPicker>,
    enabled: bool,
    pending: Option<u64>,
    error: Option<String>,
    _events: Vec<Subscription>,
}
impl MetadataEditor {
    pub fn new(task: TaskItem, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let project = task.reference.project.clone();
        let labels = cx.new(|cx| OptionPicker::new(project.clone(), OptionKind::Label, window, cx));
        let assignees =
            cx.new(|cx| OptionPicker::new(project.clone(), OptionKind::Assignee, window, cx));
        let milestone = cx.new(|cx| OptionPicker::new(project, OptionKind::Milestone, window, cx));
        let state = cx.global::<AppState>().integrations.clone();
        let events = vec![
            cx.subscribe(&labels, |this, _, event: &OptionChanged, cx| {
                this.change(
                    TaskWrite::Label {
                        task: this.task.reference.clone(),
                        label: event.value.clone(),
                        add: event.selected,
                    },
                    cx,
                )
            }),
            cx.subscribe(&assignees, |this, _, event: &OptionChanged, cx| {
                this.change(
                    TaskWrite::Assignee {
                        task: this.task.reference.clone(),
                        login: event.value.clone(),
                        add: event.selected,
                    },
                    cx,
                )
            }),
            cx.subscribe(&milestone, |this, _, event: &OptionChanged, cx| {
                this.change(
                    TaskWrite::Milestone {
                        task: this.task.reference.clone(),
                        number: if event.selected {
                            event.value.parse().ok()
                        } else {
                            None
                        },
                    },
                    cx,
                )
            }),
            cx.observe(&state, |this, _, cx| this.sync_enabled(cx)),
            cx.subscribe(&state, |this, _, event: &WriteFinished, cx| {
                if this.pending != Some(event.id) {
                    return;
                }
                this.pending = None;
                match &event.result {
                    Ok(receipt) => {
                        this.error = receipt.notice.clone();
                        if let Some(task) = &receipt.task {
                            this.task = task.clone();
                        }
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                this.sync_values(cx);
                this.sync_enabled(cx);
                cx.notify();
            }),
        ];
        let mut this = Self {
            task,
            labels,
            assignees,
            milestone,
            enabled: false,
            pending: None,
            error: None,
            _events: events,
        };
        this.sync_values(cx);
        this.sync_enabled(cx);
        this
    }
    pub fn set_task(&mut self, task: TaskItem, cx: &mut Context<Self>) {
        self.task = task;
        if self.pending.is_none() {
            self.sync_values(cx);
        }
        cx.notify();
    }
    pub fn enable(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        self.sync_enabled(cx);
    }
    fn sync_enabled(&mut self, cx: &mut Context<Self>) {
        let enabled = self.enabled
            && self.pending.is_none()
            && !cx.global::<AppState>().integrations.read(cx).busy;
        for picker in [&self.labels, &self.assignees, &self.milestone] {
            picker.update(cx, |s, cx| s.enable(enabled, cx));
        }
        cx.notify();
    }
    fn sync_values(&mut self, cx: &mut Context<Self>) {
        for label in &self.task.labels {
            self.labels.update(cx, |picker, cx| {
                picker.seed_current(
                    canopy_desktop::integrations::TaskOption {
                        value: label.clone(),
                        label: label.clone(),
                    },
                    cx,
                );
            });
        }
        for login in &self.task.assignees {
            self.assignees.update(cx, |picker, cx| {
                picker.seed_current(
                    canopy_desktop::integrations::TaskOption {
                        value: login.clone(),
                        label: format!("@{login}"),
                    },
                    cx,
                );
            });
        }
        if let Some((number, title)) = &self.task.milestone {
            self.milestone.update(cx, |s, cx| {
                s.seed_current(
                    canopy_desktop::integrations::TaskOption {
                        value: number.to_string(),
                        label: title.clone(),
                    },
                    cx,
                )
            });
        }

        self.labels
            .update(cx, |s, cx| s.set_selected(self.task.labels.clone(), cx));
        self.assignees
            .update(cx, |s, cx| s.set_selected(self.task.assignees.clone(), cx));
        self.milestone.update(cx, |s, cx| {
            s.set_selected(
                self.task
                    .milestone
                    .as_ref()
                    .map(|m| vec![m.0.to_string()])
                    .unwrap_or_default(),
                cx,
            )
        });
    }
    fn change(&mut self, command: TaskWrite, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        self.error = None;
        match cx
            .global::<AppState>()
            .integrations
            .clone()
            .update(cx, |s, cx| s.submit(command, None, cx))
        {
            Ok(id) => self.pending = Some(id),
            Err(error) => {
                self.error = Some(error);
                self.sync_values(cx);
            }
        }
        self.sync_enabled(cx);
        cx.notify();
    }
}
impl Render for MetadataEditor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column()
            .gap(px(16.))
            .child(self.labels.clone())
            .child(self.assignees.clone())
            .child(self.milestone.clone())
            .children(self.error.clone().map(|e| integration_message(e, true)))
    }
}
