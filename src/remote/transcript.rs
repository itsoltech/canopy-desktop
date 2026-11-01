//! Pure domain-to-wire projection. Provider names never determine tool semantics.
use crate::api::proto;
use crate::chat::{self, transcript as t};
use std::{collections::HashMap, path::Path};
use tonic::Status;

pub fn key(epoch: u64, id: u64) -> proto::ItemKey {
    proto::ItemKey {
        history_epoch: epoch,
        item_id: id,
    }
}
pub fn attachment(
    value: &t::Attachment,
    project_file: bool,
    root: &Path,
    images: &HashMap<String, proto::ArtifactRef>,
) -> Result<proto::Attachment, Status> {
    use proto::attachment::Content;
    let content = match value {
        t::Attachment::Image { file, .. } => {
            let image = images.get(file).cloned().ok_or_else(|| {
                Status::failed_precondition(
                    "The chat image has not been published for remote reading.",
                )
            })?;
            Content::Image(proto::ImageAttachment {
                host_id: image.host_id,
                artifact_id: image.artifact_id,
                sha256: image.sha256,
                media_type: image.media_type,
                size: image.size,
            })
        }
        t::Attachment::Path { path, .. } => {
            let relative = if path.is_absolute() {
                path.strip_prefix(root).map_err(|_| {
                    Status::permission_denied("The attachment is outside this checkout.")
                })?
            } else {
                path.as_path()
            };
            if !relative
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
            {
                return Err(Status::permission_denied("Invalid attachment path."));
            }
            Content::File(proto::FileAttachment {
                project_file,
                relative_path: relative.to_string_lossy().into_owned(),
            })
        }
        t::Attachment::Quote { text, .. } => {
            Content::Quote(proto::QuoteAttachment { text: text.clone() })
        }
        t::Attachment::Command {
            name,
            skill,
            path,
            description,
            ..
        } => Content::Command(proto::CommandAttachment {
            name: name.clone(),
            skill: *skill,
            path: path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            description: description.clone(),
        }),
        t::Attachment::Pull {
            number,
            title,
            url,
            head,
            base,
            state,
            body,
            ..
        } => Content::Pull(proto::PullAttachment {
            number: *number,
            title: title.clone(),
            url: url.clone(),
            head: head.clone(),
            base: base.clone(),
            state: state.clone(),
            body: body.clone(),
            context_id: None,
        }),
        t::Attachment::Shell {
            command,
            exit,
            stopped,
            output,
            ..
        } => Content::Shell(proto::ShellAttachment {
            command: command.clone(),
            exit_code: *exit,
            stopped: *stopped,
            output: output.clone(),
        }),
    };
    Ok(proto::Attachment {
        label: value.label().unwrap_or_default().into(),
        content: Some(content),
    })
}
pub fn draft(
    value: &chat::session::Draft,
    project_file: bool,
    root: &Path,
    images: &HashMap<String, proto::ArtifactRef>,
) -> Result<proto::MessageDraft, Status> {
    Ok(proto::MessageDraft {
        text: value.text.clone(),
        context: value.context.clone(),
        goal: value.goal,
        attachments: value
            .attachments
            .iter()
            .map(|a| attachment(a, project_file, root, images))
            .collect::<Result<_, _>>()?,
    })
}
pub fn item(
    value: &t::Item,
    epoch: u64,
    revision: u64,
    run: u64,
    project_file: bool,
    root: &Path,
    images: &HashMap<String, proto::ArtifactRef>,
) -> Result<proto::TranscriptItem, Status> {
    use proto::transcript_item::Body;
    let body = match &value.body {
        t::Body::User(message) => Body::User(proto::UserMessage {
            message_id: message.uuid.clone().unwrap_or_default(),
            draft: Some(draft(
                &chat::session::Draft {
                    text: message.text.clone(),
                    attachments: message.attachments.clone(),
                    context: message.context.clone(),
                    goal: message.goal,
                },
                project_file,
                root,
                images,
            )?),
            delivery: match message.delivery {
                t::Delivery::Sent => "sent",
                t::Delivery::Steered => "steered",
            }
            .into(),
        }),
        t::Body::Text(block) => Body::Text(proto::TextBlock {
            text: block.text.clone(),
            streaming: block.streaming,
        }),
        t::Body::Thinking(block) => Body::Thinking(proto::TextBlock {
            text: block.text.clone(),
            streaming: block.streaming,
        }),
        t::Body::Tool(tool) => {
            use chat::backend::ToolKind as K;
            let kind = match tool.semantic_kind() {
                K::Read => proto::ToolKind::Read,
                K::Edit => proto::ToolKind::Edit,
                K::Shell => proto::ToolKind::Execute,
                K::Search => proto::ToolKind::Search,
                K::Web => proto::ToolKind::Fetch,
                K::Agent => proto::ToolKind::Subagent,
                K::Plan => proto::ToolKind::Plan,
                K::Other => proto::ToolKind::Other,
            };
            let (command, diff) = match chat::tools::view(tool) {
                chat::tools::ToolView::Shell { command, .. } => (Some(command), vec![]),
                chat::tools::ToolView::Change(change) => (
                    None,
                    change
                        .lines
                        .into_iter()
                        .map(|line| proto::DiffLine {
                            old_line: line.old,
                            new_line: line.new,
                            kind: line.kind.to_string(),
                            text: line.text,
                        })
                        .collect(),
                ),
                _ => (None, vec![]),
            };
            Body::Tool(proto::ToolCall {
                call_id: tool.call_id.clone(),
                original_name: tool.name.clone(),
                kind: kind as i32,
                title: chat::presentation::tool_summary(tool, root),
                state: match tool.state {
                    t::ToolState::Streaming => "streaming",
                    t::ToolState::Running => "running",
                    t::ToolState::Succeeded => "succeeded",
                    t::ToolState::Failed => "failed",
                    t::ToolState::Cancelled => "cancelled",
                }
                .into(),
                paths: tool.file_path().into_iter().map(str::to_owned).collect(),
                output: tool.output.as_ref().map(|o| o.text.clone()),
                background: tool.background,
                activity: tool.activity.clone(),
                elapsed_seconds: tool.elapsed_seconds,
                command,
                diff,
                generic_input: (kind == proto::ToolKind::Other).then(|| tool.input.to_string()),
                plan: if kind == proto::ToolKind::Plan {
                    tool.input
                        .get("todos")
                        .and_then(serde_json::Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|entry| {
                            Some(proto::PlanEntry {
                                content: entry
                                    .get("content")
                                    .or_else(|| entry.get("text"))?
                                    .as_str()?
                                    .into(),
                                status: entry
                                    .get("status")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("pending")
                                    .into(),
                                priority: entry
                                    .get("priority")
                                    .and_then(serde_json::Value::as_str)
                                    .map(str::to_owned),
                            })
                        })
                        .collect()
                } else {
                    Vec::new()
                },
                files: tool
                    .file_path()
                    .and_then(|path| scoped_file(path, project_file, root))
                    .into_iter()
                    .collect(),
                page_item: (tool.name == chat::page::SHOW_TOOL
                    && chat::page::Page::from_input(&tool.input).is_ok())
                .then(|| key(epoch, value.id.0)),
            })
        }
        t::Body::Permission(permission) => Body::Decision(proto::Decision {
            decision_id: value.id.0.to_string(),
            run_id: run.to_string(),
            title: permission
                .title
                .clone()
                .unwrap_or_else(|| permission.tool.clone()),
            state: match permission.decision {
                t::Decision::Pending => "pending",
                t::Decision::Allowed => "allowed",
                t::Decision::Denied => "denied",
                t::Decision::Cancelled => "cancelled",
            }
            .into(),
            options: vec![],
            resolution: permission
                .resolution
                .as_ref()
                .and_then(|r| serde_json::to_string(r).ok()),
            native_kind: Some(
                match permission.kind {
                    t::PermissionKind::Tool => "tool",
                    t::PermissionKind::Question => "question",
                    t::PermissionKind::Plan => "plan",
                }
                .into(),
            ),
            description: permission.description.clone(),
            can_remember: permission.remember.is_some(),
            questions: chat::presentation::questions(permission)
                .into_iter()
                .map(|q| proto::DecisionQuestion {
                    question: q.question,
                    header: q.header,
                    multiple: q.multi,
                    options: q
                        .options
                        .into_iter()
                        .map(|(label, description)| proto::OptionChoice {
                            id: label.clone(),
                            label,
                            description: Some(description),
                        })
                        .collect(),
                })
                .collect(),
        }),
        t::Body::Notice(notice) => Body::Notice(proto::Notice {
            kind: match notice.kind {
                t::NoticeKind::ApiRetry => "api_retry",
                t::NoticeKind::RateLimit => "rate_limit",
                t::NoticeKind::Compacted => "compacted",
                t::NoticeKind::Diagnostic => "diagnostic",
                t::NoticeKind::Error => "error",
            }
            .into(),
            text: notice.text.clone(),
            until_unix: notice.until,
        }),
        t::Body::TurnEnd(end) => {
            let (outcome, error) = match &end.outcome {
                t::Outcome::Completed => ("completed", None),
                t::Outcome::Interrupted => ("interrupted", None),
                t::Outcome::Failed(e) => ("failed", Some(e.clone())),
                t::Outcome::Limit(e) => ("limit", Some(e.clone())),
            };
            Body::TurnEnd(proto::TurnEnd {
                outcome: outcome.into(),
                error,
                usage: end.usage.as_ref().map(|u| proto::Usage {
                    input_tokens: u.input_tokens,
                    output_tokens: u.output_tokens,
                    ..Default::default()
                }),
                saved: false,
                fork_available: Some(end.resume_at.is_some() && !value.trimmed),
            })
        }
        t::Body::Shell(shell) => Body::Shell(proto::ShellRun {
            command: shell.command.clone(),
            output: shell.output.clone(),
            running: shell.running,
            stopped: shell.stopped,
            exit_code: shell.exit,
        }),
        t::Body::GoalCheck(check) => Body::Goal(proto::GoalState {
            objective: String::new(),
            state: "checking".into(),
            turns: 0,
            checks: check.n,
            note: Some(check.reason.clone()),
            ..Default::default()
        }),
        t::Body::GoalTurn(n) => Body::Goal(proto::GoalState {
            objective: String::new(),
            state: "pursuing".into(),
            turns: *n,
            note: None,
            ..Default::default()
        }),
        t::Body::GoalEnd(end) => Body::Goal(proto::GoalState {
            objective: end.objective.clone(),
            state: match end.reason {
                chat::goal::EndReason::Achieved => "achieved",
                chat::goal::EndReason::Impossible => "impossible",
                chat::goal::EndReason::ClearedByUser => "cleared_by_user",
                chat::goal::EndReason::ClearedOnSwitch => "cleared_on_switch",
                chat::goal::EndReason::ClearedOnEdit => "cleared_on_edit",
                chat::goal::EndReason::ClearedByError => "cleared_by_error",
            }
            .into(),
            turns: end.turns,
            note: end.note.clone(),
            tokens: end.tokens,
            elapsed_seconds: end.seconds,
            checks: end.checks,
            ..Default::default()
        }),
    };
    Ok(proto::TranscriptItem {
        key: Some(key(epoch, value.id.0)),
        revision,
        turn_id: value.turn.0,
        parent_call_id: value.parent.clone(),
        trimmed: value.trimmed,
        body: Some(body),
    })
}

