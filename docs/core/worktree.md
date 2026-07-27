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

1. User removes a worktree from the sidebar (worktrees list or project tree), via the command
   palette ("Remove Current Worktree"), or remotely from the mobile app.
2. Every entry point runs the same preflight/consent gate BEFORE any teardown. Preflight
   (`worktree:prepareRemove`) reports what a safe removal needs: uncommitted changes, unmerged
   commits, and initialized submodules (git refuses to remove those worktrees even when clean and
   documents `--force` as the remedy) all set `forceRequired`:
   - local flows share `confirmWorktreeRemoval()` — warnings appear in the confirmation, and only
     that informed confirmation authorizes `--force`; when the preflight itself fails (ghost
     worktree with a broken checkout) the confirmation is explicitly destructive,
   - the mobile flow calls the `worktree.prepareRemove` RPC and shows the warnings on the device;
     independently, the host rejects an unconsented force-required `worktree.remove` BEFORE
     closing any tabs, so a stale or missing device-side preflight cannot destroy live host
     context for a removal that would fail anyway.
3. All tabs associated with that worktree path are closed with removal semantics — PTYs are
   tree-killed and awaited until the processes actually exit, BEFORE their session records are
   dropped. If the removed worktree is currently selected, the selection switches to the main
   worktree first so no UI keeps polling a disappearing path.
4. The main process sweeps remaining holders (Windows deletes a directory only when nothing holds
   a handle inside it): any other PTY whose cwd lies inside the worktree is tree-killed and
   awaited, and file-tree/git watchers subscribed inside the path are stopped and awaited.
5. `git worktree remove <path>` runs without `--force` first. On failure the error is classified:
   - "is not a working tree" → a previous attempt already unregistered the worktree; continue to
     the verified cleanup instead of retrying git,
   - broken `.git` link (a ghost left by an earlier failed removal) → git cannot verify the tree,
     so this proceeds only with the destructive-consent flag; the — prunable — registration is
     cleared by `git worktree prune` below,
   - lock symptoms (permission denied, directory not empty, EBUSY…) → retried WITHOUT `--force`
     with backoff (forcing does not bypass OS file locks; time does — transient locks from dying
     processes or AV scans clear within seconds). When retries run out while git still owns the
     worktree, the removal FAILS with the tree and registration intact — no fallback deletion,
   - dirty-tree or force-required refusals (e.g. submodules) → retried with `--force` only when
     the caller passed the destructive-consent flag.
6. After unregistration, `git worktree prune` clears stale admin records and the absence of the
   path is verified against `git worktree list` BEFORE any raw filesystem cleanup — if git still
   lists it, the removal fails without touching files. Then remnants are deleted with `fs.rm`
   retries; files still held open by another process are reported via `leftoverPath` (surfaced as
   a toast) instead of silently leaving a ghost folder.
7. Branch deletion (when requested) runs even if the worktree removal needed the fallback paths —
   an aborted flow no longer leaves stray branches.
8. The git watcher updates the sidebar.

### Creation pre-flight

`worktree:create` refuses an existing non-empty target directory with a clear message BEFORE any
git call (an empty leftover directory is removed automatically). If `git worktree add` fails
after creating a branch (`-b`), the branch is rolled back — a failed creation no longer leaves a
stray local branch behind.

### Checking unmerged commits

Before removing a worktree or deleting a branch, the app can check for work that has not been pushed:

1. Renderer calls `window.api.gitUnmergedCommits(repoRoot, branch)`.
2. `GitRepository.getUnmergedCommits()` runs `git log <branch> --not --remotes --oneline`.
3. Returns an array of one-line commit descriptions. If non-empty, the UI can warn the user before proceeding.

### Checking dirty status per worktree

1. Renderer calls `window.api.gitStatusPorcelain(repoRoot, worktreePath)`.
2. `GitRepository.getStatusPorcelain()` runs `git status --porcelain` in the specified worktree directory.
3. Returns raw porcelain output. A non-empty result means the worktree has uncommitted changes.

### Worktree path validation

Worktrees are intentionally created outside the workspace by default. New worktrees must
resolve under the user-configured `worktrees.baseDir` preference, which defaults to
`~/canopy/worktrees`. If the user changes that base directory in Preferences, creation is
allowed under the configured directory even when it is outside the user's home directory
or outside the currently attached workspace, provided the base directory was selected
through the main-process directory picker and recorded as trusted.

Creation paths walk up to the closest existing ancestor before realpath-normalizing, so a
non-existent leaf is OK. The missing tail is then reattached and checked against the
configured base directory to reject escapes via `..` or symlinked ancestors. Because the
renderer can write ordinary preferences, the configured base is not the sole authorization
input: creation also requires the resolved base to live under the user's home directory,
an attached workspace, a current main-process folder-selection grant, or the persisted
main-only trusted base recorded from such a grant.

Removal is validated differently because existing worktrees may have been created before
the user changed `worktrees.baseDir`, or may live in another directory that Git still
tracks for the repository. `gitWorktreeRemove`, `worktree:prepareRemove`, and
`worktree:removeWithBranch` only accept an absolute path that exactly matches a non-main
worktree returned by `git worktree list --porcelain` for the target repository. The main
worktree and the current repo root are never removable through these handlers. When
removing and deleting a branch, the branch to delete is derived from the matched Git
worktree record, not from renderer-supplied payload data.

