//! DEV ONLY — chat benchmark. A composer button that queues prompts exercising what the
//! current stage adds (now: permissions, questions and plan approval in the decision
//! panel), then cleans up. Each prompt starts with what the tester should check.
//!
//! Remove with this file plus the two lines marked `dev_bench` in `mod.rs` and
//! `composer.rs`.
use super::view::ChatView;
use crate::ui::components::*;
use gpui_kit::component::{IconName, button::Button};
use gpui_kit::*;

/// Everything the agent creates lives here, inside the chat's folder; the last prompt
/// deletes it.
const FOLDER: &str = "canopy-bench";

fn prompts() -> [String; 2] {
    let f = FOLDER;
    [
        format!(
            "[Canopy benchmark 1/2 — decisions]\n\
             Tester checks (switch the work mode to Ask first): the panel above the composer for a question \
             (single + multi choice, numbers 1–9, ‹/Next/Submit, own answer typed in the composer, Skip), \
             permissions (Allow / Always allow in this chat / Deny with a note typed in the composer, the diff \
             or command preview, \"Replaces the existing file\"), plan approval (Approve / Approve and accept edits / \
             Keep planning with feedback), the trace rows in the conversation, the rules listed in the mode menu and \
             the notch when this chat is hidden.\n\n\
             One step at a time:\n\
             1. Use AskUserQuestion once with two questions: which color I prefer (red, green, blue; single choice) and \
             which languages I use (Rust, TypeScript, Python; several allowed).\n\
             2. Write `{f}/data.json` with a JSON object holding my answers.\n\
             3. Bash: `touch {f}/allow-me.txt`, then the same command `touch {f}/allow-me.txt` again.\n\
             4. Bash: `rm {f}/allow-me.txt` (I may deny it with a note; follow the note).\n\
             5. Write `{f}/data.json` again, adding the key \"checked\": true (it replaces the file).\n\
             6. Enter plan mode (EnterPlanMode) and present a three-step plan to add `{f}/README.md` describing the \
             benchmark (ExitPlanMode). If I ask to keep planning, revise it. Once approved, carry it out.\n\
             Reply with what I answered and what you did."
        ),
        format!(
            "[Canopy benchmark 2/2 — cleanup]\n\
             Tester checks: a permission for a destructive command; the queue tray coming back after decisions.\n\n\
             Delete the `{f}/` folder with Bash (`rm -rf {f}`), then run `ls {f}` to confirm it is gone (it fails).\n\
             Reply with a checklist of everything this benchmark exercised."
        ),
    ]
}

impl ChatView {
    /// Sends the first prompt (or queues it behind running work) and queues the rest.
    fn queue_benchmark(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for prompt in prompts() {
            self.composer
                .update(cx, |state, cx| state.set_value(prompt, window, cx));
            self.submit(window, cx);
        }
    }
}

pub(super) fn button(cx: &mut Context<ChatView>, disabled: bool) -> Button {
    use gpui_kit::base::Disableable;
    icon_button(
        "chat-dev-bench",
        IconName::Play,
        "Queue the chat benchmark (dev)",
    )
    .disabled(disabled)
    .on_click(cx.listener(|this, _, window, cx| this.queue_benchmark(window, cx)))
}
