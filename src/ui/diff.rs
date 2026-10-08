use super::{components::*, theme as t};
use crate::app_state::AppState;
use canopy_desktop::{
    git::changes::{FileChange, FileDiff},
    state::workspace::Pane,
};
use gpui_kit::{component::IconName, *};
pub struct DiffView {
    pane: Pane,
    target: Option<FileChange>,
    data: Option<FileDiff>,
    longest_line: usize,
    additions: usize,
    deletions: usize,
    error: Option<String>,
    loading: bool,
    pending: bool,
    visible: bool,
    revision: u64,
    task: Option<Task<()>>,
}
impl DiffView {
    pub fn new(pane: Pane) -> Self {
        let target = pane
            .metadata
            .resource
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok());
        Self {
            pane,
            target,
            data: None,
            longest_line: 0,
            additions: 0,
            deletions: 0,
            error: None,
            loading: false,
            pending: false,
            visible: false,
            revision: 0,
            task: None,
        }
    }
    pub fn update_pane(&mut self, pane: &Pane, cx: &mut Context<Self>) {
        if self.pane.metadata.cwd != pane.metadata.cwd
            || self.pane.metadata.resource != pane.metadata.resource
        {
            self.pane = pane.clone();
            self.target = pane
                .metadata
                .resource
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok());
            self.data = None;
            self.error = None;
            if self.visible {
                self.reload(cx);
            }
        }
    }
    pub fn update_visibility(&mut self, visible: bool, revision: u64, cx: &mut Context<Self>) {
        let refresh = visible && (!self.visible || self.revision != revision);
        self.visible = visible;
        self.revision = revision;
        if refresh {
            self.reload(cx);
        }
    }
    fn reload(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            self.pending = true;
            return;
        }
        let Some(path) = self.pane.metadata.cwd.clone() else {
            self.error = Some("Diff has no working directory.".into());
            return;
        };
        let Some(file) = self.target.clone() else {
            self.error = Some("Invalid saved diff target.".into());
            return;
        };
        let Some(client) = cx.global::<AppState>().changes.read(cx).client.clone() else {
            return;
        };
        self.loading = true;
        cx.notify();
        let requested = file.clone();
        let cwd = path.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = client.diff(path, file).await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                if this.target.as_ref() == Some(&requested)
                    && this.pane.metadata.cwd.as_ref() == Some(&cwd)
                {
                    match result {
                        Ok(mut data) => {
                            data.lines.retain(|line| line.kind != 'F');
                            this.additions = data.lines.iter().filter(|l| l.kind == '+').count();
                            this.deletions = data.lines.iter().filter(|l| l.kind == '-').count();
                            this.longest_line = data
                                .lines
                                .iter()
                                .enumerate()
                                .max_by_key(|(_, l)| l.text.chars().count())
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            this.data = Some(data);
                            this.error = None;
                        }
                        Err(error) => this.error = Some(error),
                    }
                }
                if std::mem::take(&mut this.pending) && this.visible {
                    this.reload(cx);
                }
                cx.notify();
            });
        }));
    }
}
impl Render for DiffView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lines = self.data.as_ref().map(|d| d.lines.len()).unwrap_or(0);
        column()
            .size_full()
            .bg(t::bg())
            .child(
                git::diff_header(self.target.as_ref(), self.additions, self.deletions).child(
                    icon_button("refresh-diff", IconName::RotateCw, "Refresh diff")
                        .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
                ),
            )
            .children(
                self.error
                    .clone()
                    .map(|e| div().p(px(12.)).text_color(t::red()).child(e)),
            )
            .children(
                (self.loading && self.data.is_none())
                    .then(|| div().p(px(12.)).child("Loading diff…")),
            )
            .children(self.data.as_ref().filter(|d| d.binary).map(|_| {
                div()
                    .p(px(12.))
                    .text_color(t::muted())
                    .child("Binary file — no text preview.")
            }))
            .children(self.data.as_ref().filter(|d| d.truncated).map(|_| {
                div()
                    .p(px(12.))
                    .text_color(t::yellow())
                    .child("Diff preview truncated (large file, line, or more than 20,000 lines).")
            }))
            .children(
                (self.data.is_some() && lines == 0 && !self.loading).then(|| {
                    div()
                        .p(px(12.))
                        .text_color(t::muted())
                        .child("No changes remain in this comparison.")
                }),
            )
            .child(
                uniform_list(
                    "diff-lines",
                    lines,
                    cx.processor(|this, range: std::ops::Range<usize>, _, _| {
                        range
                            .filter_map(|i| this.data.as_ref().and_then(|d| d.lines.get(i)))
                            .map(git::diff_line)
                            .collect::<Vec<_>>()
                    }),
                )
                .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                .with_width_from_item(Some(self.longest_line))
                .flex_1()
                .min_h_0(),
            )
    }
}
