//! Semantic activity presentation. The transcript owner retains expansion and thread selection.
use super::*;

impl RemoteWindow {
    pub(super) fn render_activity(
        &mut self,
        tool: &p::ToolCall,
        key: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = self.activity_expanded.contains(key);
        let epoch = key
            .split_once(':')
            .and_then(|(epoch, _)| epoch.parse::<u64>().ok())
            .unwrap_or_default();
        let kind = match p::ToolKind::try_from(tool.kind).ok() {
            Some(p::ToolKind::Read) => "Read",
            Some(p::ToolKind::Edit) => "Edit",
            Some(p::ToolKind::Execute) => "Command",
            Some(p::ToolKind::Search) => "Search",
            Some(p::ToolKind::Fetch) => "Web",
            Some(p::ToolKind::Plan) => "Plan",
            Some(p::ToolKind::Subagent) => "Subagent",
            _ => "Tool",
        };
        let state = match tool.state.as_str() {
            "streaming" => "Preparing",
            "running" => "Running",
            "succeeded" => "Completed",
            "failed" => "Failed",
            "cancelled" => "Cancelled",
            _ => "Unknown state",
        };
        let mut card = column()
            .gap_2()
            .border_l_2()
            .border_color(if tool.state == "failed" {
                t::red()
            } else {
                t::border()
            })
            .pl_3()
            .child(
                row()
                    .gap_2()
                    .child(format!("{kind} · {}", tool.title))
                    .child(
                        div()
                            .text_color(if tool.state == "failed" {
                                t::red()
                            } else {
                                t::secondary()
                            })
                            .child(state),
                    ),
            );
        if tool.background {
            card = card.child(if matches!(tool.state.as_str(), "running" | "streaming") {
                "Running in background"
            } else {
                "Background task"
            });
        }
        if let Some(activity) = &tool.activity {
            card = card.child(activity.clone());
        }
        if let Some(seconds) = tool.elapsed_seconds {
            card = card.child(format!("{seconds:.1} s"));
        }
        if let Some(command) = &tool.command {
            card = card.child(div().font_family(t::MONO).child(command.clone()));
        }
        let files: Vec<_> = if tool.files.is_empty() {
            tool.paths
                .iter()
                .filter_map(|path| self.activity_file(path))
                .collect()
        } else {
            tool.files.clone()
        };
        for file in files.into_iter().take(32) {
            card = card.child(
                button(
                    SharedString::from(format!("activity-file:{key}:{}", file.relative_path)),
                    file.relative_path.clone(),
                )
                .disabled(!self.ready())
                .on_click(
                    cx.listener(move |this, _, _, cx| this.open_remote_file(file.clone(), cx)),
                ),
            );
        }
        for (ix, entry) in tool.plan.iter().take(100).enumerate() {
            card = card.child(
                row()
                    .gap_2()
                    .child(format!("{}. {}", ix + 1, entry.content))
                    .child(entry.status.clone())
                    .children(entry.priority.clone()),
            );
        }
        if let Some(item) = tool.page_item {
            card = card.child(
                button(
                    SharedString::from(format!("page:{key}")),
                    "Open interactive page…",
                )
                .disabled(!self.ready())
                .on_click(cx.listener(move |this, _, _, cx| {
                    let host = this.host.clone().unwrap();
                    let chat = this.chat.clone().unwrap();
                    this.run(
                        async move {
                            let bytes = tokio::time::timeout(
                                std::time::Duration::from_secs(30),
                                host.read_chat_page(chat, item),
                            )
                            .await
                            .map_err(|_| {
                                "Page read timed out. Retry opening the page.".to_owned()
                            })??;
                            let html =
                                String::from_utf8(bytes).map_err(|_| "Page is not UTF-8.")?;
                            Ok(Update::HtmlPreview(html))
                        },
                        cx,
                    );
                })),
            );
        }
        if !tool.diff.is_empty() {
            let added = tool.diff.iter().filter(|line| line.kind == "+").count();
            let removed = tool.diff.iter().filter(|line| line.kind == "-").count();
            card = card.child(format!("+{added} −{removed}"));
            for line in tool.diff.iter().take(if expanded { 200 } else { 12 }) {
                card = card.child(
                    div()
                        .font_family(t::MONO)
                        .text_color(match line.kind.as_str() {
                            "+" => t::green(),
                            "-" => t::red(),
                            _ => t::secondary(),
                        })
                        .child(format!(
                            "{} {} {}",
                            line.old_line.map(|n| n.to_string()).unwrap_or_default(),
                            line.new_line.map(|n| n.to_string()).unwrap_or_default(),
                            line.text
                        )),
                );
            }
            if tool.diff.len() > if expanded { 200 } else { 12 } {
                card = card.child(format!(
                    "Showing {} of {} diff lines",
                    if expanded { 200 } else { 12 },
                    tool.diff.len()
                ));
            }
            let call_id = tool.call_id.clone();
            card = card.child(
                button(
                    SharedString::from(format!("activity-diff:{key}")),
                    "Open diff…",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(tool) = this
                        .projection
                        .items
                        .iter()
                        .filter(|item| item.key.is_some_and(|key| key.history_epoch == epoch))
                        .find_map(|item| match &item.body {
                            Some(p::transcript_item::Body::Tool(tool))
                                if tool.call_id == call_id =>
                            {
                                Some(tool)
                            }
                            _ => None,
                        })
                    {
                        let diff = tool
                            .diff
                            .iter()
                            .map(|line| format!("{}{}", line.kind, line.text))
                            .collect::<Vec<_>>()
                            .join("\n");
                        this.apply(Update::TextPreview("Diff".into(), diff, false), cx);
                    }
                })),
            );
        }
        if tool.plan.len() > 100 {
            card = card.child("Showing the first 100 plan entries.");
        }
        if expanded {
            if let Some(markdown) = self.markdown.get(key) {
                card = card.child(markdown.clone());
            }
            if let Some(input) = &tool.generic_input {
                card = card.child(
                    div()
                        .font_family(t::MONO)
                        .child(input.chars().take(20_000).collect::<String>()),
                );
            }
        }
        if tool.output.is_some() || tool.generic_input.is_some() || tool.plan.len() > 100 {
            let call_id = tool.call_id.clone();
            card = card.child(
                button(
                    SharedString::from(format!("activity-full:{key}")),
                    "Open full activity…",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(tool) = this
                        .projection
                        .items
                        .iter()
                        .filter(|item| item.key.is_some_and(|key| key.history_epoch == epoch))
                        .find_map(|item| match &item.body {
                            Some(p::transcript_item::Body::Tool(tool))
                                if tool.call_id == call_id =>
                            {
                                Some(tool)
                            }
                            _ => None,
                        })
                    {
                        let mut text = format!(
                            "{}\n{}\n{}",
                            tool.title,
                            tool.generic_input.as_deref().unwrap_or_default(),
                            tool.output.as_deref().unwrap_or_default()
                        );
                        for entry in &tool.plan {
                            text.push_str(&format!("\n{}: {}", entry.status, entry.content));
                        }
                        this.apply(Update::TextPreview("Activity".into(), text, false), cx);
                    }
                })),
            );
        }
        if tool.output.is_some() || tool.generic_input.is_some() || tool.diff.len() > 12 {
            let key = key.to_owned();
            card = card.child(
                button(
                    SharedString::from(format!("activity-expand:{key}")),
                    if expanded {
                        "Show less"
                    } else {
                        "Show details"
                    },
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.activity_expanded.remove(&key) {
                        this.activity_expanded.insert(key.clone());
                    }
                    this.chat_list.reset(this.chat_rows.len());
                    cx.notify();
                })),
            );
        }
        if self
            .activity_children
            .contains(&(epoch, tool.call_id.clone()))
        {
            let call = tool.call_id.clone();
            card = card.child(
                button(
                    SharedString::from(format!("activity-thread:{call}")),
                    "Open thread",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.activity_thread = Some((epoch, call.clone()));
                    this.refresh_markdown(cx);
                    cx.notify();
                })),
            );
        }
        card.into_any_element()
    }
    fn activity_file(&self, path: &str) -> Option<p::FileRef> {
        let workspace = self.projection.summary.as_ref()?.workspace.clone()?;
        let catalog = self.host.as_ref()?.catalog()?;
        let root = catalog
            .projects
            .iter()
            .flat_map(|p| &p.checkouts)
            .find(|w| w.workspace.as_ref() == Some(&workspace))?;
        let relative = path
            .strip_prefix(&format!("{}/", root.path.trim_end_matches('/')))
            .unwrap_or(path);
        if relative.starts_with('/')
            || relative.contains('\\')
            || relative.split('/').any(|part| part == "..")
        {
            return None;
        }
        Some(p::FileRef {
            workspace: Some(workspace),
            relative_path: relative.to_owned(),
        })
    }
}

