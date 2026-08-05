# Task tracker

> Connect Jira, YouTrack, or GitHub Issues to browse tasks, create branches, and open pull requests from within Canopy.

**Status:** Stable
**Introduced:** v0.9.0
**Platforms:** All

## Overview

The task tracker lets users connect one or more issue trackers (Jira Cloud, YouTrack, GitHub Issues) and work with tasks without leaving Canopy. A user can browse tasks filtered by status or assignee, create a Git branch named from a configurable template, and open a pull request whose title and body are also template-driven.

Configuration lives in two stores: a personal store in Canopy preferences (tracker connections, private to the user) and the per-repository config in `.canopy/config.json` (naming configuration, shared via git). Tracker definitions are merged additively (repo overrides personal on the same `id`); branch/PR templates and project overrides come from the repo config alone, falling back to built-in defaults when unset. Project-level overrides customize branch and PR templates per tracker project, keyed by the task-key prefix.

Authentication tokens are stored locally in Canopy's capability-scoped credential registry. Stable
credential IDs are bound to individual tracker connections, while service, audience and capability
checks prevent a token intended for another integration from being reused. Secrets are encrypted at
rest via Electron `safeStorage` when available and are never written to `.canopy/config.json`. See
[Integration credentials](credentials.md) for the storage, binding and migration model.

Each provider implements a common `TaskTrackerProviderClient` interface. Jira uses the REST v3 and Agile 1.0 APIs. YouTrack uses the Hub REST API. GitHub Issues uses the GraphQL API (with automatic `owner/repo` detection from git remotes when the `projectKey` is empty).

## Behavior

### Connecting a tracker

1. User opens the task tracker settings panel.
2. User selects a provider (Jira, YouTrack, or GitHub) and enters the base URL and project key.
3. For Jira with username/password auth, user provides both username and API token. For bearer token auth (YouTrack, GitHub PAT), user provides only the token.
4. Canopy calls `testConnection` or `testNewConnection` against the provider's user endpoint (`/rest/api/3/myself` for Jira, `/api/users/me` for YouTrack, `{ viewer { login } }` GraphQL query for GitHub).
5. On success, credentials are stored via `keychainSetCredentials(provider, baseUrl, token, username?)`. The tracker definition is saved to the personal (Settings) or repo config.
6. On failure, the provider returns a `ProviderApiError` with the HTTP status and message. The UI shows the error inline.

### Browsing tasks

1. User opens the task list for a connected tracker.
2. Canopy calls `fetchTasks` with optional filters: `statuses`, `assignedToMe`, `projectKey` (`boardId` remains as a legacy fallback).
3. Jira queries via JQL (`project = KEY AND statusCategory != Done` for the selected project; `assignee = currentUser()` only in the legacy no-project fallback). YouTrack uses its query syntax (`for: me`, `project: {KEY}`). GitHub fetches issues via GraphQL with `IssueFilters`.
4. Tasks are returned as `TrackerTask` objects with normalized fields: `key`, `summary`, `status`, `priority`, `type` (mapped from provider-specific values), `parentKey`, `sprintName`, `assignee`, and `url`.
5. If no tasks match the filters, the UI shows an empty state.
6. Jira and YouTrack fetch up to 200 tasks per request. GitHub fetches up to 100. Jira excludes issues in the "Done" status category by default.
7. The picker opens with the filter panel expanded: "Only assigned to me", per-status chips, and per-sprint chips (tasks without a sprint fall into a "(no sprint)" bucket). Rows show the sprint name next to the assignee. Filter selections persist per connection + project; the pickers filter by tracker PROJECT (boards are no longer surfaced).
8. The picker is also reachable from the Create Worktree modal ("+ new" → **From task**, disabled when no tracker is configured or its credentials are missing/expired) — picking a task continues into the branch-create form, where the base branch is chosen and the branch name is generated from the task.

### Task type mapping

GitHub Issues does not have native issue types. The provider infers type from labels:

- Labels starting with `type:` or `kind:` are parsed (e.g., `type:bug` maps to `bug`, `type:feature` maps to `story`).
- Standalone labels `bug`, `enhancement`, `feature`, `epic` are also recognized.
- Priority is inferred similarly from `priority:` or `p:` labels, or standalone `critical`/`high`/`low` labels.

Jira maps `issuetype.subtask = true` to `subtask`, and normalizes type names (`User Story` to `story`).

### Viewing task details

1. User selects a task from the list.
2. Canopy fetches comments via `fetchTaskComments` and attachments via `fetchTaskAttachments`.
3. Comments are truncated to 1000 characters each; a maximum of 15 recent comments are shown.
4. Task description is truncated to 3000 characters. Descriptions and comments render as
   sanitized markdown through the shared `Markdown` component
   (`src/renderer/src/components/shared/Markdown.svelte`).
5. Attachments can be downloaded to a temp directory (`canopy-attachments-{uuid}` in `os.tmpdir()`). Downloads are capped at 50 MB per file with a 60-second timeout. The download URL must match the connection's `baseUrl` origin.
6. Downloaded attachments are automatically cleaned up after 60 seconds.
7. Clicking an attachment opens an in-app lightbox (`AttachmentLightbox`): images render
   directly (proxied through the authenticated connection, addressed by task key +
   attachment id only), every attachment offers **Save…** to disk via a native save
   dialog (`taskTracker:attachmentSave`, suggested filename sanitized), and opening the
   tracker in the browser is a secondary icon action.

### Task panel & write-back

When the selected worktree is backed by a tracker task, the right Inspector gains a **Task** tab
(also opened by clicking the task banner in the sidebar's Project management section — the banner
is always visible; with no task linked it reads "No task linked — pick one" and opens the task
picker). The backing tasks resolve from **every** task key found in the branch name
(`extractTaskKeys` in `src/renderer/src/lib/taskTracker/branchTaskKey.ts`, e.g. both `GAKKO-100`
and `GAKKO-123` from `s115/GAKKO-100/GAKKO-123-fix`), each validated via `findTaskByKey` (keys the
tracker rejects are dropped; bare keys are kept when the tracker is unreachable), plus the
`activeTask.{worktreePath}` preference written at branch creation. When several tasks are tracked
the panel shows a key switcher; the activeTask is selected by default, otherwise the **last** key
in the branch name (parent/subtask convention — work happens on the most specific task) and the
sidebar banner shows a `+N` counter.

The panel shows the task header (key linking to the tracker, status chip colored by the tracker's
status category, **assignee**, the task **description**), a **Change status** form (target
statuses render as category-colored chips) and the **comment history** with an add-comment box.
Clicking the active-task chip in the sidebar opens (and re-opens) the right panel on this tab, so a
hidden Inspector is always one click away. Status changes are workflow-aware where the provider
allows introspection:

| Capability                   | Jira                                                   | YouTrack                                                   | GitHub Issues                                   |
| ---------------------------- | ------------------------------------------------------ | ---------------------------------------------------------- | ----------------------------------------------- |
| Available transitions        | `GET /issue/{key}/transitions` (from current status)   | State bundle values (minus current)                        | Static: close (completed / not planned), reopen |
| Required-field introspection | Yes — `expand=transitions.fields` (`required`, values) | No — workflow rules live in scripts; server error surfaced | n/a                                             |
| Resolution / state reason    | `resolution` select from `allowedValues`               | n/a                                                        | `stateReason`: COMPLETED / NOT_PLANNED          |
| Comment on transition        | `update.comment` (ADF) in the same request             | attached to the Commands API call                          | separate `addComment` mutation after the change |
| Add comment                  | `POST /issue/{key}/comment` (ADF body)                 | `POST /api/issues/{key}/comments`                          | `addComment` GraphQL mutation (issue node id)   |

Requirement fields the panel cannot edit (required fields other than an option list or the comment)
disable Apply with a hint to set them in the tracker. IPC channels:
`trackerConfig:fetchTransitions`, `trackerConfig:applyTransition`, `trackerConfig:addComment`.

### Linking tasks to a worktree

