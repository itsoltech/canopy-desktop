//! The same conversation model feeds the desktop renderer for local and remote
//! agents. Wire identities remain epoch-scoped; paths are display data, never
//! local filesystem authority.
use crate::{
    chat::{self, transcript as t},
    remote::proto as p,
};
use serde_json::json;
pub fn call_id(epoch: u64, id: &str) -> String {
    format!("{epoch}:{id}")
}

pub fn draft(value: &p::MessageDraft) -> chat::session::Draft {
    chat::session::Draft {
        text: value.text.clone(),
        context: value.context.clone(),
        goal: value.goal,
        attachments: value.attachments.iter().filter_map(attachment).collect(),
    }
}

pub fn attachment(value: &p::Attachment) -> Option<t::Attachment> {
    use p::attachment::Content;
    let label = (!value.label.is_empty()).then(|| value.label.clone());
    Some(match value.content.as_ref()? {
        Content::Image(image) => t::Attachment::Image {
            file: format!("remote:{}", image.artifact_id),
            media_type: image.media_type.clone(),
            width: 0,
            height: 0,
            data: None,
            label,
        },
        Content::Path(file) => t::Attachment::Path {
            path: file.relative_path.clone().into(),
            label,
        },
        Content::Quote(quote) => t::Attachment::Quote {
            text: quote.text.clone(),
            label,
        },
        Content::Command(command) => t::Attachment::Command {
            name: command.name.clone(),
            skill: command.skill,
            path: command.path.as_ref().map(Into::into),
            description: command.description.clone(),
            label,
        },
        Content::Pull(pull) => t::Attachment::Pull {
            number: pull.number,
            title: pull.title.clone(),
            url: pull.url.clone(),
            head: pull.head.clone(),
            base: pull.base.clone(),
            state: pull.state.clone(),
            body: pull.body.clone(),
            label,
        },
        Content::Shell(shell) => t::Attachment::Shell {
            command: shell.command.clone(),
            exit: shell.exit_code,
            stopped: shell.stopped,
            output: shell.output.clone(),
            label,
        },
    })
}

