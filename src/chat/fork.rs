//! Forking a conversation into a new chat (DEC-092/093): its saved parts up to an agent
//! reply's turn end, the last part set to branch its agent's session off at that point.
use super::history::SavedEpoch;
use super::transcript::{Attachment, Body, ItemId};

/// Where a fork is cut: the turn end of the chosen reply, in its session (`epoch`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cut {
    pub epoch: u64,
    pub end: ItemId,
    /// The provider's point to branch at (`TurnEnd::resume_at`).
    pub at: String,
}

/// The parts a fork starts with: every earlier session whole, the cut one up to its
/// turn end (its subagents' output included), nothing after. The last part branches its
/// agent's session off at the cut and carries no goal. `None` when the cut's session or
/// its agent session isn't in `epochs`.
pub fn cut(epochs: Vec<SavedEpoch>, cut: &Cut) -> Option<Vec<SavedEpoch>> {
    let mut kept: Vec<SavedEpoch> = epochs
        .into_iter()
        .filter(|epoch| epoch.index <= cut.epoch)
        .collect();
    let last = kept.last_mut().filter(|last| last.index == cut.epoch)?;
    last.meta.agent.as_ref()?.session.as_ref()?;
    if !last.items.iter().any(|item| item.id == cut.end) {
        return None;
    }
    last.items.retain(|item| item.id.0 <= cut.end.0);
    last.meta.continued = true;
    last.meta.goal = None;
    last.meta.fork_at = Some(cut.at.clone());
    Some(kept)
}

/// The attached images the parts' messages name (files in the chat's folder).
pub fn images(epochs: &[SavedEpoch]) -> Vec<String> {
    let mut files: Vec<String> = epochs
        .iter()
        .flat_map(|epoch| &epoch.items)
        .filter_map(|item| match &item.body {
            Body::User(message) => Some(&message.attachments),
            _ => None,
        })
        .flatten()
        .filter_map(|attachment| match attachment {
            Attachment::Image { file, .. } => Some(file.clone()),
            _ => None,
        })
        .collect();
    files.sort();
    files.dedup();
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::goal::{Goal, GoalStatus};
    use crate::chat::history::{AgentRef, Boundary, EpochMeta};
    use crate::chat::transcript::{
        Delivery, Item, Outcome, TextBlock, TurnEnd, TurnId, UserMessage,
    };

    fn item(id: u64, parent: Option<&str>, body: Body) -> Item {
        Item {
            id: ItemId(id),
            turn: TurnId(1),
            parent: parent.map(str::to_owned),
            body,
            trimmed: false,
        }
    }

    fn text(id: u64, parent: Option<&str>) -> Item {
        item(
            id,
            parent,
            Body::Text(TextBlock {
                text: format!("reply {id}"),
                streaming: false,
            }),
        )
    }

    fn end(id: u64) -> Item {
        item(
            id,
            None,
            Body::TurnEnd(TurnEnd {
                outcome: Outcome::Completed,
                usage: None,
                cost_usd: None,
                duration_ms: None,
                resume_at: Some(format!("m{id}")),
                quiet: false,
            }),
        )
    }

    fn epoch(index: u64, session: Option<&str>, items: Vec<Item>) -> SavedEpoch {
        SavedEpoch {
            index,
            meta: EpochMeta {
                before: (index > 0).then_some(Boundary::Restarted),
                agent: Some(AgentRef {
                    tool: "claude".into(),
                    session: session.map(str::to_owned),
                    ..Default::default()
                }),
                goal: Some(Goal::new("tests pass".into(), GoalStatus::Pursuing, 0)),
                ..Default::default()
            },
            items,
        }
    }

    #[test]
    fn a_fork_keeps_the_conversation_up_to_the_reply() {
        let epochs = vec![
            epoch(0, Some("s0"), vec![text(1, None), end(2)]),
            epoch(
                1,
                Some("s1"),
                vec![
                    text(1, None),
                    text(2, Some("agent")),
                    end(3),
                    text(4, None),
                    end(5),
                ],
            ),
            epoch(2, Some("s2"), vec![text(1, None)]),
        ];
        let cut_at = Cut {
            epoch: 1,
            end: ItemId(3),
            at: "m3".into(),
        };
        let parts = cut(epochs.clone(), &cut_at).expect("fork");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], epochs[0]);
        let last = &parts[1];
        assert_eq!(
            last.items.iter().map(|item| item.id.0).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(last.meta.continued);
        assert_eq!(last.meta.goal, None);
        assert_eq!(last.meta.fork_at.as_deref(), Some("m3"));
        assert_eq!(
            last.meta.agent.as_ref().unwrap().session.as_deref(),
            Some("s1")
        );
        // A part without its agent session, or a turn end not there, can't fork.
        let no_session = vec![epoch(0, None, vec![end(1)])];
        let at = Cut {
            epoch: 0,
            end: ItemId(1),
            at: "m1".into(),
        };
        assert!(cut(no_session, &at).is_none());
        assert!(
            cut(
                epochs,
                &Cut {
                    end: ItemId(9),
                    ..cut_at
                }
            )
            .is_none()
        );
    }

    #[test]
    fn the_branch_point_is_saved_with_the_part_and_old_parts_still_read() {
        use crate::chat::history::meta_json;
        let meta = EpochMeta {
            continued: true,
            fork_at: Some("m3".into()),
            ..Default::default()
        };
        let json = meta_json(&meta);
        assert!(json.contains("\"fork_at\":\"m3\""));
        assert_eq!(serde_json::from_str::<EpochMeta>(&json).unwrap(), meta);
        // Parts saved before forks existed have no point; none is written for them.
        let old: EpochMeta = serde_json::from_str(r#"{"before":null,"continued":false}"#).unwrap();
        assert_eq!(old.fork_at, None);
        assert!(!meta_json(&old).contains("fork_at"));
    }

    #[test]
    fn images_named_by_the_kept_messages() {
        let image = |file: &str| Attachment::Image {
            file: file.into(),
            media_type: "image/png".into(),
            width: 1,
            height: 1,
            data: None,
            label: None,
        };
        let message = item(
            1,
            None,
            Body::User(UserMessage {
                text: "see".into(),
                delivery: Delivery::Sent,
                uuid: None,
                attachments: vec![image("b.png"), image("a.png"), image("b.png")],
                context: None,
                goal: false,
            }),
        );
        assert_eq!(
            images(&[epoch(0, Some("s"), vec![message])]),
            ["a.png", "b.png"]
        );
    }
}
