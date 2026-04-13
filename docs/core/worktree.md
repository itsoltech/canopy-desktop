# Git worktree management

> Work on multiple branches simultaneously, each in its own directory, with automated post-creation setup.

**Status:** Stable
**Introduced:** v0.3.0
**Platforms:** All

## Overview

Git worktrees allow checking out multiple branches of the same repository into separate directories without cloning. Canopy treats worktrees as first-class navigation targets: the sidebar lists all worktrees for the current repo, each with its own set of tabs, terminal sessions, and agent instances. Selecting a worktree switches the entire workspace context (branch display, dirty status, file tree, changes panel) to that directory.

Users can create worktrees from the UI by picking a base branch and naming a new branch, or by checking out an existing local or remote branch into a new worktree directory. After git creates the worktree, an optional setup runner executes user-configured actions (shell commands, file copies) to bootstrap the new directory with dependencies, environment files, or build artifacts. Each action's progress and output stream back to the UI in real time.

Agents (Claude, Gemini, OpenCode, Codex) can run in dedicated worktrees for isolated code changes. The sidebar shows an aggregate agent status badge per worktree (idle, working, waiting for permission, error) derived from all agent sessions running in tabs under that worktree.

## Behavior

### Creating a worktree with a new branch

1. User opens the create-worktree modal and selects "New branch".
2. User enters a branch name and selects a base branch.
3. Renderer calls `window.api.gitWorktreeAdd(repoRoot, path, branch, baseBranch)`.
4. `GitRepository.worktreeAdd()` validates both ref names (rejects names starting with `-`), then runs `git worktree add -b <branch> <path> <baseBranch>`.
5. The git watcher detects changes in `.git/worktrees/` and fires `git:changed` with `worktrees: true`, updating the sidebar.
6. If worktree setup actions are configured, the setup runner starts automatically (see "Post-creation setup" below).

### Creating a worktree from an existing branch

1. User selects "Existing branch" in the create-worktree modal and picks a local or remote branch.
2. Renderer calls `window.api.gitWorktreeCheckout(repoRoot, path, branch, createLocalTracking)`.
3. For local branches, `GitRepository.worktreeAddCheckout()` runs `git worktree add <path> <branch>`.
4. For remote branches (containing a `/`), if `createLocalTracking` is true, the method extracts the local name (everything after the first `/`), checks whether a local branch with that name already exists via `git branch --list`, and either checks it out or creates a tracking branch with `git worktree add -b <localName> <path> <remoteBranch>`.
5. The worktree appears in the sidebar after the git watcher fires.

### Removing a worktree

1. User right-clicks a worktree in the sidebar and selects "Remove".
2. The app may check for uncommitted changes or unmerged commits before proceeding.
3. Renderer calls `window.api.gitWorktreeRemove(repoRoot, path, force)`.
4. `GitRepository.worktreeRemove()` runs `git worktree remove <path>` (with `--force` if force is true).
5. All tabs associated with that worktree path are closed, killing their PTY sessions.
6. The git watcher updates the sidebar.

### Checking unmerged commits

Before removing a worktree or deleting a branch, the app can check for work that has not been pushed:

1. Renderer calls `window.api.gitUnmergedCommits(repoRoot, branch)`.
2. `GitRepository.getUnmergedCommits()` runs `git log <branch> --not --remotes --oneline`.
3. Returns an array of one-line commit descriptions. If non-empty, the UI can warn the user before proceeding.

### Checking dirty status per worktree

1. Renderer calls `window.api.gitStatusPorcelain(repoRoot, worktreePath)`.
2. `GitRepository.getStatusPorcelain()` runs `git status --porcelain` in the specified worktree directory.
3. Returns raw porcelain output. A non-empty result means the worktree has uncommitted changes.

### Post-creation setup

Worktree setup actions are configured per workspace and stored in the preferences database under the key `workspace:<workspaceId>:worktreeSetup` as a JSON array.

