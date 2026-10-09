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
            "[Canopy benchmark 1/3 — message actions]\n\
             Tester checks: hover a reply for Copy (✓ after copying), Retry on the last reply and the turn's \
             numbers (time, tokens; cost only with an API key); hover your message for Copy and Edit; the \
             context ring next to the pickers.\n\n\
             Remember the word APPLE. Then use the Write tool to create `{f}/notes.txt` containing exactly `v1`. \
             Reply with one sentence and a short `rust` code block."
        ),
        format!(
            "[Canopy benchmark 2/3 — edit and restore]\n\
             Tester checks: after this turn, Edit THIS message (hover → pencil): the tray offers to restore \
             1 file; send the edit with restore on and ask \"Which words do you remember?\" — the answer must be \
             APPLE only, `{f}/notes.txt` must be back to `v1`, and the earlier version stays above the separator.\n\n\
             Remember the word BANANA. Then use the Write tool to overwrite `{f}/notes.txt` with exactly `v2`. \
             Reply with one sentence."
        ),
        format!(
            "[Canopy benchmark 3/3 — cleanup]\n\
             Tester checks: the change summary tree, keyboard: Tab through pills, cards and actions, Enter/Space \
             to open them.\n\n\
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