The link dialog (sidebar → "Link task" / "Link another task") has two tabs:

- **Existing tasks** — the shared task-selection block (`TaskListPicker`: Project select, filters,
  search, task list). Picking a task shows a confirmation card with **Cancel / Link**; linking
  writes the `activeTask.{worktreePath}` preference, closes the dialog and opens the Task panel.
  Already-linked rows carry a "Linked" chip and an Unlink action (keys embedded in the branch name
  read as linked and cannot be unlinked).
- **New task** — the shared create form (see below); the created task is linked automatically.

### Creating a task in the tracker

The **New task** form (`NewTaskForm.svelte`) is shared by the link dialog and the Create Worktree
modal's fourth mode. Fields adapt per provider:

| Field    | Jira                                                                 | YouTrack                                                      | GitHub Issues               |
| -------- | -------------------------------------------------------------------- | ------------------------------------------------------------- | --------------------------- |
| Project  | `project/search` (shown when tracking >1)                            | `/api/admin/projects`                                         | hidden                      |
| Type     | per-project `createmeta` (global list on 403)                        | project Type bundle                                           | hidden                      |
| Assignee | `user/assignable/search` (accountId)                                 | project team, global users fallback (login)                   | `assignableUsers` (node id) |
| Board    | all boards (sprint source)                                           | agile boards                                                  | hidden                      |
| Sprint   | board sprints (active+future), applied post-create via the agile API | board sprints via the Commands API (`Board {board} {sprint}`) | open milestones (node id)   |

The current user is preselected as assignee when they appear in the assignable list. Create is
single-fire; post-create steps that fail after the task exists (Jira sprint move, YouTrack
type/assignee/sprint commands) surface as **warnings** (toasts), never as a failed create — a
retry would duplicate the task. After creation the full task is re-fetched (`findTaskByKey`) so
branch templates render from real data. IPC channels: `trackerConfig:fetchAssignableUsers`,
`trackerConfig:fetchSprints`, `trackerConfig:fetchCreateTaskTypes`, `trackerConfig:createTask`.

In the Create Worktree modal ("+ new" → **New task**, gated like From task) the created task drops
straight into the From-task flow: selected-task card, template-generated editable branch name,
base branch and Create.

### Creating a branch from a task

1. User clicks "Create Branch" on a task.
2. Canopy resolves the branch name using the configured `branchTemplate`. The default template is `{branchType}/{taskKey}-{taskTitle}`.
3. The create form shows the proposed branch name, an optional branch-type select, an optional agent to launch, and a **base branch** picker (grouped local/remote, populated from `gitBranches`). The base defaults to the currently active branch but can be changed to any local or remote branch.
4. Canopy creates a worktree for the new branch off the selected base via `gitWorktreeAdd`, then runs any configured worktree setup actions and optionally launches the selected agent with the task context.
5. While the flow runs, the form shows inline progress for worktree creation, setup, agent startup, agent readiness, and sending the task context.
6. If worktree creation fails, an error dialog is shown with the Git error message.
7. If the worktree is created but the agent cannot be started, does not become ready, or cannot receive the task context, the form keeps the partial-success state visible and offers **Retry Send** without creating another worktree.

### Branch template system

Templates use `{placeholder}` syntax. Built-in placeholders:

| Placeholder    | Description                            | Example value   |
| -------------- | -------------------------------------- | --------------- |
| `{branchType}` | Mapped from task type                  | `feat`          |
| `{taskKey}`    | Task key/ID                            | `GAKKO-21`      |
| `{taskTitle}`  | Task summary, slugified                | `fix-login-bug` |
| `{taskType}`   | Raw task type                          | `subtask`       |
| `{parentKey}`  | Parent task key                        | `GAKKO-20`      |
| `{sprint}`     | Sprint number                          | `10`            |
| `{sprintName}` | Sprint name                            | `Sprint 10`     |
| `{boardKey}`   | Board/project key (prefix of task key) | `GAKKO`         |