fn scoped_file(path: &str, project_file: bool, root: &Path) -> Option<proto::FileAttachment> {
    let path = Path::new(path);
    let relative = if path.is_absolute() {
        path.strip_prefix(root).ok()?
    } else {
        path
    };
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
    {
        return None;
    }
    Some(proto::FileAttachment {
        project_file,
        relative_path: relative.to_str()?.into(),
    })
}

/// Full replacements remain available. Text appends carry UTF-8 byte offsets,
/// never character indices, so a reconnect cannot duplicate a multi-byte suffix.
pub fn text_append(
    old: &proto::TranscriptItem,
    new: &proto::TranscriptItem,
) -> Option<proto::TextAppend> {
    use proto::transcript_item::Body;
    if old.key != new.key || new.revision <= old.revision {
        return None;
    }
    let (before, after) = match (old.body.as_ref()?, new.body.as_ref()?) {
        (Body::Text(a), Body::Text(b)) | (Body::Thinking(a), Body::Thinking(b)) => (a, b),
        _ => return None,
    };
    let suffix = after.text.strip_prefix(&before.text)?;
    Some(proto::TextAppend {
        key: new.key,
        expected_revision: old.revision,
        revision: new.revision,
        utf8_offset: before.text.len() as u64,
        text: suffix.into(),
        streaming: after.streaming,
    })
}

