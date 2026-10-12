//! The conversation so far for an agent switched to (DEC-062): built by Canopy from the
//! transcript — messages, final replies with who wrote them, changed files, the last
//! plan — without tool output or thinking, within a size limit.
use super::tools::turn_changes;
use super::transcript::{Attachment, Body, Delivery, Item, Outcome, Transcript, TurnId};
use serde_json::Value;

/// Upper bound of the summary, in bytes.
pub const LIMIT: usize = 24 * 1024;
/// Older turns keep this much of each reply and message.
const SHORT_REPLY: usize = 600;
const SHORT_MESSAGE: usize = 400;
/// Turns kept in full, newest first, while the budget allows.
const FULL_SHARE: f32 = 0.7;

/// A part of the conversation and the agent that ran it.
pub struct Part<'a> {
    pub agent: &'a str,
    pub transcript: &'a Transcript,
}

struct Turn {
    agent: String,
    message: String,
    reply: Option<String>,
    files: Vec<String>,
    pages: Vec<String>,
    note: Option<&'static str>,
}

fn cut(text: &str, limit: usize) -> String {
    let text = text.trim();
    match text.char_indices().nth(limit) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

fn turns(parts: &[Part]) -> Vec<Turn> {
    let mut turns: Vec<Turn> = vec![];
    for part in parts {
        let items: Vec<&Item> = part
            .transcript
            .items()
            .iter()
            .filter(|item| item.parent.is_none())
            .collect();
        let mut ids: Vec<TurnId> = vec![];
        for item in &items {
            if !ids.contains(&item.turn) {
                ids.push(item.turn);
            }
        }
        for id in ids {
            let of_turn: Vec<&&Item> = items.iter().filter(|item| item.turn == id).collect();
            let message = of_turn
                .iter()
                .filter_map(|item| match &item.body {
                    Body::User(message) => {
                        let names: Vec<String> = message
                            .attachments
                            .iter()
                            .filter(|a| {
                                matches!(a, Attachment::Image { .. } | Attachment::Path { .. })
                            })
                            .filter_map(|a| a.label().map(str::to_owned))
                            .collect();
                        let steered = message.delivery == Delivery::Steered;
                        let mentioned = names.iter().all(|n| message.text.contains(n.as_str()));
                        let mut text =
                            crate::chat::compose::plain(&message.text, &message.attachments);
                        if !names.is_empty() && !mentioned {
                            text.push_str(&format!(" (attached: {})", names.join(", ")));
                        }
                        Some(if steered {
                            format!("(while the agent worked) {text}")
                        } else {
                            text
                        })
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if message.trim().is_empty() {
                continue;
            }
            let reply = of_turn.iter().rev().find_map(|item| match &item.body {
                Body::Text(block) if !block.text.trim().is_empty() => Some(block.text.clone()),
                _ => None,
            });
            let note = of_turn.iter().find_map(|item| match &item.body {
                Body::TurnEnd(end) => match end.outcome {
                    Outcome::Interrupted => Some("The turn was stopped before it finished."),
                    Outcome::Failed(_) => Some("The turn failed."),
                    Outcome::Limit(_) => Some("The turn hit a limit."),
                    Outcome::Completed => None,
                },
                _ => None,
            });
            let files = turn_changes(part.transcript, id)
                .into_iter()
                .map(|file| {
                    format!(
                        "{} (+{} −{})",
                        file.path.display(),
                        file.added,
                        file.removed
                    )
                })
                .collect();
            // Pages the agent showed: their titles, not their HTML.
            let pages = of_turn
                .iter()
                .filter_map(|item| match &item.body {
                    Body::Tool(tool)
                        if tool.name == crate::chat::page::SHOW_TOOL
                            && tool.state == crate::chat::transcript::ToolState::Succeeded =>
                    {
                        crate::chat::page::Page::from_input(&tool.input)
                            .ok()
                            .map(|page| page.title)
                    }
                    _ => None,
                })
                .collect();
            turns.push(Turn {
                agent: part.agent.to_owned(),
                message,
                reply,
                files,
                pages,
                note,
            });
        }
    }
    turns
}

/// The last plan (steps with their status) in the parts, if any.
fn plan(parts: &[Part]) -> Option<String> {
    let todos = parts.iter().rev().find_map(|part| {
        part.transcript
            .items()
            .iter()
            .rev()
            .find_map(|item| match &item.body {
                Body::Tool(tool) if tool.name == "TodoWrite" => tool.input.get("todos").cloned(),
                _ => None,
            })
    })?;
    let lines: Vec<String> = todos
        .as_array()?
        .iter()
        .filter_map(|todo| {
            let content = todo.get("content").and_then(Value::as_str)?;
            let mark = match todo.get("status").and_then(Value::as_str) {
                Some("completed") => "[x]",
                Some("in_progress") => "[~]",
                _ => "[ ]",
            };
            Some(format!("- {mark} {content}"))
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn render(turn: &Turn, short: bool) -> String {
    let mut out = format!(
        "### User\n{}\n",
        if short {
            cut(&turn.message, SHORT_MESSAGE)
        } else {
            turn.message.trim().to_owned()
        }
    );
    if let Some(reply) = &turn.reply {
        out.push_str(&format!(
            "### {}\n{}\n",
            turn.agent,
            if short {
                cut(reply, SHORT_REPLY)
            } else {
                reply.trim().to_owned()
            }
        ));
    }
    if !turn.files.is_empty() {
        out.push_str(&format!("Changed files: {}\n", turn.files.join(", ")));
    }
    if !turn.pages.is_empty() {
        out.push_str(&format!("Showed pages: {}\n", turn.pages.join(", ")));
    }
    if let Some(note) = turn.note {
        out.push_str(&format!("({note})\n"));
    }
    out
}

/// The summary for `target` (the agent switched to): everything in `parts`, or with
/// `since` the part of `since` agent's absence only. `None` when nothing happened.
pub fn summary(parts: &[Part], target: &str, since: bool) -> Option<String> {
    let turns = turns(parts);
    if turns.is_empty() {
        return None;
    }
    let intro = if since {
        format!(
            "You ({target}) worked on this conversation in Canopy earlier. Meanwhile the user \
             continued it with another agent; this is what happened since you left, in order. \
             Tool results aren't included: read files again when you need them."
        )
    } else {
        format!(
            "The user switched to you ({target}) in the middle of this conversation in Canopy. \
             This is what happened before, in order; continue from it. Earlier agents' tool \
             results aren't included: read files again when you need them."
        )
    };
    let plan = plan(parts).map(|plan| format!("### Latest plan\n{plan}\n"));
    let budget = LIMIT.saturating_sub(intro.len() + plan.as_ref().map_or(0, String::len) + 64);
    // Newest first: full while most of the budget is left, then shortened, then dropped.
    let mut kept: Vec<String> = vec![];
    let mut used = 0;
    let mut omitted = 0;
    for turn in turns.iter().rev() {
        let full = render(turn, false);
        let text = if (used + full.len()) as f32 <= budget as f32 * FULL_SHARE {
            full
        } else {
            render(turn, true)
        };
        if used + text.len() > budget {
            omitted += 1;
            continue;
        }
        used += text.len();
        kept.push(text);
    }
    kept.reverse();
    let mut out = format!("<canopy-context>\n{intro}\n\n");
    if omitted > 0 {
        out.push_str(&format!("({omitted} earlier turns omitted)\n\n"));
    }
    out.push_str(&kept.join("\n"));
    if let Some(plan) = plan {
        out.push_str(&format!("\n{plan}"));
    }
    out.push_str("</canopy-context>");
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::transcript::{
        Item, ItemId, TextBlock, ToolCall, ToolOutput, ToolState, TurnEnd, UserMessage,
    };
    use serde_json::json;

    fn transcript(turns: &[(&str, &str)]) -> Transcript {
        let mut t = Transcript::default();
        for (message, reply) in turns {
            let turn = t.allocate_turn();
            t.push_in(
                turn,
                None,
                Body::User(UserMessage {
                    text: (*message).into(),
                    delivery: Delivery::Sent,
                    uuid: None,
                    attachments: vec![],
                    context: None,
                    goal: false,
                }),
            );
            t.push_in(
                turn,
                None,
                Body::Text(TextBlock {
                    text: (*reply).into(),
                    streaming: false,
                }),
            );
            t.push_in(
                turn,
                None,
                Body::TurnEnd(TurnEnd {
                    outcome: Outcome::Completed,
                    usage: None,
                    cost_usd: None,
                    duration_ms: None,
                    resume_at: None,
                    quiet: false,
                }),
            );
        }
        t
    }

    #[test]
    fn quoted_text_reaches_the_next_agent_as_a_quote() {
        let quote = crate::chat::quotes::quote("KIWI is the word").unwrap();
        let mut t = transcript(&[("placeholder", "Noted.")]);
        let mut items = t.items().to_vec();
        if let Body::User(message) = &mut items[0].body {
            message.text = format!("{} why?", quote.label().unwrap());
            message.attachments = vec![quote];
        }
        t = Transcript::restore(items);
        let summary = summary(
            &[Part {
                agent: "Claude Code",
                transcript: &t,
            }],
            "Codex",
            false,
        )
        .unwrap();
        assert!(summary.contains("> KIWI is the word\n\nwhy?"), "{summary}");
        assert!(!summary.contains("❝") && !summary.contains("attached:"));
    }

    #[test]
    fn parts_become_turns_with_their_agents_files_and_plan() {
        let claude = transcript(&[("Remember KIWI", "Noted.")]);
        let mut codex = transcript(&[("Create a.txt", "Created it.")]);
        let turn = codex.items()[0].turn;
        codex.push_in(
            turn,
            None,
            Body::Tool(ToolCall {
                call_id: "c1".into(),
                name: "Write".into(),
                input: json!({"file_path": "/r/a.txt"}),
                input_partial: String::new(),
                state: ToolState::Succeeded,
                output: Some(ToolOutput {
                    text: String::new(),
                    is_error: false,
                    structured: Some(
                        json!({"type":"create","filePath":"/r/a.txt","content":"x\n"}),
                    ),
                }),
                elapsed_seconds: None,
                activity: None,
                background: false,
            }),
        );
        codex.push_in(
            turn,
            None,
            Body::Tool(ToolCall {
                call_id: "p".into(),
                name: "TodoWrite".into(),
                input: json!({"todos": [{"content": "Write tests", "status": "in_progress"}]}),
                input_partial: String::new(),
                state: ToolState::Succeeded,
                output: None,
                elapsed_seconds: None,
                activity: None,
                background: false,
            }),
        );
        let parts = [
            Part {
                agent: "Claude Code",
                transcript: &claude,
            },
            Part {
                agent: "Codex",
                transcript: &codex,
            },
        ];
        let text = summary(&parts, "Claude Code", false).expect("summary");
        assert!(text.starts_with("<canopy-context>") && text.ends_with("</canopy-context>"));
        let kiwi = text.find("Remember KIWI").unwrap();
        let created = text.find("Created it.").unwrap();
        assert!(kiwi < created, "in order");
        assert!(text.contains("### Codex\nCreated it."));
        assert!(text.contains("Changed files: /r/a.txt (+1 −0)"));
        assert!(text.contains("- [~] Write tests"));
        assert!(summary(&[], "Codex", false).is_none());
        let _ = ItemId(0);
        let _: Option<&Item> = None;
    }

    #[test]
    fn long_conversations_fit_the_limit_newest_first() {
        let turns: Vec<(String, String)> = (0..200)
            .map(|n| {
                (
                    format!("question {n}"),
                    format!("answer {n} {}", "x".repeat(2000)),
                )
            })
            .collect();
        let refs: Vec<(&str, &str)> = turns
            .iter()
            .map(|(q, a)| (q.as_str(), a.as_str()))
            .collect();
        let long = transcript(&refs);
        let text = summary(
            &[Part {
                agent: "Codex",
                transcript: &long,
            }],
            "Claude Code",
            true,
        )
        .expect("summary");
        assert!(text.len() <= LIMIT + 256, "{}", text.len());
        assert!(text.contains("question 199") && !text.contains("question 0\n"));
        assert!(text.contains("earlier turns omitted"));
        assert!(text.contains("since you left"));
    }
}
