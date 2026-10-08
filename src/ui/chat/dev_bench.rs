//! DEV ONLY — chat benchmark. A composer button that queues prompts exercising the
//! whole chat: thinking, code blocks, file pills, tool cards and diffs, turn change
//! summaries, "Worked for …", subagents (foreground and background), long output,
//! the queue tray and cleanup. Each prompt starts with what the tester should check.
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

fn prompts() -> [String; 6] {
    let f = FOLDER;
    [
        format!(
            "[Canopy benchmark 1/6 — thinking, files, code blocks, file pills]\n\
             Tester checks: live Thinking… then \"Thought for Ns\", Write cards with created diffs, \"Show N more lines\", \
             file pills with icons, code cards with language/Copy/line numbers, the change summary with folders, \"Worked for …\".\n\n\
             Think hard about the file layout before acting. Create these files, one Write call each:\n\
             - `{f}/src/core/util.py`: four small functions (`add`, `slug`, `chunks`, `clamp`) with docstrings, about 40 lines.\n\
             - `{f}/src/web/app.ts`: a TypeScript module exporting an interface and two functions, about 25 lines.\n\
             - `{f}/docs/notes.md`: a Markdown note with a heading and a list.\n\
             - `{f}/data.json`: a JSON object with three keys.\n\
             - `{f}/Cargo.toml`: a minimal package manifest.\n\
             Then reply with:\n\
             1. Every file as inline code; `{f}/src/core/util.py` with a line number like `{f}/src/core/util.py:12`; \
             the absolute path of `{f}/src/web/app.ts` in plain text.\n\
             2. Code blocks tagged `rust`, `tsx`, `python`, `bash`, `json`, and one untagged block.\n\
             3. One code block whose single line is longer than 200 characters.\n\
             Keep the prose short."
        ),
        format!(
            "[Canopy benchmark 2/6 — Read, Edit diffs, Bash]\n\
             Tester checks: Read card with \"lines X–Y of Z\", Edit diffs (one number column), Bash stdout / stderr / failure ✗ / \
             empty output (not expandable), status icons on one right edge, change summary + Open diff pane.\n\n\
             One tool call at a time:\n\
             1. Read `{f}/src/core/util.py` fully, then Read it again with offset 5 and limit 10.\n\
             2. Edit `{f}/src/core/util.py` twice: rename `chunks` to `batched`; add type hints to `add` and `clamp`.\n\
             3. Edit `{f}/src/web/app.ts`: add a third exported function.\n\
             4. Rewrite `{f}/docs/notes.md` completely with Write (new heading, four bullet points).\n\
             5. Create `{f}/src/core/model.rs` with a Rust struct and an impl block.\n\
             6. Bash: `ls -la {f}/src/core`.\n\
             7. Bash: `cat {f}/missing.txt` (fails).\n\
             8. Bash: `ls {f} {f}/nope` (output and an error).\n\
             9. Bash: `touch {f}/empty.flag` (prints nothing).\n\
             Reply in two sentences."
        ),
        format!(
            "[Canopy benchmark 3/6 — subagents]\n\
             Tester checks: two subagent cards grouped in one row (stacked icons, expands to one row each), click a card to open \
             its read-only conversation and go back, \"Waiting on subagent\" rows above the composer with Stop, the reports.\n\n\
             Launch two Explore subagents in parallel in the background (run_in_background true):\n\
             - A: read every file under `{f}/src` and report every function and method name.\n\
             - B: count the lines of every file in `{f}/` with Bash and report a table.\n\
             While they run, use one foreground general-purpose subagent that edits `{f}/data.json` to add the key \"benchmark\": true.\n\
             Wait for all three, then summarize their reports in a short table."
        ),
        ("[Canopy benchmark 4/6 — long output, web, files outside the folder]\n\
             Tester checks: long Bash output clipped with \"Show N more lines\", WebFetch card, a pill for a file outside the chat \
             folder that opens it in its own folder.\n\n\
             1. Bash: `seq 1 600`.\n\
             2. Fetch https://example.com with WebFetch and quote its title.\n\
             3. Read `/etc/shells` and mention it as an absolute path in your reply.\n\
             Reply with a numbered list.")
            .to_owned(),
        ("[Canopy benchmark 5/6 — short answer]\n\
             Tester checks: a turn with no tools has no \"Worked for …\" and no change summary.\n\n\
             Without using any tools, answer in one sentence: what is a git worktree?")
            .to_owned(),
        format!(
            "[Canopy benchmark 6/6 — cleanup]\n\
             Tester checks: deleted files are not counted as edits; the failing `ls` shows ✗.\n\n\
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