fn thread_member(item: &p::TranscriptItem, epoch: u64, parent: &str) -> bool {
    item.key.is_some_and(|key| key.history_epoch == epoch)
        && (item.parent_call_id.as_deref() == Some(parent)
            || matches!(&item.body, Some(p::transcript_item::Body::Tool(tool)) if tool.call_id == parent))
}
pub(super) fn visible(
    item: &p::TranscriptItem,
    thread: Option<&(u64, String)>,
    parents: &std::collections::BTreeSet<(u64, String)>,
) -> bool {
    match thread {
        Some((epoch, parent)) => thread_member(item, *epoch, parent),
        None => item.parent_call_id.as_ref().is_none_or(|parent| {
            !parents.contains(&(item.key.unwrap_or_default().history_epoch, parent.clone()))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::thread_member;
    use canopy_desktop::remote::proto as p;
    #[test]
    fn nested_navigation_does_not_merge_replayed_call_ids_from_another_epoch() {
        let mut item = p::TranscriptItem {
            key: Some(p::ItemKey {
                history_epoch: 2,
                item_id: 10,
            }),
            parent_call_id: Some("worker".into()),
            ..Default::default()
        };
        assert!(thread_member(&item, 2, "worker"));
        assert!(!thread_member(&item, 1, "worker"));
        item.parent_call_id = Some("another-worker".into());
        assert!(!thread_member(&item, 2, "worker"));
        item.body = Some(p::transcript_item::Body::Tool(p::ToolCall {
            call_id: "worker".into(),
            ..Default::default()
        }));
        assert!(thread_member(&item, 2, "worker"));
    }
}
