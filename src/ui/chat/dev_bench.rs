//! DEV ONLY — chat benchmark. Composer buttons that queue prompts exercising what the
//! current stage adds, and load a long recorded transcript for profiling. Each prompt
//! starts with what the tester should check.
//!
//! Remove with this file plus the two lines marked `dev_bench` in `mod.rs` and
//! `composer.rs`.
use super::view::ChatView;
use crate::ui::components::*;
use gpui_kit::component::IconName;
use gpui_kit::*;

/// Everything the agent creates lives here, inside the chat's folder; the last prompt
/// deletes it.
const FOLDER: &str = "canopy-bench";

fn prompts() -> [String; 3] {
    let f = FOLDER;
    [
        format!(
            "[Canopy benchmark 1/3 — history after restart]\n\
             Tester checks: after this turn quit Canopy (⌘Q) and start it again: the whole conversation is \
             back (folded turn, subagent card, change summary); the subagent and \"Open diff\" open; then ask \
             \"Which fruit did I mention?\" — the answer must be MANGO.\n\n\
             Remember the word MANGO. Use the Write tool to create `{f}/notes.txt` containing exactly `v1`, then \
             use the Task tool with a general-purpose subagent to list the files in `{f}/`. Reply with one sentence."
        ),
        "[Canopy benchmark 2/3 — attachments]\n\
         Tester checks: before this runs, Edit it in the queue and attach: a screenshot (⌘⇧⌃4, then ⌘V), \
         an image dragged from Finder, and a file or folder via the paperclip; thumbnails show above the \
         field, X removes one; after sending, the thumbnails sit under your message and open full size.\n\n\
         Describe each attached image in one sentence and name each attached file or folder. If nothing \
         is attached, say so."
            .to_owned(),
        format!(
            "[Canopy benchmark 3/3 — navigator, New session, cleanup]\n\
             Tester checks: the marks on the left (one per message, the one you read stands out); hover grows \
             them and previews the message; click and ⌥⌘↑/⌥⌘↓ glide to messages. After this turn press ⌘K \
             (New session) and ask \"Which fruit did I mention?\" — the agent must not know; the earlier \
             conversation stays above \"New session\".\n\n\
             Delete the `{f}/` folder with Bash (`rm -rf {f}`). Reply with a checklist of what this benchmark exercised."
        ),
    ]
}

/// Recorded sessions replayed into a long offline conversation for profiling.
const RECORDED: [&str; 3] = [
    include_str!("../../../tests/fixtures/chat/claude-2.1.294/tools.jsonl"),
    include_str!("../../../tests/fixtures/chat/claude-2.1.294/thinking.jsonl"),
    include_str!("../../../tests/fixtures/chat/claude-2.1.294/parallel-subagents.jsonl"),
];
const ROUNDS: usize = 60;

/// A long conversation built offline from recorded sessions (no CLI process).
pub(super) fn long_session() -> canopy_desktop::chat::session::ChatSession {
    use canopy_desktop::chat::{claude::wire, session::ChatSession};
    let (mut session, _) = ChatSession::new();
    for round in 0..ROUNDS {
        for (index, recorded) in RECORDED.iter().enumerate() {
            session.submit(&format!("Long transcript, round {round}, part {index}"));
            let idle = r#"{"type":"system","subtype":"session_state_changed","state":"idle"}"#;
            // Each recording ends idle, so the next round starts a turn of its own.
            for line in recorded.lines().chain([idle]) {
                // Every round gets its own message and tool ids, or it would update
                // the first round's rows.
                let line = line
                    .replace("msg_", &format!("msg_r{round}_"))
                    .replace("toolu_", &format!("toolu_r{round}_"));
                if let Ok(Some(message)) = wire::parse_line(line.as_bytes()) {
                    session.on_message(message);
                }
            }
        }
    }
    session
}

impl ChatView {
    /// Replaces this chat's session with a long offline one, to profile scrolling and
    /// rendering. Restart brings the real session back.
    fn load_long_transcript(&mut self, cx: &mut Context<Self>) {
        let stop = self.shutdown(cx);
        cx.spawn(async move |this, cx| {
            let _ = stop.await;
            let _ = this.update(cx, |this, cx| {
                let started = std::time::Instant::now();
                this.session = Some(long_session());
                // Not part of the chat's saved history.
                this.history_off = true;
                this.rows = super::rows::Rows::new(cx).in_folder(this.rows.cwd.clone());
                this.threads.clear();
                this.after_change(cx);
                eprintln!(
                    "dev_bench: long transcript ({} items, {} rows) built in {:?}",
                    this.session
                        .as_ref()
                        .map_or(0, |s| s.transcript().items().len()),
                    this.rows.rows.len(),
                    started.elapsed()
                );
            });
        })
        .detach();
    }

    /// Sends the first prompt (or queues it behind running work) and queues the rest.
    fn queue_benchmark(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for prompt in prompts() {
            self.composer
                .update(cx, |state, cx| state.set_value(prompt, window, cx));
            self.submit(window, cx);
        }
    }
}

pub(super) fn button(cx: &mut Context<ChatView>, disabled: bool) -> Div {
    use gpui_kit::base::Disableable;
    row()
        .gap(px(2.))
        .child(
            icon_button(
                "chat-dev-long",
                IconName::FileText,
                "Load a long recorded transcript for profiling (dev)",
            )
            .on_click(cx.listener(|this, _, _, cx| this.load_long_transcript(cx))),
        )
        .child(
            icon_button(
                "chat-dev-bench",
                IconName::Play,
                "Queue the chat benchmark (dev)",
            )
            .disabled(disabled)
            .on_click(cx.listener(|this, _, window, cx| this.queue_benchmark(window, cx))),
        )
}
