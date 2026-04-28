# Architecture

## Process model

Canopy uses a three-process Electron architecture:

- **Main process** (`src/main/`) runs in Node.js. Manages windows, spawns PTY sessions, accesses the filesystem, runs Git commands, connects to external APIs (Jira, YouTrack, GitHub), and owns the SQLite database. All privileged operations happen here.
- **Preload** (`src/preload/`) bridges main and renderer via `contextBridge`. Exposes typed functions organized by feature domain. Stateless -- no logic beyond IPC forwarding.
- **Renderer** (`src/renderer/`) runs Svelte 5 in a browser context. DOM and browser APIs only. Cannot import Node.js modules. Communicates with main exclusively through the preload bridge.

## IPC conventions

Channels follow a hierarchical `feature:action` or `feature:subfeature:action` naming convention: `git:commit`, `db:workspace:list`, `perf:hud:start`.

Request-response uses `ipcRenderer.invoke` / `ipcMain.handle`. One-way events from main to renderer use `ipcRenderer.on` (push notifications like `git:changed`, `pty:exit`, `update:available`).

The preload exposes dedicated typed functions per domain, not a generic `invoke(channel, ...args)`. The full API surface is typed in `src/preload/index.d.ts`.

IPC handlers validate input from the renderer (the renderer is untrusted). Write operations unwrap Results with `unwrapOrThrow()`. Read operations use `.unwrapOr(defaultValue)` for safe fallbacks.

## Error handling

Business logic uses `neverthrow` (`Result<T, E>`, `ResultAsync<T, E>`) instead of `try/catch`. Each domain defines a typed error union with `_tag` discriminants in a dedicated `errors.ts` file (e.g., `src/main/git/errors.ts`, `src/main/taskTracker/errors.ts`).

External calls (shell commands, HTTP requests, file I/O) are wrapped with `fromExternalCall()` from `src/main/errors.ts`, which converts rejected promises into typed errors.

Error message formatters are co-located with error type definitions. They use `ts-pattern` `.exhaustive()` to guarantee all error variants produce a user-facing message.

`try/catch` is allowed only at process boundaries: PTY cleanup (EBADF), HTTP body parsing, `JSON.parse` on untrusted input, `contextBridge` initialization, file cleanup in `finally`, and renderer event handlers.

## Security

Canopy applies a defense-in-depth approach to security:

- **Environment Isolation**: AI agent sessions are spawned with a filtered environment. `src/main/security/envBlocklist.ts` prevents sensitive Electron and system variables from leaking to agents.
- **Path Validation**: The `validatePathAccess` helper in IPC handlers ensures that file operations (read/write/watch) are restricted to directories explicitly registered as part of the current workspace.
- **Isolated JS Worlds**: Browser credential autofill executes in an isolated JavaScript world (ID 999) to prevent malicious web pages from intercepting injected credentials.
- **Credential Protection**: Tokens and API keys are stored in the OS keychain via Electron's `safeStorage`. They are decrypted only in the main process and never sent to the renderer in plaintext.

## Pattern matching

`ts-pattern` (`match`/`with`) replaces `switch` and `if/else` chains when branching on discriminated unions, string literal types, or object shapes (3+ branches). `.exhaustive()` enforces compile-time completeness. `.otherwise()` handles cases with a default. `P.union()` groups related cases. `P.when()` adds predicate guards.

Simple 1-2 branch conditionals and numeric threshold checks do not use `ts-pattern`.

## State management

Renderer state uses Svelte 5 runes in `.svelte.ts` store files under `src/renderer/src/lib/stores/`. Stores are module-level singletons (not context-based).

- `$state` for mutable reactive state
- `$derived` for computed values
- `$effect` for side effects (must return cleanup functions when registering listeners or timers)

Main process state is mostly stateless per-request, except for long-lived managers (PTY sessions, Git watchers, file watchers, remote sessions) that maintain in-memory maps of active resources.

## Data storage

Canopy uses a multi-tiered storage strategy to balance persistence, security, and portability:

- **SQLite** (`better-sqlite3`) is the primary local database, located at the Electron `userData` path. It is managed through several specialized stores:
  - `WorkspaceStore`: Tracks opened repositories, their display names, and cached Git status.
  - `PreferencesStore`: Key-value storage for user settings and feature toggles.
  - `LayoutStore`: Persists the arrangement of split panes and open tabs per worktree.
  - `OnboardingStore`: Records completed onboarding steps to prevent re-display.
  - `CredentialStore`: Stores masked credentials for the browser; actual passwords are encrypted via `safeStorage`.
- **OS Keychain** (`safeStorage`) protects sensitive tokens (task tracker API keys, service credentials). These are never stored in plaintext and never leave the main process.
- **Repository Configuration** (`.canopy/config.json`) allows per-project settings like task tracker board IDs and branch/PR templates. This file is intended to be committed to version control.
- **Global Configuration** (`~/.canopy/config.json`) provides a fallback for repository-level settings across all projects on a machine.

Config resolution follows this precedence: **Built-in defaults < Global config < Repo config < Board-level overrides**.

## Theming

All UI colors use CSS custom properties from the `--color-*` system defined in `src/renderer-shared/styles/tokens.css` (Tailwind v4 `@theme` block). The theme engine (`src/renderer/src/lib/theme/appTheme.ts`) derives the full `--color-*` palette from the active terminal theme. When the user changes their terminal color scheme, the entire app UI adapts.

Categories: `--color-bg*` (backgrounds), `--color-text*` (foreground), `--color-border*` (borders), `--color-accent*` (interactive elements), `--color-danger*`/`--color-success`/`--color-warning*` (status), `--color-hover`/`--color-active` (interaction states).

Exception: the notch overlay (`src/renderer/src/components/notch/NotchOverlay.svelte`, `NotchNotificationRow.svelte`) uses fixed colors because it renders on the macOS physical black notch area. Box-shadow `rgba(0,0,0,...)` values are structural and exempt.

## Feature flags

Non-core features are gated by user preferences (stored in SQLite via `db:prefs:*` IPC). Features default to off. Users opt in through Settings or onboarding. Only security fixes, critical UX, and essential workflows may be auto-enabled.

The onboarding system (`src/renderer/src/lib/onboarding/steps.ts`) presents new features to users after app updates, with each step tagged by the version that introduced it.
