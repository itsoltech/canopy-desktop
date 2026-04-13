# Git operations

> Detects repositories, watches for changes in real time, and provides a full set of git operations through IPC handlers.

**Status:** Stable
**Introduced:** v0.1.0
**Platforms:** All

## Overview

When a workspace is opened, Canopy runs `git rev-parse --show-toplevel` to detect whether the directory is inside a git repository. If it is, the app reads the current branch, worktree list, dirty status, and ahead/behind counts, then starts a file watcher on the `.git/` directory to keep this information current without polling.

The git integration exposes over 25 IPC handlers covering day-to-day operations: committing, pushing, pulling, fetching, stashing, branching, checking out, staging individual files, reverting files, viewing diffs, creating pull requests, and generating AI commit messages. All git commands run in the main process via the `simple-git` library, wrapped in `neverthrow` Result types. The renderer never shells out to git directly.

The workspace store on the renderer side holds the current `GitInfo` state (branch name, dirty flag, ahead/behind counts, worktree list). This state updates automatically via `git:changed` push events from the main process whenever the watcher detects relevant file system changes.

## Behavior

### Repository detection

1. User opens a folder as a workspace.
2. Renderer calls `window.api.gitDetect(path)`.
3. `GitRepository.detect()` runs `git rev-parse --show-toplevel` to find the repo root.
4. If the path is not inside a git repo, the result contains `{ isGitRepo: false }` and no watcher is started.
5. If it is a repo, the method reads branch (`rev-parse --abbrev-ref HEAD`), worktree list (`worktree list --porcelain`), dirty status (`git status`), and ahead/behind (`rev-list --left-right --count HEAD...@{upstream}`).
6. Each sub-query uses `orElse` so that failures in one (e.g., no upstream configured) do not prevent the others from succeeding.
7. The renderer receives a `GitInfo` object and updates `workspaceState`.

### File system watching

1. After detection, the renderer calls `window.api.gitWatch(repoRoot, snapshot)`.
2. The main process creates a `GitWatcher` that subscribes to the `.git/` directory using `@parcel/watcher`. **This watcher is specialized for repository state changes** and only monitors the internal `.git/` structure (HEAD, index, refs, worktrees). It does NOT monitor the user's project files in the workspace; that is handled by the general [File watcher](../diagnostics/file-watcher.md).
3. The watcher ignores high-noise subdirectories: `objects`, `logs`, `hooks`, `lfs`, `modules`.
4. Only four categories of paths trigger refreshes: `HEAD`, `index`, `refs/*`, and `worktrees/*`. Lock files and transient files (`COMMIT_EDITMSG`, `FETCH_HEAD`, `packed-refs`, `config`) are ignored.
5. Events are debounced 300ms. Multiple events within the window are collapsed into a single refresh that only re-queries the changed dimensions (branch, worktrees, dirty, aheadBehind).
6. After each refresh completes, a 500ms grace window suppresses `.git/index` events that were triggered by the refresh itself (stat calls from `isDirty()` / `git diff` touch the index), preventing a self-triggered loop.
7. The refreshed `GitInfo` is sent to the renderer via `git:changed` push event.
8. When the workspace is closed, the renderer calls `window.api.gitUnwatch()` to tear down the subscription.

### Committing

1. User writes a commit message in the changes panel (or accepts an AI-generated one).
2. Renderer calls `window.api.gitCommit(repoRoot, message, stageAll)`.
3. If `stageAll` is true, `git add -A` runs first.
4. `git commit` executes and returns `{ hash, summary }` where summary is a string like "3 changed, 10 insertions, 2 deletions".
5. The git watcher detects the index/HEAD change and fires a `git:changed` event to update the UI.

### Pushing

1. Renderer calls `window.api.gitPush(repoRoot)`.
2. `GitRepository.push()` first checks whether an upstream is configured (`rev-parse --abbrev-ref @{u}`).
3. If upstream exists, it runs `git push`. If not, it runs `git push -u origin HEAD` to set up tracking.
4. Returns `{ branch, remote }`.