Worktree-scoped git operations (`diff`, `stage`, `revert`, `commit`, push preflight,
`push`, `pull`, `fetch`, `stash`, branch listing, branch create/delete, worktree create,
and worktree remove) validate their repository/worktree argument against the main process'
attached project snapshot. If strict active-window containment fails, the path is still
allowed only when it exactly matches the attached project's workspace path, repo root, or
one of its known worktree paths. This lets inactive attached worktrees operate from
`~/canopy/worktrees` without allowing arbitrary renderer-supplied paths.

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

### Worktree base directory

Configured in Preferences under the Worktrees section. Stored globally as
`worktrees.baseDir`.

| Key                 | Default              | Description                                                                                                                                                        |
| ------------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `worktrees.baseDir` | `~/canopy/worktrees` | Base directory under which newly created worktrees must live. `~` expands to the user's home folder. Use the Browse button to trust a base outside home/workspace. |

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

| Error                                | User sees                                                        | Cause                                                                                                             |
| ------------------------------------ | ---------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `GitCommandFailed` (worktree add)    | "Git worktree add failed: \<message\>"                           | Branch already checked out in another worktree, or path already exists                                            |
| `GitCommandFailed` (worktree remove) | "Git worktree remove failed: \<message\>"                        | Worktree has uncommitted changes and force was not set                                                            |
| `InvalidRef`                         | "Invalid git ref: \<ref\>"                                       | Branch name starts with `-`                                                                                       |
| Path not absolute                    | "Worktree path must be absolute"                                 | Renderer passed a relative path to create or remove                                                               |
| No existing ancestor                 | "Access denied: no existing ancestor"                            | Creation path walks up past the filesystem root without finding an existing ancestor                              |
| Base path escapes ancestor           | "Access denied: worktree base path escapes ancestor"             | Configured `worktrees.baseDir` escapes its closest existing ancestor                                              |
| Base directory not trusted           | "Access denied: worktree base directory is not trusted"          | Configured `worktrees.baseDir` is outside home/workspace and was not selected through a trusted main-process flow |
| Path outside configured base         | "Access denied: worktree path outside configured base directory" | Creation path does not resolve under `worktrees.baseDir`                                                          |
| Worktree not registered              | "Access denied: worktree is not registered for this repository"  | Removal path does not exactly match a Git-listed worktree for the repository                                      |
| Main worktree removal                | "Access denied: cannot remove the main worktree"                 | Removal path matches the repository's main worktree                                                               |
| Current repo root removal            | "Access denied: cannot remove the current repo root"             | Removal path matches the repo root passed to the IPC handler                                                      |
| Repo outside attached project        | "Access denied: path outside workspace"                          | Repo/worktree path is not a strict workspace path or exact attached project path                                  |
| Path escapes ancestor                | "Access denied: worktree path escapes ancestor"                  | Non-existent tail of the creation path escapes its ancestor via `..` or absolute ref                              |
| Setup command timeout                | "Command timed out after 5 minutes"                              | A setup action's shell command did not exit within 300 seconds                                                    |
| Setup command failure                | "\<label\>: Command exited with code \<N\>"                      | A setup action's shell command returned non-zero                                                                  |
| Setup aborted                        | "Setup aborted"                                                  | User cancelled the setup while it was running                                                                     |

## Source files

- Worktree setup runner: `src/main/worktree/WorktreeSetupRunner.ts`
- Git worktree operations: `src/main/git/GitRepository.ts` (methods `worktreeAdd`, `worktreeAddCheckout`, `worktreeRemove`, `worktreePrune`, `listWorktrees`, `getUnmergedCommits`, `getStatusPorcelain`, `hasInitializedSubmodules`, `isBranchMerged`, `branchExists`)
- Removal failure taxonomy: `src/main/git/worktreeRemoval.ts` (`classifyWorktreeRemoveError`, `REMOVE_RETRY_DELAYS_MS`)
- Setup action types: `src/main/db/types.ts` (`WorktreeSetupAction`, `WorktreeSetupProgress`)
- Removal preflight/consent gate: `src/renderer/src/lib/worktrees/removalConsent.ts` (`confirmWorktreeRemoval`) and `src/renderer/src/lib/worktrees/removalGuard.ts` (`removalNeedsForceConsent`, shared with the remote host guard)
- Remote removal RPC: `worktree.prepareRemove` / `worktree.remove` in `src/renderer-shared/rpc/methodList.ts`, handled by `src/renderer/src/lib/remote/HostRpcServer.ts` (mirrored in `mobile/src/lib/remote/protocol/rpc-methods.ts`)
- Worktree agent status: `src/renderer/src/lib/agents/worktreeStatus.svelte.ts`
- Agent state: `src/renderer/src/lib/agents/agentState.svelte.ts`
- Preload (worktree API): `src/preload/index.ts` (`gitWorktreeAdd`, `gitWorktreeCheckout`, `gitWorktreeRemove`, `worktreeCreate`, `worktreePrepareRemove`, `worktreeRemoveWithBranch`, `runWorktreeSetup`, `abortWorktreeSetup`, `onWorktreeSetupProgress`)
