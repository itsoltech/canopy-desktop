# Auto-label instructions

You are labeling a GitHub issue or pull request for Canopy, an Electron + Svelte 5 desktop app.

## Available labels

Type (pick one):

- `bug` — something is broken
- `enhancement` — new feature or improvement
- `documentation` — docs changes
- `security` — security hardening or vulnerability
- `type:refactor` — code restructuring, no behavior change
- `type:build` — build system, electron-vite, CI
- `type:perf` — performance improvement

Area (pick all that apply):

- `area:terminal` — PTY, shell, terminal emulator
- `area:browser` — built-in browser
- `area:ai` — AI/Claude integration
- `area:git` — Git and worktree features
- `area:sidebar` — sidebar navigation
- `area:palette` — command palette
- `area:preferences` — settings/preferences
- `area:dashboard` — dashboard view

Scope (pick all that apply, for PRs only):

- `scope:main` — main process (Node.js, IPC, window management)
- `scope:renderer` — renderer / Svelte UI
- `scope:preload` — preload bridge

## Rules

1. Read the issue/PR title and body to determine labels.
2. For PRs, also read the diff (`gh pr diff`) to determine scope labels from changed file paths:
   - `src/main/` -> `scope:main`
   - `src/renderer/` -> `scope:renderer`
   - `src/preload/` -> `scope:preload`
3. Always apply exactly one type label. If unclear, skip the type label rather than guess.
4. Apply area labels only when the content clearly maps to a specific area. Multiple area labels are fine.
5. Do not apply `claude:review:*` labels. Those are managed by the code review workflow.
6. Do not remove existing labels that were set manually.
7. Apply all labels in a single `gh` command.

## How to apply

```bash
gh issue edit $NUMBER --add-label "bug,area:terminal"
# or for PRs
gh pr edit $NUMBER --add-label "enhancement,area:git,scope:main"
```