### Pulling

1. Renderer calls `window.api.gitPull(repoRoot, rebase)`.
2. If `rebase` is true, `--rebase` is passed to `git pull`.
3. Returns `{ summary }` with the number of files updated.

### Branching

1. **List branches**: `git branch -a` returns local and remote branches, with the current branch identified.
2. **Create branch**: validates the ref name (rejects names starting with `-`), then runs `git branch <name> <baseBranch>`.
3. **Checkout**: validates the ref name, then runs `git checkout <branch>`.
4. **Delete branch**: runs `git branch -d` (or `-D` if force is true).
5. **Delete remote branch**: runs `git push <remote> --delete <name>`.
6. **Check if merged**: runs `git branch --merged` and checks whether the branch appears in the output.

### Diff viewing

1. **Full diff**: `GitRepository.getDiffParsed()` runs `git diff HEAD` (falling back to `git diff` if HEAD fails, e.g., in an empty repo). It then finds untracked files via `git ls-files --others --exclude-standard` and reads their content to build synthetic "added" diffs.
2. **Single file diff**: `GitRepository.getFileDiff()` runs `git diff HEAD -- <path>`. If there is no tracked diff (file is untracked), it reads the file content directly.
3. The diff parser (`diffParser.ts`) converts raw unified diff output into structured `ParsedDiff` objects with file-level metadata (status: added/modified/deleted/renamed, additions, deletions) and hunk-level change arrays.

### Staging and reverting files

1. **Stage**: `git add <filePath>`.
2. **Revert**: `git checkout -- <filePath>` to discard working tree changes.

### Stashing

1. **Stash**: `git stash`.
2. **Stash pop**: `git stash pop`.

### AI commit message generation

1. User clicks the "Generate commit message" button in the changes panel.
2. Renderer calls `window.api.gitGenerateCommitMessage(repoRoot)`.
3. The main process reads the current diff (`git diff --cached` or `git diff` if none staged) and sends it to the AI model defined in preferences.
4. The generation uses a specialized prompt (`src/main/ai/commitMessageGenerator.ts`) that enforces the Conventional Commits specification.
5. The generated message is returned to the renderer and inserted into the commit message text area for the user to review.

### Push info

`GitRepository.getPushInfo()` returns the current branch, its configured remote, and the number of commits ahead of the remote (`rev-list --count <remote>/<branch>..HEAD`). This data drives the push button badge showing how many commits will be pushed.

## Configuration

Git operations use the repository's own `.gitconfig` and `.gitattributes`. No app-level git configuration is stored.

The git watcher debounce interval is 300ms (hardcoded in `GitWatcher`). The self-triggered grace window is 500ms.

## Error states

| Error                | User sees                                              | Cause                                                |
| -------------------- | ------------------------------------------------------ | ---------------------------------------------------- |
| `NotAGitRepo`        | "Not a git repository: /path"                          | Opened folder is not inside a git repo               |
| `GitCommandFailed`   | "Git \<command\> failed: \<message\>"                  | Any git command returned a non-zero exit or threw    |
| `InvalidRef`         | "Invalid git ref: \<ref\>"                             | Branch or ref name starts with `-` (injection guard) |
| `WatcherStartFailed` | "Failed to start git watcher at \<path\>: \<message\>" | `@parcel/watcher` could not subscribe to `.git/`     |

## Source files

- Repository operations: `src/main/git/GitRepository.ts`
- File system watcher: `src/main/git/GitWatcher.ts`
- Error types and formatters: `src/main/git/errors.ts`
- Diff parser: `src/main/git/diffParser.ts`
- Diff/file types: `src/main/git/types.ts`
- Workspace state (renderer): `src/renderer/src/lib/stores/workspace.svelte.ts`
- Preload (git API): `src/preload/index.ts`
