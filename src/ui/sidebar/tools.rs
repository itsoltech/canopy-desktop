use super::*;
use gpui_kit::base::Disableable;
impl Sidebar {
    fn hover_tool_label(
        &mut self,
        id: &str,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if hovered {
            if self.hovered_tool_label.as_deref() == Some(id) {
                return;
            }
            self.hovered_tool_label = Some(id.to_owned());
        } else if self.hovered_tool_label.as_deref() == Some(id) {
            self.hovered_tool_label = None;
        } else {
            return;
        }
        window.refresh();
        cx.notify();
    }

    pub(super) fn sync_tools(&mut self, cx: &mut Context<Self>) {
        let catalog = &self.tools.read(cx).catalog;
        self.profiles.retain(|id, _| catalog.get(id).is_some());
        for tool in &catalog.tools {
            let disclosure = self
                .profiles
                .entry(tool.id.clone())
                .or_insert_with(|| Disclosure::new(false, Instant::now()));
            if tool.profiles.len() < 2 {
                *disclosure = Disclosure::new(false, Instant::now());
            }
        }
        cx.notify();
    }
    pub(super) fn tools_content(&self, now: Instant, cx: &mut Context<Self>) -> (Div, f32) {
        let state = self.tools.read(cx);
        let runtime = self.terminals.read(cx);
        let mut height = 0.;
        let rows = state
            .catalog
            .tools
            .iter()
            .filter(|t| t.enabled)
            .map(|tool| {
                let id = tool.id.clone();
                let launch_id = id.clone();
                let hover_id = id.clone();
                let expandable = tool.profiles.len() > 1;
                let sole_profile = (tool.profiles.len() == 1).then(|| tool.profiles[0].id.clone());
                let disclosure = &self.profiles[&id];
                let running = runtime.running_count(&id, None, cx);
                let profiles_height = if expandable {
                    tool.profiles.len() as f32 * t::ROW
                } else {
                    0.
                };
                height += t::ROW + disclosure.height(profiles_height, now);
                let availability = state.availability.get(&id);
                let tooltip = match availability {
                    Some(Ok(path)) => path.display().to_string(),
                    Some(Err(e)) => e.clone(),
                    None => "Checking executable…".into(),
                };
                let header = ToolHeader {
                    name: &tool.name,
                    expanded: expandable.then_some(disclosure.open),
                    chevron: expandable.then(|| disclosure.chevron(now)),
                    hovered: self.hovered_tool_label.as_deref() == Some(&id),
                    running,
                    missing: matches!(availability, Some(Err(_))),
                }
                .button(SharedString::from(format!("tool-header-{id}")), cx)
                .tooltip(tooltip)
                .disabled(!state.ready || !self.sections[2].open)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if expandable {
                        if let Some(disclosure) = this.profiles.get_mut(&launch_id) {
                            disclosure.toggle(Instant::now(), cx);
                            cx.notify();
                        }
                    } else {
                        this.tools.update(cx, |state, cx| {
                            state.launch(&launch_id, sole_profile.as_deref(), cx)
                        });
                    }
                }));
                let header = hover_action(
                    SharedString::from(format!("tool-hover-{id}")),
                    header,
                    cx.listener(move |this, hovered, window, cx| {
                        this.hover_tool_label(&hover_id, *hovered, window, cx)
                    }),
                )
                .w_full()
                .h(px(t::ROW))
                .flex_shrink_0();
                let body = column().children(tool.profiles.iter().map(|profile| {
                    let tool_id = id.clone();
                    let profile_id = profile.id.clone();
                    let hover_id = profile.id.clone();
                    let count = runtime.running_count(&id, Some(&profile.id), cx);
                    row()
                        .id(SharedString::from(format!("profile-row-{}", profile.id)))
                        .on_hover(cx.listener(move |this, hovered, window, cx| {
                            this.hover_tool_label(&hover_id, *hovered, window, cx)
                        }))
                        .h(px(t::ROW))
                        .flex_shrink_0()
                        .pl(px(24.))
                        .gap(px(6.))
                        .child(
                            list_button(
                                SharedString::from(format!("profile-{}", profile.id)),
                                profile.name.clone(),
                                self.hovered_tool_label.as_deref() == Some(&profile.id),
                                cx,
                            )
                            .flex_1()
                            .border_0()
                            .bg(rgba(0))
                            .px(px(2.))
                            .disabled(!disclosure.open || !state.ready || !self.sections[2].open)
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.tools.update(cx, |state, cx| {
                                        state.launch(&tool_id, Some(&profile_id), cx)
                                    })
                                },
                            )),
                        )
                        .children(
                            (tool.default_profile.as_ref() == Some(&profile.id)).then(|| {
                                div()
                                    .text_color(t::faint())
                                    .text_size(px(10.))
                                    .child("default")
                            }),
                        )
                        .children((count > 0).then(|| badge(count.to_string())))
                }));
                column()
                    .child(header)
                    .children(expandable.then(|| disclosure.body(profiles_height, body, now)))
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        let content = column().children(rows).children(
            state
                .error
                .clone()
                .map(|e| div().text_size(px(10.)).text_color(t::red()).child(e)),
        );
        if state.error.is_some() {
            height += t::ROW * 2.;
        }
        (content, height)
    }
}
