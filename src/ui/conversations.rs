use super::*;

impl Workspace {
    pub(super) fn rename_scratch_chat(
        &mut self,
        workspace: WorkspaceId,
        pane: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal.is_some() || self.new_chat.is_some() {
            return;
        }
        let projects = self.state.projects.clone();
        let state = projects.read(cx);
        if state.scratch_id(cx) != workspace {
            return;
        }
        let scratch = state.scratch_workspace(cx);
        let Some(value) = scratch.chat_title(pane).map(str::to_owned) else {
            return;
        };
        let return_focus = window.focused(cx).unwrap_or_else(|| self.focus.clone());
        let modal = cx.new(|cx| {
            TextPrompt::new(
                "Rename chat",
                "Chat name",
                value,
                return_focus,
                move |title, cx| {
                    projects.update(cx, |projects, cx| {
                        projects.rename_scratch_chat(workspace, pane, title, cx)
                    })
                },
                window,
                cx,
            )
        });
        self.modal_events = Some(cx.subscribe(&modal, |this, _, _: &ModalDismissed, cx| {
            this.modal = None;
            this.modal_events = None;
            cx.notify();
        }));
        self.modal = Some(modal);
        cx.notify();
    }
}
