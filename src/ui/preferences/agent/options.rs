use super::*;
/// One option catalog for initialization and profile switches.
pub(super) fn choices(key: &str, agent: Agent) -> Vec<SelectOption> {
    let pairs: &[(&str, &str)] = match key {
        "permission" => &[
            ("", "Default"),
            ("plan", "Plan"),
            ("auto", "Auto"),
            ("acceptEdits", "Accept edits"),
            ("bypassPermissions", "Bypass permissions"),
        ],
        "effort" => &[
            ("", "Default"),
            ("low", "Low"),
            ("medium", "Medium"),
            ("high", "High"),
            ("xhigh", "Extra high"),
            ("max", "Max"),
        ],
        "provider" => &[
            ("", "Default (Anthropic)"),
            ("bedrock", "AWS Bedrock"),
            ("vertex", "Google Vertex AI"),
            ("foundry", "Microsoft Foundry"),
        ],
        "approval" if agent == Agent::Gemini => &[
            ("", "Default"),
            ("default", "Prompt"),
            ("auto_edit", "Auto edit"),
            ("yolo", "YOLO"),
            ("plan", "Plan (read-only)"),
        ],
        "approval" => &[
            ("", "Default"),
            ("on-request", "On request"),
            ("never", "Never"),
        ],
        "sandbox" => &[
            ("", "Default"),
            ("read-only", "Read only"),
            ("workspace-write", "Workspace write"),
            ("danger-full-access", "Full access"),
        ],
        _ => unreachable!("unknown agent select"),
    };
    pairs
        .iter()
        .map(|(value, label)| SelectOption::new(*value, *label))
        .collect()
}