pub const MAX_ITEM_BYTES: usize = 32 * 1024 * 1024;
pub fn fragments(item: &proto::TranscriptItem) -> Result<Vec<proto::ItemFragment>, Status> {
    use prost::Message;
    use sha2::{Digest, Sha256};
    if item.encoded_len() > MAX_ITEM_BYTES {
        return Err(Status::resource_exhausted(
            "A transcript item exceeds the remote assembly limit.",
        ));
    }
    let bytes = item.encode_to_vec();
    let hash = Sha256::digest(&bytes).to_vec();
    let transfer = uuid::Uuid::new_v4().to_string();
    Ok(bytes
        .chunks(64 * 1024)
        .enumerate()
        .map(|(index, data)| proto::ItemFragment {
            key: item.key,
            revision: item.revision,
            transfer_id: transfer.clone(),
            offset: (index * 64 * 1024) as u64,
            total_bytes: bytes.len() as u64,
            sha256: hash.clone(),
            data: data.to_vec(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use sha2::{Digest, Sha256};
    #[test]
    fn unknown_agent_tool_named_bash_keeps_its_declared_semantics() {
        let tool = t::ToolCall {
            kind: Some(chat::backend::ToolKind::Other),
            call_id: "opaque".into(),
            name: "Bash".into(),
            input: serde_json::json!({"example":true}),
            input_partial: String::new(),
            state: t::ToolState::Succeeded,
            output: None,
            elapsed_seconds: None,
            activity: None,
            background: false,
        };
        let value = t::Item {
            id: t::ItemId(5),
            turn: t::TurnId(1),
            parent: None,
            body: t::Body::Tool(tool),
            trimmed: false,
        };
        let projected = item(&value, 1, 1, 1, false, Path::new("/"), &HashMap::new()).unwrap();
        let Some(proto::transcript_item::Body::Tool(tool)) = projected.body else {
            panic!("tool projection missing")
        };
        assert_eq!(tool.kind, proto::ToolKind::Other as i32);
        assert!(tool.command.is_none());
        assert!(tool.generic_input.is_some());
    }
    #[test]
    fn utf8_appends_and_large_item_fragments_reconstruct_the_same_item() {
        let old = proto::TranscriptItem {
            key: Some(key(1, 1)),
            revision: 2,
            body: Some(proto::transcript_item::Body::Text(proto::TextBlock {
                text: "ą🙂".into(),
                streaming: true,
            })),
            ..Default::default()
        };
        let mut new = old.clone();
        new.revision = 3;
        new.body = Some(proto::transcript_item::Body::Text(proto::TextBlock {
            text: format!("ą🙂{}", "λ".repeat(600_000)),
            streaming: false,
        }));
        let append = text_append(&old, &new).unwrap();
        assert_eq!(append.utf8_offset, 6);
        let pieces = fragments(&new).unwrap();
        let mut data = Vec::new();
        for piece in &pieces {
            assert_eq!(piece.offset, data.len() as u64);
            assert!(piece.data.len() <= 64 * 1024);
            data.extend_from_slice(&piece.data);
        }
        assert_eq!(Sha256::digest(&data).as_slice(), pieces[0].sha256);
        assert_eq!(proto::TranscriptItem::decode(data.as_slice()).unwrap(), new);
        new.body = Some(proto::transcript_item::Body::Text(proto::TextBlock {
            text: "replacement".into(),
            streaming: false,
        }));
        assert!(text_append(&old, &new).is_none());
    }
}
