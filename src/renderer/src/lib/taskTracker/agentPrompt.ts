// One-shot prompt the user pastes into an AI agent session. Running it makes the agent persist
// Canopy's branch/PR conventions in its own instruction file. The rules deliberately REFERENCE
// `.canopy/config.json` instead of copying its values, so later config edits take effect without
// regenerating anything.
export function buildAgentSetupPrompt(): string {
  return `Add a "Canopy project conventions" section to your agent instructions file (CLAUDE.md, AGENTS.md, or the equivalent for your tooling) containing the rules below, then follow them in every task in this repository.

1. The single source of truth for branch and pull-request conventions is \`.canopy/config.json\` at the repository root (each git worktree carries its own copy — use the one in the worktree you are working in). Re-read that file every time before creating a branch or a PR. Do NOT copy its concrete values into your instructions — reference the file, so configuration changes apply immediately.
2. Branch names: render \`branchTemplate.template\` (placeholders: {branchType}, {taskKey}, {taskTitle} slugified to lowercase-with-dashes). Map the task type to {branchType} via \`branchTemplate.typeMapping\`.
3. Pull requests: render the title from \`prTemplate.titleTemplate\` and the description from \`prTemplate.bodyTemplate\` (placeholders: {taskKey}, {taskTitle}, {taskType}, {taskUrl}, {boardKey}).
4. PR target branch: \`prTemplate.defaultTargetBranch\`, unless an entry in \`prTemplate.targetRules\` matches the task type.
5. \`boardOverrides\` take precedence over the root templates for tasks from that board.
6. Task data (key, title, type, URL) comes from the tracker(s) configured in the \`trackers\` array of that file; the active task key is usually embedded in the current branch name (e.g. ABC-123).
7. The user may explicitly override any of the above (branch name, PR title, description or target) when asking you to create a branch or PR — an explicit user request always wins over the templates.
8. You MUST NOT modify \`.canopy/config.json\` or any other Canopy configuration file — they are user-managed. Also follow any additional guidance in the \`agents.instructions\` array of that file.`
}