pub fn item(value: &p::TranscriptItem) -> Option<t::Item> {
    use p::transcript_item::Body;
    let key = value.key?;
    let body = match value.body.as_ref()? {
        Body::User(user) => {
            let draft = user.draft.as_ref().map(draft).unwrap_or_default();
            t::Body::User(t::UserMessage {
                text: draft.text,
                attachments: draft.attachments,
                context: draft.context,
                goal: draft.goal,
                uuid: Some(user.message_id.clone()),
                delivery: if user.delivery == "steered" {
                    t::Delivery::Steered
                } else {
                    t::Delivery::Sent
                },
            })
        }
        Body::Text(text) => t::Body::Text(t::TextBlock {
            text: text.text.clone(),
            streaming: text.streaming,
        }),
        Body::Thinking(text) => t::Body::Thinking(t::TextBlock {
            text: text.text.clone(),
            streaming: text.streaming,
        }),
        Body::Tool(tool) => {
            use chat::backend::ToolKind as K;
            let kind = match p::ToolKind::try_from(tool.kind).unwrap_or(p::ToolKind::Other) {
                p::ToolKind::Read => K::Read,
                p::ToolKind::Edit => K::Edit,
                p::ToolKind::Execute => K::Shell,
                p::ToolKind::Search => K::Search,
                p::ToolKind::Fetch => K::Web,
                p::ToolKind::Plan => K::Plan,
                p::ToolKind::Subagent => K::Agent,
                _ => K::Other,
            };
            let mut input = tool
                .generic_input
                .as_deref()
                .and_then(|v| serde_json::from_str(v).ok())
                .filter(serde_json::Value::is_object)
                .unwrap_or_else(|| json!({}));
            if let Some(file) = tool.files.first() {
                input["file_path"] = json!(file.relative_path);
            }
            if let Some(command) = &tool.command {
                input["command"] = json!(command);
            }
            input["description"] = json!(tool.title);
            if tool.page_item.is_some() {
                input["html"] = json!("<p>Loading page…</p>");
                if !input["title"].is_string() {
                    input["title"] = json!(tool.title);
                }
            }
            if !tool.plan.is_empty() {
                input["todos"] = json!(tool.plan.iter().map(|entry| json!({
                    "content": entry.content, "status": entry.status, "priority": entry.priority
                })).collect::<Vec<_>>());
            }
            let structured = if !tool.diff.is_empty() {
                Some(
                    json!({"filePath": tool.files.first().map(|f| f.relative_path.as_str()).unwrap_or(""),
                    "type": "update", "structuredPatch": [{"oldStart": 1,"newStart": 1,
                    "lines": tool.diff.iter().map(|line| format!("{}{}", line.kind, line.text)).collect::<Vec<_>>()}]}),
                )
            } else if kind == K::Read {
                tool.output.as_ref().map(|output| json!({"file": {
                    "filePath": tool.files.first().map(|f| f.relative_path.as_str()).unwrap_or(""),
                    "content": output, "startLine": 1, "numLines": output.lines().count(), "totalLines": output.lines().count()
                }}))
            } else {
                None
            };
            t::Body::Tool(t::ToolCall {
                kind: Some(kind),
                call_id: call_id(key.history_epoch, &tool.call_id),
                name: tool.original_name.clone(),
                input,
                input_partial: String::new(),
                state: match tool.state.as_str() {
                    "streaming" => t::ToolState::Streaming,
                    "running" => t::ToolState::Running,
                    "succeeded" => t::ToolState::Succeeded,
                    "cancelled" => t::ToolState::Cancelled,
                    _ => t::ToolState::Failed,
                },
                output: tool.output.as_ref().map(|text| t::ToolOutput {
                    text: text.clone(),
                    is_error: tool.state == "failed",
                    structured,
                }),
                elapsed_seconds: tool.elapsed_seconds,
                activity: tool.activity.clone(),
                background: tool.background,
            })
        }
        Body::Decision(decision) => t::Body::Permission(t::Permission {
            request_id: decision.decision_id.clone(),
            call_id: None,
            tool: decision.title.clone(),
            title: Some(decision.title.clone()),
            description: decision.description.clone(),
            input: json!({"plan": decision.description, "questions": decision.questions.iter().map(|q| json!({
                "question": q.question, "header": q.header, "multiSelect": q.multiple,
                "options": q.options.iter().map(|o| json!({"label": o.label, "description": o.description})).collect::<Vec<_>>()
            })).collect::<Vec<_>>()}),
            kind: match decision.native_kind.as_deref() {
                Some("question") => t::PermissionKind::Question,
                Some("plan") => t::PermissionKind::Plan,
                _ => t::PermissionKind::Tool,
            },
            decision: match decision.state.as_str() {
                "pending" => t::Decision::Pending,
                "allowed" => t::Decision::Allowed,
                "denied" => t::Decision::Denied,
                _ => t::Decision::Cancelled,
            },
            remember: decision.can_remember.then_some(t::Remember::Session),
            resolution: decision
                .resolution
                .as_deref()
                .and_then(|v| serde_json::from_str(v).ok()),
        }),
        Body::Notice(notice) => t::Body::Notice(t::Notice {
            kind: match notice.kind.as_str() {
                "api_retry" => t::NoticeKind::ApiRetry,
                "rate_limit" => t::NoticeKind::RateLimit,
                "compacted" => t::NoticeKind::Compacted,
                "error" => t::NoticeKind::Error,
                _ => t::NoticeKind::Diagnostic,
            },
            text: notice.text.clone(),
            until: notice.until_unix,
        }),
        Body::TurnEnd(end) => t::Body::TurnEnd(t::TurnEnd {
            outcome: match end.outcome.as_str() {
                "completed" => t::Outcome::Completed,
                "interrupted" => t::Outcome::Interrupted,
                "limit" => t::Outcome::Limit(end.error.clone().unwrap_or_default()),
                _ => t::Outcome::Failed(end.error.clone().unwrap_or_default()),
            },
            usage: end.usage.as_ref().map(|u| t::Usage {
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
                ..Default::default()
            }),
            cost_usd: None,
            duration_ms: None,
            resume_at: (end.saved && end.fork_available == Some(true))
                .then(|| key.item_id.to_string()),
            quiet: false,
        }),
        Body::Shell(shell) => t::Body::Shell(t::ShellRun {
            command: shell.command.clone(),
            output: shell.output.clone(),
            running: shell.running,
            stopped: shell.stopped,
            exit: shell.exit_code,
            ..Default::default()
        }),
        Body::Goal(goal) if goal.state == "checking" => t::Body::GoalCheck(t::GoalCheck {
            n: goal.checks,
            reason: goal.note.clone().unwrap_or_default(),
        }),
        Body::Goal(goal) if goal.state == "pursuing" => t::Body::GoalTurn(goal.turns),
        Body::Goal(goal) => t::Body::GoalEnd(chat::goal::GoalEnd {
            objective: goal.objective.clone(),
            reason: match goal.state.as_str() {
                "achieved" => chat::goal::EndReason::Achieved,
                "impossible" => chat::goal::EndReason::Impossible,
                "cleared_on_edit" => chat::goal::EndReason::ClearedOnEdit,
                "cleared_on_switch" => chat::goal::EndReason::ClearedOnSwitch,
                "cleared_by_error" => chat::goal::EndReason::ClearedByError,
                _ => chat::goal::EndReason::ClearedByUser,
            },
            note: goal.note.clone(),
            turns: goal.turns,
            checks: goal.checks,
            seconds: goal.elapsed_seconds,
            tokens: goal.tokens,
        }),
    };
    Some(t::Item {
        id: t::ItemId(key.item_id),
        turn: t::TurnId(value.turn_id),
        parent: value
            .parent_call_id
            .as_ref()
            .map(|id| call_id(key.history_epoch, id)),
        trimmed: value.trimmed,
        body,
    })
}