Legacy conditional markers (`{?varName}`/`{/varName}`) are stripped during rendering and their inner content is treated as normal text. Instead, a placeholder with no value renders to nothing **and removes its immediately preceding separator** (`/`, `-` or `_`), so empty fields never leave a dangling separator; duplicate slashes are then collapsed and leading/trailing separators trimmed.

Templates must contain `{taskKey}`. The slugify function lowercases, strips non-alphanumeric characters, replaces spaces with hyphens, and caps at 50 characters. The result is sanitized as a valid Git branch name (no `..`, `~`, `^`, `:`, `?`, `*`, `[`, `]`, `\`, `@`, `#`, `{`, `}`, spaces).

Default type mapping: `bug` to `fix`, `story`/`task`/`subtask`/`epic` to `feat`. Custom type mappings can override this at the base template or per project override; the settings editor lists the tracker's own task types.

### Creating a pull request from a task

Repository-declared trackers are ordered before personal/global connections when configurations
are merged. Operations that do not carry an explicit tracker ID therefore default to repository
context; an explicit unknown ID always fails closed instead of falling back to another tracker.

When the repository does not have the GitHub API integration configured, the sidebar resolves its
`View PR #N` row through the lightweight `taskTracker:prSummary` IPC channel. The main process first
runs `gh pr list --state open --head <branch> --limit 1`; when there is no open PR, it repeats the
lookup with `--state closed` so the sidebar can still show the latest merged/closed state while
keeping **Create PR** available. Both calls return only the PR number, state, and draft flag. The
lookup is coalesced per repository and branch, and settled results are cached for 30 seconds; in-app
PR mutations invalidate the entry immediately. A PR created externally — including with `gh pr
create` in a Canopy terminal — is picked up after the cache expires when the branch is revisited, or
after restarting Canopy. A missing `gh` executable
or repository without a GitHub origin silently disables this optional fallback. Authentication,
network, timeout, and malformed-response failures are shown as a retryable `PRLookupFailed` row and
hide **Create PR** until the retry succeeds; they are not treated as proof that the branch has no PR.

1. User triggers PR creation from the sidebar **GIT** section (`Create PR` row; an existing PR shows as `View PR #N` with a state chip instead). A native form shows the title and description for editing — rendered from the PR template when a tracker task is linked to the worktree, otherwise pre-filled from the branch name — plus a target-branch select, a reviewer search picker, and an assignee field defaulting to the authenticated `gh` user.
2. Canopy pushes the current branch to the remote (failure is non-fatal).
3. Canopy checks that the GitHub CLI (`gh`) is installed. If not, the operation fails with a `PRCreationFailed` error.
4. Canopy checks for an existing **open** PR on the branch using `gh pr list --state open --head`. If one exists, its URL is returned without creating a duplicate; merged/closed PRs do not block a new one.
5. If no open PR exists, Canopy runs `gh pr create` with the (possibly user-edited) title, body, base branch, head branch, `--assignee` (the form's assignee, `@me` by default) and any `--reviewer` entries.
6. PR title and body are rendered from the `prTemplate` config using `{taskKey}`, `{taskTitle}`, `{taskType}`, `{parentKey}`, `{boardKey}`, `{taskUrl}`, and `{taskDescription}` placeholders.
7. The target branch is resolved from `targetRules`: if a rule matches the task's type, the rule's `targetPattern` is used (with placeholder substitution and optional lookup against existing branches). Otherwise, `defaultTargetBranch` is used.
8. The source branch and resolved target branch are rejected if they start with `-`, so renderer-provided branch names or repository PR config cannot be interpreted as `gh` CLI flags.

### Sending task context to an AI agent

1. User clicks "Send to Agent" on a task.
2. Canopy fetches three things in parallel: the full task details (to get the description, which list fetches omit for performance), comments, and attachments — each with fallback from config-based API to legacy connection-based API.
3. A formatted context string is assembled: task header, metadata (status, priority, type), URL, description, comments, and attachment file references (`@/path/to/file`).
4. The text is wrapped in bracketed-paste markers (`ESC[200~ … ESC[201~`) and written to the agent's PTY as one block, followed by `\r` to submit. The wrapping keeps the CLI's paste detection honest even when the OS (e.g. Windows ConPTY) delivers the write in multiple chunks; without it the tail of a long description can leak in as typed input. Control characters and any stray `ESC[201~` inside the content are sanitised first so they can't hijack the terminal or end the paste early.
5. Quick send from the task picker keeps the picker open and shows inline feedback if there is no running agent, the target agent tab cannot be focused, task context cannot be built, or the paste fails.
6. When sending as part of **Create & Start Agent**, the branch/worktree form shows the same progress and failure states. If the worktree already exists but sending fails, the user can start or focus an agent in that worktree and click **Retry Send**.
7. Attachment files are cleaned up after 60 seconds via `taskTrackerCleanupAttachments`.

### Sprints

Each provider exposes sprint/milestone information differently:

- Jira: Fetches active sprints from the Agile board endpoint. Sprint numbers are parsed from sprint names.
- YouTrack: Queries board sprints and finds the currently active one by date range. Falls back to the first unresolved sprint.
- GitHub: Maps milestones to sprints. The current sprint is the earliest open milestone by due date.

## Configuration

### Config file locations

| Tier              | Location                                   | Stored in                   |
| ----------------- | ------------------------------------------ | --------------------------- |
| Built-in defaults | Hardcoded in `configDefaults.ts`           | Source code                 |
| Personal store    | Preferences key `taskTracker.globalConfig` | `PreferencesStore` (SQLite) |
| Repo config       | `{repoRoot}/.canopy/config.json`           | Filesystem                  |

### Config schema (`RepoConfig`)

```json
{
  "version": 1,
  "trackers": [
    {
      "id": "jira-default",
      "provider": "jira",
      "baseUrl": "https://team.atlassian.net",
      "projectKey": "PROJ"
    }
  ],
  "branchTemplate": {
    "template": "{branchType}/{taskKey}-{taskTitle}",
    "customVars": {},
    "typeMapping": { "bug": "fix", "story": "feat" }
  },
  "prTemplate": {
    "titleTemplate": "[{taskKey}] {taskTitle}",
    "bodyTemplate": "## {taskKey}: {taskTitle}\n\n{taskUrl}",
    "defaultTargetBranch": "main",
    "targetRules": [{ "taskType": "subtask", "targetPattern": "feat/{parentKey}" }]
  },
  "projectOverrides": {
    "GAKKO": {
      "branchTemplate": { "template": "custom/{taskKey}" },
      "prTemplate": { "defaultTargetBranch": "develop" }
    }
  },
  "filters": {
    "assignedToMe": true,
    "statuses": ["To Do", "In Progress"]
  }
}
```

### Merge order

Naming configuration is owned by the **project alone**; the personal (global) store only contributes
tracker connections. `mergeConfigs()` applies these rules:

1. **Trackers**: Additive merge by `id` (personal + repo). If both define a tracker with the same `id`, repo wins.
2. **Branch template / PR template / project overrides**: From the repo config only; when unset, the built-in defaults (`configDefaults.ts`) apply. The personal store is never a template fallback.
3. **Filters**: Repo always wins when repo config exists.

**Legacy config (intentional breaking change).** Two earlier config tiers are no longer read:
`boardOverrides` (template overrides keyed by AGILE BOARD id) was replaced by `projectOverrides`
keyed by the tracker PROJECT (task-key prefix) — a board id said nothing about which template a
task should use once pickers moved from boards to projects; and the personal ("Your defaults")
naming-template tier was removed — naming is owned by the project alone. Old keys left in
`.canopy/config.json` are ignored (never deleted); recreate the relevant overrides per project
in the Project tracker modal.

The `ResolvedConfig` includes a `source` object indicating where each field came from (`'repo'` or `'default'`; templates never resolve as `'global'`).

### Configuration UI

Two separate surfaces, deliberately not mixed:

- **Settings → Project management → Your connections** — your personal tracker connections (stored in the preferences DB, private to you, reused across projects) with full add/edit/delete and credential management. This is the authoritative place to change or remove a token. An **OS-aware** note states where credentials are kept — encrypted via Windows DPAPI / macOS Keychain / Linux keyring in Canopy's local database, bound to the connection by stable credential ID, never written to the repository (and warns when OS encryption is unavailable).
- **Project tracker modal** — opened from the left sidebar's **Project management** section; scoped to the **active worktree** and edits its `.canopy/config.json` (shared with the team via git). Sections:
  - **Connections** — trackers defined in the repo config; here you only _connect_ them (enter credentials in a dedicated dialog). Each tracker has a local binding to a compatible credential. Stored tokens are **verified** against the tracker API on config load; a rejected token shows its last authentication/capability result with a **Reconnect** action. The binding remains resolvable so corrected server-side permissions can recover on the next request.
  - **Branch naming** and **Pull request naming** — per-project rows (`All projects (default)` + one row per project override), each showing the template plus a rendered example. Editing happens in place (Cancel reverts, Done collapses); **Add project override** creates a new per-project template, picking from the tracker's project list. PR rows show the title; the editor exposes title, body (multi-line) and the default target branch. The base branch editor also maps the tracker's task types to {branchType}. Editing is read-only until a tracker is connected.
- **Reset to default** — removes the project value from `.canopy/config.json` so the **built-in** template applies (there is no other tier). **Remove project override** drops a project-specific override so tasks from that project fall back to the base template.
- **Template editor** — hybrid: `{field}` placeholders are draggable chips; everything between them is plain text edited in place (any separator works). Renderer-side helpers in `src/renderer/src/components/preferences/_partials/configScopeLabels.ts` (unit-tested with Vitest, `npm test`).
- **Needs-credentials surfacing** — when the repo config defines a tracker with no usable credentials (missing or expired), it is listed in the left sidebar's **Project management** section with an "Add credentials" action.

### Project-level overrides

Project overrides are keyed by the tracker PROJECT key — the task-key prefix (`GAKKO-1` → `GAKKO`) — within `projectOverrides`. The key is derived from the task itself, so overrides also apply to tasks resolved from a branch name. When fetching the effective template:

1. Start with the base template (from the repo config, or the built-in default when unset).
2. If a `projectOverrides[projectKey]` entry exists, apply its partial override:
   - For branch templates: override `template`, merge `customVars` (override wins on same key), override `typeMapping`.
   - For PR templates: override individual fields (`titleTemplate`, `bodyTemplate`, `defaultTargetBranch`, `targetRules`).

### Credential storage

Tracker connections bind to stable records in the capability-scoped credential registry. Secret,
descriptor and binding storage, renderer isolation and migration of legacy
`taskTracker.token.<provider>:<baseUrl>` entries are documented in
[Integration credentials](credentials.md).

### GitHub auto-detection

When a GitHub tracker has an empty `projectKey`, Canopy reads the git remote URL from the workspace and parses `owner/repo` from it. This supports `git@`, `https://`, and `ssh://` URL formats. If the remote is not a GitHub URL, a `ProviderApiError` is returned.

## Error states

Note: `taskTracker:attachmentSave` additionally throws plain validation errors that surface
verbatim in a toast — `Invalid task key`, `Invalid attachment id`, `No tracker configured`,
and `Attachment not found on this task`.

| Error                      | User sees                                        | Cause                                                                     |
| -------------------------- | ------------------------------------------------ | ------------------------------------------------------------------------- |
| `ConnectionNotFound`       | "Connection not found: {id}"                     | Deleted or invalid connection ID referenced                               |
| `AuthTokenMissing`         | "No auth token for {name}"                       | No credentials stored for this tracker's provider/URL pair                |
| `CredentialUnavailable`    | "Credentials unavailable for {name}: {reason}"   | Stored credentials are ambiguous, incompatibly bound, or missing a secret |
| `ProviderApiError`         | "{provider} API error {status}: {message}"       | HTTP error from the provider API (auth failure, rate limit, server error) |
| `AttachmentDownloadFailed` | "Failed to download {filename}: {reason}"        | Download timeout, file too large (>50 MB), URL mismatch, or network error |
| `ConfigNotFound`           | "Config not found at {root}/.canopy/config.json" | Repo config file does not exist                                           |
| `ConfigParseError`         | "Invalid config in {root}: {reason}"             | JSON parse error or unsupported config version                            |
| `ConfigWriteError`         | "Failed to write config in {root}: {reason}"     | Filesystem permission error or disk full                                  |
| `PRCreationFailed`         | "PR creation failed: {reason}"                   | `gh` CLI not installed, Git push failure, or `gh pr create` error         |
| `PRLookupFailed`           | "PR lookup failed: {reason}"                     | `gh` auth, network, timeout, or malformed summary/details response        |
| `NoActiveAgent`            | "No running agent is available..."               | Quick send was triggered after the active agent target disappeared        |
| `AgentStartFailed`         | "The worktree was created, but..."               | Worktree creation succeeded, but Canopy could not open the selected agent |
| `AgentNotReady`            | "The agent did not become ready..."              | Started agent ended, errored, or did not become idle before timeout       |
| `TabFocusFailed`           | "Could not focus the target agent tab..."        | The target agent tab could not be activated before sending                |
| `TaskContextBuildFailed`   | "Could not build the task context..."            | Task details/comments/attachments could not be fetched or formatted       |
| `TaskContextPasteFailed`   | "Could not paste the task into the agent..."     | Target agent session rejected or lost the paste target                    |

For the four statuses that carry an underlying error — `AgentStartFailed`, `TabFocusFailed`, `TaskContextBuildFailed`, and `TaskContextPasteFailed` — the banner appends that detail as `"{message} — {detail}"` (e.g. the provider, path, or agent error) so the cause is visible without DevTools. The other statuses show the generic message only.

## Security and privacy

- Authentication tokens are stored through `KeychainTokenStore` in the capability-scoped,
  main-process-only registry documented in [Integration credentials](credentials.md). Secrets live
  only in Canopy's local database and never in `.canopy/config.json` or git.
- Attachment downloads validate that the URL origin matches the connection's `baseUrl` before fetching. Downloads are capped at 50 MB and time out after 60 seconds.
- Provider API requests use a 15-second timeout (`AbortSignal.timeout`).
- Jira supports both Basic auth (username + API token) and Bearer token auth, selected based on whether a `username` is present.

## Source files

- Main: `src/main/taskTracker/`
  - `TaskTrackerManager.ts` - core manager with both config-based and legacy connection-based methods
  - `GlobalConfigManager.ts` - global config persistence with legacy migration
  - `RepoConfigManager.ts` - per-repo `.canopy/config.json` management
  - `configMerge.ts` - three-tier config merge logic
  - `configDefaults.ts` - built-in defaults, project-aware template resolution
  - `branchTemplate.ts` - template rendering, slugification, validation, type mapping
  - `prTemplate.ts` - PR title/body rendering, target branch resolution
  - `prCreation.ts` - `gh` CLI integration for push + PR creation
  - `prSummary.ts` - bounded, typed `gh` CLI lookup for sidebar/worktree PR state
  - `KeychainTokenStore.ts` - capability resolution, local bindings and legacy migration
  - `providers/jira.ts` - Jira REST + Agile API client
  - `providers/youtrack.ts` - YouTrack REST API client
  - `providers/github.ts` - GitHub GraphQL API client
  - `errors.ts` - typed error union with message formatter
- Credential registry: `src/main/credentials/CredentialRegistry.ts`
- Store: `src/renderer/src/lib/stores/taskTracker.svelte.ts`
- UI components: `src/renderer/src/components/taskTracker/AttachmentLightbox.svelte` (in-app
  attachment viewer with save-to-disk) and `src/renderer/src/components/shared/Markdown.svelte`
  (sanitized markdown rendering for descriptions/comments)
- Components: `src/renderer/src/lib/taskTracker/`
  - `branchCreation.ts` - branch creation flow with stash/confirm dialogs
  - `taskContext.ts` - task context formatting for AI agents
  - `providerLabel.ts` - display name mapping for providers
