<p align="center">
  <img src="resources/icon.png" alt="Canopy — Developer Terminal Workstation" width="128" height="128">
</p>

<h1 align="center">Canopy</h1>

<p align="center">
  <strong>Developer terminal workstation with built-in AI coding assistant and Git management</strong>
</p>

<p align="center">
  A native desktop terminal emulator that integrates Claude Code AI, Git workflows, and multi-pane terminals into one powerful developer tool — available for macOS, Windows, and Linux.
</p>

<p align="center">
  <a href="https://github.com/itsoltech/canopy-desktop/releases/latest">Download</a> &bull;
  <a href="https://canopy.itsol.tech">Website</a> &bull;
  <a href="https://github.com/itsoltech/canopy-desktop/issues">Issues</a>
</p>

---

<p align="center">
  <img src="docs/screenshot.png" alt="Canopy terminal workstation — Claude Code AI, Git sidebar, and multi-pane terminal" width="800">
</p>

## Features

### Integrated Terminal

Full terminal emulation powered by xterm.js with WebGL-accelerated rendering. Split panes horizontally and vertically, organize work in tabs, and resize with drag-and-drop dividers. Your shell config (`.zshrc`, `.bashrc`) works out of the box with login shell environment resolution. Font ligatures, progress indicators, and layout persistence included.

### Claude Code Integration

Launch Claude Code sessions directly inside Canopy. The real-time Claude Inspector panel shows session status, tool calls, and permission requests at a glance. A hook-based event system streams lifecycle events with native desktop notifications. Configure your API provider (Anthropic, AWS Bedrock, Google Vertex AI, Foundry), choose your model, set permission modes, and inject custom system prompts — all from Preferences. AI-powered commit message generation is built in.

### Git & Worktree Management

Built-in Git sidebar with real-time status monitoring — current branch, dirty state, ahead/behind counts. Push, pull, fetch, stash, and commit directly from the UI. Full Git worktree support lets you create, switch, and remove worktrees without leaving the app. Configure setup actions to run commands or copy files automatically when creating a new worktree. Branch management with merge status indicators and local/remote operations.

### Multi-Project Workspaces

Open multiple projects in a single window with a hierarchical sidebar tree. Each window maintains its own workspace, layout, and sessions. The Welcome Dashboard shows recent projects for quick access. Workspace and layout state persist across restarts — everything is exactly where you left it.

### Tool Ecosystem

Built-in launcher for Claude Code, LazyGit, Codex, Gemini, OpenCode, and custom shell tools. Add your own CLI tools with custom commands, arguments, icons, and categories. Automatic tool availability detection ensures you only see what's installed. Launch any tool from the Command Palette (`Cmd+Shift+P` / `Ctrl+Shift+P`).

### URL Scheme

Deep link into Canopy from anywhere. `canopy://open?path=/project` opens a project. `canopy://run?path=/project&tool=claude` launches a tool in a directory. Single-instance enforcement with smart window deduplication.

## Installation

Download the latest release for your platform:

- **macOS** — DMG or ZIP (code signed & notarized)
- **Windows** — NSIS installer
- **Linux** — AppImage or DEB

**[Download Canopy v0.1.0](https://github.com/itsoltech/canopy-desktop/releases/latest)**

Auto-updates are built in — you'll be notified when new versions are available.

## Tech Stack

Electron &bull; Svelte 5 &bull; TypeScript &bull; xterm.js &bull; node-pty &bull; SQLite &bull; simple-git

## License

Source-available under the [Canopy Source-Available License v1.0](LICENSE.md). Free to use for any purpose — commercial or personal.

Copyright (c) 2026 IT SOL Sp. z o.o.