1. After a worktree is created, the renderer calls `window.api.runWorktreeSetup(workspaceId, repoRoot, newWorktreePath)`.
2. The main process reads the setup configuration from preferences. If no config exists, it returns immediately with `{ success: true }`.
3. `WorktreeSetupRunner.runWorktreeSetup()` iterates over each action sequentially.
4. For `command` actions: a PTY is spawned in the new worktree directory running the command through the user's shell. The command string supports three variables: `$MAIN_WORKTREE`, `$NEW_WORKTREE`, and `$REPO_ROOT`, each shell-quoted. Output chunks stream back to the renderer via `worktree:setupProgress` push events. Each command has a 5-minute timeout.
5. For `copy` actions: the file at `source` (relative to the main worktree) is copied to `dest` (relative to the new worktree, defaults to `source` if omitted). Parent directories are created automatically.
6. Progress events include `{ actionIndex, totalActions, label, status, outputChunk?, error? }` where status is `running`, `done`, or `error`.
7. The user can abort setup at any time by calling `window.api.abortWorktreeSetup()`, which triggers an AbortController. The currently running command's PTY is killed and the runner returns `{ success: false, errors: ['Setup aborted'] }`.
8. If an individual action fails, its error is recorded but the runner continues with the remaining actions. The final result includes all errors.

### Agent worktree status

The sidebar shows an aggregate status badge for each worktree that has agent sessions:

1. `getWorktreeAgentStatus(worktreePath)` collects all panes across all tabs for that worktree.
2. It filters to panes that have an entry in `agentSessions`.
3. Each agent's status type is mapped to an aggregate level: `waitingPermission` > `error` > `working` (thinking/toolCalling/compacting) > `idle` > `none`.
4. The highest-priority status across all agents in the worktree is returned.
5. The sidebar renders the corresponding badge (pulsing dot for working, warning icon for permission, etc.).

## Configuration

### Worktree setup actions

Configured in Preferences under the Git section (per workspace). Stored as `workspace:<workspaceId>:worktreeSetup`.

Two action types:

| Type      | Fields                      | Description                                                                                                   |
| --------- | --------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `command` | `command`, `label?`         | Shell command to run in the new worktree. Supports `$MAIN_WORKTREE`, `$NEW_WORKTREE`, `$REPO_ROOT` variables. |
| `copy`    | `source`, `dest?`, `label?` | Copies a file from the main worktree to the new worktree. `dest` defaults to `source`.                        |

Example configuration (JSON):

```json
[
  { "type": "copy", "source": ".env", "label": "Copy .env" },
  { "type": "command", "command": "npm install", "label": "Install dependencies" }
]
```

## Error states

| Error                                | User sees                                   | Cause                                                                  |
| ------------------------------------ | ------------------------------------------- | ---------------------------------------------------------------------- |
| `GitCommandFailed` (worktree add)    | "Git worktree add failed: \<message\>"      | Branch already checked out in another worktree, or path already exists |
| `GitCommandFailed` (worktree remove) | "Git worktree remove failed: \<message\>"   | Worktree has uncommitted changes and force was not set                 |
| `InvalidRef`                         | "Invalid git ref: \<ref\>"                  | Branch name starts with `-`                                            |
| Setup command timeout                | "Command timed out after 5 minutes"         | A setup action's shell command did not exit within 300 seconds         |
| Setup command failure                | "\<label\>: Command exited with code \<N\>" | A setup action's shell command returned non-zero                       |
| Setup aborted                        | "Setup aborted"                             | User cancelled the setup while it was running                          |

## Source files

- Worktree setup runner: `src/main/worktree/WorktreeSetupRunner.ts`
- Git worktree operations: `src/main/git/GitRepository.ts` (methods `worktreeAdd`, `worktreeAddCheckout`, `worktreeRemove`, `listWorktrees`, `getUnmergedCommits`, `getStatusPorcelain`)
- Setup action types: `src/main/db/types.ts` (`WorktreeSetupAction`, `WorktreeSetupProgress`)
- Worktree agent status: `src/renderer/src/lib/agents/worktreeStatus.svelte.ts`
- Agent state: `src/renderer/src/lib/agents/agentState.svelte.ts`
- Preload (worktree API): `src/preload/index.ts` (`gitWorktreeAdd`, `gitWorktreeCheckout`, `gitWorktreeRemove`, `runWorktreeSetup`, `abortWorktreeSetup`, `onWorktreeSetupProgress`)
