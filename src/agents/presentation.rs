//! Human-facing labels only. Hook matching, session identity and tool routing keep raw IDs.
fn words(raw: &str) -> String {
    let chars = raw.chars().collect::<Vec<_>>();
    let mut text = String::new();
    for (i, c) in chars.iter().copied().enumerate() {
        if matches!(c, '_' | '-') {
            if !text.ends_with(' ') {
                text.push(' ');
            }
            continue;
        }
        if i > 0
            && c.is_uppercase()
            && (chars[i - 1].is_lowercase()
                || chars[i - 1].is_uppercase()
                    && chars.get(i + 1).is_some_and(|c| c.is_lowercase()))
            && !text.ends_with(' ')
        {
            text.push(' ');
        }
        text.push(c);
    }
    let mut chars = text.trim().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "—".into(),
    }
}
fn brand(raw: &str) -> String {
    match raw {
        "github" => "GitHub".into(),
        "jira" => "Jira".into(),
        "codex" => "Codex".into(),
        "claude" => "Claude Code".into(),
        _ => words(raw),
    }
}
pub fn tool_label(raw: &str) -> String {
    if let Some(mcp) = raw.strip_prefix("mcp__")
        && let Some((server, tool)) = mcp.split_once("__")
    {
        return format!("{} · {}", brand(server), words(tool));
    }
    let tool = raw.strip_prefix("functions.").unwrap_or(raw);
    let known = match tool {
        "Bash" | "bash" | "exec_command" | "shell" | "shell_command" => "Terminal",
        "Read" | "read_file" => "Read file",
        "Write" | "write_file" => "Write file",
        "Edit" | "MultiEdit" | "apply_patch" => "Edit files",
        "Glob" | "glob" => "Find files",
        "Grep" | "grep" | "search_files" => "Search files",
        "WebSearch" | "web_search" => "Search the web",
        "WebFetch" | "web_fetch" => "Fetch webpage",
        "AskUserQuestion" | "request_user_input" | "request_user_input_async" => "Ask a question",
        "TodoWrite" | "update_plan" => "Update plan",
        "Task" | "Agent" | "spawn_agent" => "Start subagent",
        "TaskOutput" | "wait_agent" => "Read subagent progress",
        "TaskStop" | "interrupt_agent" => "Stop subagent",
        "send_message" => "Message subagent",
        "write_stdin" => "Send terminal input",
        "BashOutput" => "Read terminal output",
        "KillShell" => "Stop terminal command",
        "exec" => "Run tools",
        "web.run" => "Browse the web",
        "EnterPlanMode" => "Enter plan mode",
        "ExitPlanMode" => "Finish planning",
        "Skill" => "Use skill",
        "NotebookEdit" => "Edit notebook",
        "ToolSearch" => "Find tools",
        "view_image" => "View image",
        _ => return words(tool),
    };
    known.into()
}
pub fn event_label(raw: &str) -> String {
    match raw {
        "SessionStart" => "Session started",
        "SessionEnd" => "Session ended",
        "UserPromptSubmit" => "Prompt received",
        "PreToolUse" => "Tool started",
        "PostToolUse" => "Tool completed",
        "PostToolUseFailure" => "Tool stopped or failed",
        "PermissionRequest" => "Permission requested",
        "PermissionDenied" => "Permission declined",
        "Stop" => "Turn completed",
        "Notification" => "Agent notification",
        "SubagentStart" => "Subagent started",
        "SubagentStop" => "Subagent completed",
        "PreCompact" => "Compacting context",
        _ => return words(raw),
    }
    .into()
}
pub fn mode_label(raw: &str) -> String {
    match raw {
        "auto" => "Automatic",
        "acceptEdits" => "Accept edits",
        "bypassPermissions" => "Skip permission prompts",
        "plan" => "Plan mode",
        "default" => "Default permissions",
        "workspace-write" => "Workspace write",
        "read-only" => "Read only",
        "danger-full-access" => "Full access",
        _ => return words(raw),
    }
    .into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_cover_both_agents_and_keep_custom_tools_distinguishable() {
        assert_eq!(tool_label("Bash"), "Terminal");
        assert_eq!(tool_label("functions.exec_command"), "Terminal");
        assert_eq!(tool_label("AskUserQuestion"), "Ask a question");
        assert_eq!(tool_label("functions.request_user_input"), "Ask a question");
        assert_eq!(
            tool_label("mcp__github__list_issues"),
            "GitHub · List issues"
        );
        assert_eq!(tool_label("MyCustomTool"), "My Custom Tool");
        assert_eq!(tool_label("get_HTTP_response"), "Get HTTP response");
        assert_eq!(event_label("PreToolUse"), "Tool started");
        assert_eq!(mode_label("acceptEdits"), "Accept edits");
    }
}
