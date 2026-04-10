<p align="center">
  <img src="resources/icon.png" alt="Canopy desktop app icon" width="128" height="128">
</p>

<h1 align="center">Canopy</h1>

<p align="center"><strong>One canopy. Every branch.</strong></p>

<p align="center">
  A desktop app for developers who run AI coding agents across multiple git worktrees at once.
</p>

<p align="center">
  <sub>AI coding terminal and multi-agent developer workstation for macOS, Windows, and Linux</sub>
</p>

<p align="center">
  <sub>Built at IT SOL, where we run Claude Code across dozens of PR branches daily.</sub>
</p>

<p align="center">
  <a href="https://github.com/itsoltech/canopy-desktop/releases/latest">Download for Free</a> &bull;
  <a href="https://canopy.itsol.tech">Website</a> &bull;
  <a href="https://github.com/itsoltech/canopy-desktop/issues">Issues</a>
</p>

---

<p align="center">
  <img src="docs/screenshot.png" alt="Canopy — Claude Code AI, Git sidebar, and multi-pane terminal in one window" width="800">
</p>

## GPU-accelerated terminal emulator (xterm.js + WebGL)

Canopy uses WebGL-powered rendering with tmux session persistence across restarts. Drag and drop panes to split, reorder, move between groups, or detach into separate windows. Tabs, persistent sessions, and your shell config work out of the box.

## AI coding agents

Claude Code with a real-time Inspector panel tracking costs, context usage, tool calls, and tasks per session. Gemini CLI, Codex, and OpenCode run the same way. Canopy works as a GUI for Claude Code, Gemini CLI, and other AI pair programming tools. Pick a default startup tool per tab or per worktree. AI-powered commit message generation included.

## Git worktree GUI and branch management

Canopy sidebar shows branches, worktrees, and merge status at a glance. Create worktrees from new or existing branches with one click. Push, pull, fetch, stash, and commit without leaving the window. Worktree setup commands run automatically on creation.

## Code review and GitHub

GitHub integration via GraphQL. A diff review panel shows all changes with inline commenting per line. Create pull requests and track their status and CI checks from the sidebar.

## Jira and YouTrack integration

Connect Jira or YouTrack to view boards, sprints, and assigned tasks. Create branches directly from task keys. Task context is sent to agent sessions so agents know what they're working on.

## Built-in browser

Each worktree gets its own browser tab. Device emulation presets for iPhone, iPad, Pixel, Samsung, and custom viewports. Credential storage backed by the OS keyring. Favorites for quick access. Element and screenshot capture feed directly into AI agent context.

## Developer tool for macOS, Windows, and Linux

Multiple projects in one window with persistent layouts that restore after restarts, including window position, size, and state. Native filesystem watcher powers a live file tree in the sidebar. macOS notch overlay shows color-coded agent status. Bottom status bar with toggleable CPU/RAM HUD. Keystroke visualizer with WPM tracking. Command palette and custom tool launcher. Run configurations via `.canopy/run.toml`. WebRTC remote control from mobile via QR pairing (beta).

## Auto-updates and diagnostics

Auto-updates with stable and next (pre-release) channels. Post-update modal shows changelog from GitHub Releases. Crash detection records diagnostics and can file GitHub issues. Guided onboarding on first launch. Anonymous daily telemetry via Umami, opt-in only.

## Multi-agent development workflow

Canopy lets you create a worktree from any branch. Launch an agent in its context. Open more worktrees, run more agents in parallel. Each session gets its own terminal, inspector, and browser tab. Switch between them from one screen.

## Free. No subscription. No account. No middleman.

Canopy is not an editor and not a terminal. It is a workstation for managing AI-powered development across multiple branches simultaneously. It replaces the need to juggle multiple terminal windows when running parallel AI coding sessions. Your API keys, your Claude Code license, your Codex or Gemini setup. You manage them, we don't touch them.

## Download

- **macOS** - DMG or ZIP (code signed and notarized, Apple Silicon + Intel)
- **Windows** - NSIS installer (code signed)
- **Linux** - AppImage or DEB

**[Download Canopy](https://github.com/itsoltech/canopy-desktop/releases/latest)** - free, source-available, cross-platform desktop app for developers. Auto-updates built in.

## Tech stack

Electron &bull; Svelte 5 &bull; TypeScript &bull; xterm.js &bull; node-pty &bull; tmux &bull; SQLite &bull; simple-git

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Source-available under the [Canopy Source-Available License v1.0](LICENSE.md). Free to use for any purpose, commercial or personal.

Copyright (c) 2026 IT SOL Sp. z o.o.