pub fn goal(value: &p::GoalState) -> chat::goal::Goal {
    chat::goal::Goal {
        objective: value.objective.clone(),
        status: match value.state.as_str() {
            "paused" => chat::goal::GoalStatus::Paused,
            "blocked" => chat::goal::GoalStatus::Blocked,
            "usage_limited" => chat::goal::GoalStatus::UsageLimited,
            "budget_limited" => chat::goal::GoalStatus::BudgetLimited,
            "set" => chat::goal::GoalStatus::Set,
            _ => chat::goal::GoalStatus::Pursuing,
        },
        since: value.since_unix.unwrap_or_else(chat::goal::now),
        tokens: value.tokens,
        budget: value.budget,
        seconds: value.elapsed_seconds,
        turns: value.turns,
        checks: value.checks,
        last_check: value.note.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn html_page_metadata_survives_the_host_projection_without_bypassing_its_authorized_read() {
        let input = json!({"title":"Przycisk z hoverem", "height":180, "html":"<button>Najedź na mnie</button>"});
        let local = t::Item {
            id: t::ItemId(8),
            turn: t::TurnId(2),
            parent: None,
            trimmed: false,
            body: t::Body::Tool(t::ToolCall {
                call_id: "page".into(),
                name: chat::page::SHOW_TOOL.into(),
                kind: Some(chat::backend::ToolKind::Other),
                input: input.clone(),
                input_partial: String::new(),
                state: t::ToolState::Succeeded,
                output: None,
                elapsed_seconds: None,
                activity: None,
                background: false,
            }),
        };
        let projected = crate::remote::transcript::item(
            &local,
            4,
            1,
            2,
            None,
            std::path::Path::new("/"),
            &Default::default(),
        )
        .unwrap();
        let restored = item(&projected).unwrap();
        let t::Body::Tool(tool) = restored.body else {
            panic!("missing tool")
        };
        let page = chat::page::Page::from_input(&tool.input).unwrap();
        let original = chat::page::Page::from_input(&input).unwrap();
        assert_eq!(page.title, original.title);
        assert_eq!(page.height, original.height);
        assert_ne!(page.html, original.html);
        assert!(page.html.contains("Loading"));
    }
    #[test]
    fn remote_semantics_use_the_same_cards_without_trusting_a_tool_name() {
        let wire = p::TranscriptItem {
            key: Some(p::ItemKey {
                history_epoch: 4,
                item_id: 9,
            }),
            parent_call_id: Some("parent".into()),
            body: Some(p::transcript_item::Body::Tool(p::ToolCall {
                call_id: "tool".into(),
                original_name: "Bash".into(),
                kind: p::ToolKind::Other as i32,
                state: "succeeded".into(),
                generic_input: Some("{\"question\":\"value\"}".into()),
                ..Default::default()
            })),
            ..Default::default()
        };
        let value = item(&wire).unwrap();
        assert_eq!(value.parent.as_deref(), Some("4:parent"));
        let t::Body::Tool(tool) = value.body else {
            panic!("tool");
        };
        assert_eq!(tool.semantic_kind(), chat::backend::ToolKind::Other);
        assert!(matches!(
            chat::tools::view(&tool),
            chat::tools::ToolView::Generic { .. }
        ));
    }
    #[test]
    fn projection_updates_preserve_row_identity_and_detect_history_prepend() {
        let value = |id, text: &str| p::TranscriptItem {
            key: Some(p::ItemKey {
                history_epoch: 2,
                item_id: id,
            }),
            body: Some(p::transcript_item::Body::Text(p::TextBlock {
                text: text.into(),
                streaming: true,
            })),
            ..Default::default()
        };
        let mut transcript = t::Transcript::default();
        assert!(!transcript.sync_projection(vec![item(&value(8, "hel")).unwrap()]));
        transcript.take_changes();
        assert!(!transcript.sync_projection(vec![item(&value(8, "hello")).unwrap()]));
        assert_eq!(transcript.take_changes(), vec![t::Change::Updated(0)]);
        assert!(transcript.sync_projection(vec![
            item(&value(7, "earlier")).unwrap(),
            item(&value(8, "hello")).unwrap()
        ]));
        assert_eq!(transcript.items()[1].id, t::ItemId(8));
    }
    #[test]
    fn a_file_and_an_image_keep_their_host_reference_in_the_shared_composer() {
        let a = p::Attachment {
            label: "picture".into(),
            content: Some(p::attachment::Content::Image(p::ArtifactRef {
                artifact_id: "image-id".into(),
                media_type: "image/png".into(),
                ..Default::default()
            })),
        };
        assert!(
            matches!(attachment(&a),Some(t::Attachment::Image {file,data:None,..}) if file=="remote:image-id")
        );
        let end = |saved, fork| p::TranscriptItem {
            key: Some(p::ItemKey {
                history_epoch: 2,
                item_id: 10,
            }),
            body: Some(p::transcript_item::Body::TurnEnd(p::TurnEnd {
                outcome: "completed".into(),
                saved,
                fork_available: Some(fork),
                ..Default::default()
            })),
            ..Default::default()
        };
        for wire in [end(false, true), end(true, false)] {
            assert!(matches!(
                item(&wire).unwrap().body,
                t::Body::TurnEnd(t::TurnEnd {
                    resume_at: None,
                    ..
                })
            ));
        }
        assert!(matches!(
            item(&end(true, true)).unwrap().body,
            t::Body::TurnEnd(t::TurnEnd {
                resume_at: Some(_),
                ..
            })
        ));
    }
}
