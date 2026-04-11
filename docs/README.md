# Canopy documentation

Feature behavior specs for code review and bug triage. Each doc describes what the feature does from the user's perspective, expected behavior as user-story workflows, error states, and configuration.

Cross-cutting patterns (IPC conventions, error handling, theming, state management) are in [architecture.md](architecture.md).

## Document template

New feature docs follow this structure:

```
# Feature name
> One-sentence purpose.

Status / Introduced / Platforms

## Overview          — bug triage reference (what "working" means)
## Behavior          — code review reference (numbered user-story steps)
## Configuration     — user-configurable options, config paths
## Error states      — table: error tag, user message, cause
## Security/privacy  — only if the feature handles sensitive data
## Source files      — directory pointers
```

## Core

Terminal, git, and worktree management. Shared by all users.

| Doc                             | Description                                                                      |
| ------------------------------- | -------------------------------------------------------------------------------- |
| [terminal.md](core/terminal.md) | PTY sessions, WebSocket streaming, tmux integration, terminal themes             |
| [git.md](core/git.md)           | Repository detection, file watching, commit/push/pull, branching, diffs, staging |
| [worktree.md](core/worktree.md) | Multiple branches in parallel directories, automated post-creation setup         |

## Integrations

Connections to external services and tools.

| Doc                                             | Description                                                            |
| ----------------------------------------------- | ---------------------------------------------------------------------- |
| [task-tracker.md](integrations/task-tracker.md) | Jira, YouTrack, GitHub Issues: browse tasks, create branches, open PRs |
| [agents.md](integrations/agents.md)             | Claude Code, Codex, Gemini CLI, OpenCode: normalized session tracking  |
| [github.md](integrations/github.md)             | PR status per branch, repo identity from remotes, PR creation          |
| [browser.md](integrations/browser.md)           | Sandboxed web pages, DevTools, device emulation, credential autofill   |

## Features

Opt-in capabilities and UI extensions.

| Doc                                                     | Description                                               |
| ------------------------------------------------------- | --------------------------------------------------------- |
| [run-configurations.md](features/run-configurations.md) | Project commands from `.canopy/run.toml`, pre/post hooks  |
| [remote-control.md](features/remote-control.md)         | Mirror/control from another device over LAN via WebRTC    |
| [notch-overlay.md](features/notch-overlay.md)           | Agent status overlay anchored to the screen's top edge    |
| [onboarding.md](features/onboarding.md)                 | First-launch wizard and post-update feature introductions |

## Diagnostics

Monitoring, crash detection, and system health.

| Doc                                                  | Description                                               |
| ---------------------------------------------------- | --------------------------------------------------------- |
| [telemetry.md](diagnostics/telemetry.md)             | One daily ping, no personal data, opt-out in Settings     |
| [crash-reporting.md](diagnostics/crash-reporting.md) | Local-only crash diagnostics, GitHub issue filing dialog  |
| [perf-hud.md](diagnostics/perf-hud.md)               | Real-time CPU/RAM in status bar, zero overhead when off   |
| [file-watcher.md](diagnostics/file-watcher.md)       | Recursive filesystem monitoring with configurable ignores |
