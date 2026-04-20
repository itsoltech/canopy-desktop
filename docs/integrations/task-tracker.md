# Task tracker

> Connect Jira, YouTrack, or GitHub Issues to browse tasks, create branches, and open pull requests from within Canopy.

**Status:** Stable
**Introduced:** v0.9.0
**Platforms:** All

## Overview

The task tracker lets users connect one or more issue trackers (Jira Cloud, YouTrack, GitHub Issues) and work with tasks without leaving Canopy. A user can browse tasks filtered by status or assignee, create a Git branch named from a configurable template, and open a pull request whose title and body are also template-driven.

Configuration lives at three tiers: built-in defaults, a global config stored in Canopy preferences, and per-repository config stored in `.canopy/config.json`. When both global and repo configs exist, they are merged with repo-level values winning for templates and filters, while tracker definitions are merged additively (repo overrides global on the same `id`). Board-level overrides can further customize branch and PR templates per board.

Authentication tokens are stored in the Canopy keychain (backed by the OS credential store via `PreferencesStore`, keyed by `provider:baseUrl`). Legacy connections that stored tokens directly in preferences are automatically migrated on first load.

Each provider implements a common `TaskTrackerProviderClient` interface. Jira uses the REST v3 and Agile 1.0 APIs. YouTrack uses the Hub REST API. GitHub Issues uses the GraphQL API (with automatic `owner/repo` detection from git remotes when the `projectKey` is empty).

## Behavior

### Connecting a tracker

1. User opens the task tracker settings panel.
2. User selects a provider (Jira, YouTrack, or GitHub) and enters the base URL and project key.
3. For Jira with username/password auth, user provides both username and API token. For bearer token auth (YouTrack, GitHub PAT), user provides only the token.
4. Canopy calls `testConnection` or `testNewConnection` against the provider's user endpoint (`/rest/api/3/myself` for Jira, `/api/users/me` for YouTrack, `{ viewer { login } }` GraphQL query for GitHub).
5. On success, credentials are stored via `keychainSetCredentials(provider, baseUrl, token, username?)`. The tracker definition is saved to the global or repo config.
6. On failure, the provider returns a `ProviderApiError` with the HTTP status and message. The UI shows the error inline.

### Browsing tasks

1. User opens the task list for a connected tracker.
2. Canopy calls `fetchTasks` with optional filters: `statuses`, `assignedToMe`, `boardId`.
3. Jira queries via JQL (`assignee = currentUser()` when `assignedToMe` is true, filtered by board when a `boardId` is set). YouTrack uses its query syntax (`for: me`, `project: {KEY}`). GitHub fetches issues via GraphQL with `IssueFilters`.
4. Tasks are returned as `TrackerTask` objects with normalized fields: `key`, `summary`, `status`, `priority`, `type` (mapped from provider-specific values), `parentKey`, `sprintName`, `assignee`, and `url`.
5. If no tasks match the filters, the UI shows an empty state.
6. Jira and YouTrack fetch up to 200 tasks per request. GitHub fetches up to 100. Jira excludes issues in the "Done" status category by default.

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
4. Task description is truncated to 3000 characters.
5. Attachments can be downloaded to a temp directory (`canopy-attachments-{uuid}` in `os.tmpdir()`). Downloads are capped at 50 MB per file with a 60-second timeout. The download URL must match the connection's `baseUrl` origin.
6. Downloaded attachments are automatically cleaned up after 60 seconds.

### Creating a branch from a task

1. User clicks "Create Branch" on a task.
2. Canopy resolves the branch name using the configured `branchTemplate`. The default template is `{branchType}/{taskKey}-{taskTitle}`.
3. A confirmation dialog shows the proposed branch name.
4. If the working tree has uncommitted changes, Canopy prompts to stash them. If the user declines or the stash fails, the operation is cancelled.
5. Canopy creates and checks out the new branch from the current branch via `gitBranchCreate`.
6. On failure, an error dialog is shown with the Git error message.

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

Conditional sections use `{?varName}content{/varName}` - the content is included only when the variable has a value.

Templates must contain `{taskKey}`. The slugify function lowercases, strips non-alphanumeric characters, replaces spaces with hyphens, and caps at 50 characters. The result is sanitized as a valid Git branch name (no `..`, `~`, `^`, `:`, `?`, `*`, `[`, `]`, `\`, `@`, `#`, `{`, `}`, spaces).

Default type mapping: `bug` to `fix`, `story`/`task`/`subtask`/`epic` to `feat`. Custom type mappings can override this per config level or per board.

### Creating a pull request from a task

1. User triggers PR creation for the current branch/task.
2. Canopy pushes the current branch to the remote (failure is non-fatal).
3. Canopy checks that the GitHub CLI (`gh`) is installed. If not, the operation fails with a `PRCreationFailed` error.
4. Canopy checks for an existing PR on the branch using `gh pr view`. If one exists, its URL is returned without creating a duplicate.
5. If no existing PR, Canopy runs `gh pr create` with the rendered title, body, base branch, head branch, and `--assignee @me`.
6. PR title and body are rendered from the `prTemplate` config using `{taskKey}`, `{taskTitle}`, `{taskType}`, `{parentKey}`, `{boardKey}`, `{taskUrl}`, and `{taskDescription}` placeholders.
7. The target branch is resolved from `targetRules`: if a rule matches the task's type, the rule's `targetPattern` is used (with placeholder substitution and optional lookup against existing branches). Otherwise, `defaultTargetBranch` is used.

### Sending task context to an AI agent

1. User clicks "Send to Agent" on a task.
2. Canopy fetches three things in parallel: the full task details (to get the description, which list fetches omit for performance), comments, and attachments — each with fallback from config-based API to legacy connection-based API.
3. A formatted context string is assembled: task header, metadata (status, priority, type), URL, description, comments, and attachment file references (`@/path/to/file`).
4. The text is wrapped in bracketed-paste markers (`ESC[200~ … ESC[201~`) and written to the agent's PTY as one block, followed by `\r` to submit. The wrapping keeps the CLI's paste detection honest even when the OS (e.g. Windows ConPTY) delivers the write in multiple chunks; without it the tail of a long description can leak in as typed input. Control characters and any stray `ESC[201~` inside the content are sanitised first so they can't hijack the terminal or end the paste early.
5. Attachment files are cleaned up after 60 seconds via `taskTrackerCleanupAttachments`.

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
| Global config     | Preferences key `taskTracker.globalConfig` | `PreferencesStore` (SQLite) |
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
  "boardOverrides": {
    "board-123": {
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

When both global and repo configs exist, `mergeConfigs()` applies these rules:

1. **Trackers**: Additive merge by `id`. If global and repo define a tracker with the same `id`, repo wins.
2. **Branch template**: Repo wins if present, otherwise global, otherwise built-in default.
3. **PR template**: Same precedence as branch template.
4. **Filters**: Repo always wins when repo config exists.
5. **Board overrides**: Shallow merge (repo overrides global on same board ID).

The `ResolvedConfig` includes a `source` object indicating where each field came from (`'repo'`, `'global'`, or `'default'`).

### Board-level overrides

Board overrides are keyed by board ID within `boardOverrides`. When fetching the effective branch or PR template for a specific board:

1. Start with the base template (from repo, global, or default, per merge rules above).
2. If a `boardOverrides[boardId]` entry exists, apply its partial override:
   - For branch templates: override `template`, merge `customVars` (override wins on same key), override `typeMapping`.
   - For PR templates: override individual fields (`titleTemplate`, `bodyTemplate`, `defaultTargetBranch`, `targetRules`).

### Keychain credentials

Credentials are stored at key `taskTracker.token.{provider}:{normalizedBaseUrl}` as JSON: `{ "token": "...", "username": "..." }`. Legacy entries that stored plain token strings are read transparently.

### GitHub auto-detection

When a GitHub tracker has an empty `projectKey`, Canopy reads the git remote URL from the workspace and parses `owner/repo` from it. This supports `git@`, `https://`, and `ssh://` URL formats. If the remote is not a GitHub URL, a `ProviderApiError` is returned.

## Error states

| Error                      | User sees                                        | Cause                                                                     |
| -------------------------- | ------------------------------------------------ | ------------------------------------------------------------------------- |
| `ConnectionNotFound`       | "Connection not found: {id}"                     | Deleted or invalid connection ID referenced                               |
| `AuthTokenMissing`         | "No auth token for {name}"                       | No credentials stored for this tracker's provider/URL pair                |
| `ProviderApiError`         | "{provider} API error {status}: {message}"       | HTTP error from the provider API (auth failure, rate limit, server error) |
| `AttachmentDownloadFailed` | "Failed to download {filename}: {reason}"        | Download timeout, file too large (>50 MB), URL mismatch, or network error |
| `ConfigNotFound`           | "Config not found at {root}/.canopy/config.json" | Repo config file does not exist                                           |
| `ConfigParseError`         | "Invalid config in {root}: {reason}"             | JSON parse error or unsupported config version                            |
| `ConfigWriteError`         | "Failed to write config in {root}: {reason}"     | Filesystem permission error or disk full                                  |
| `PRCreationFailed`         | "PR creation failed: {reason}"                   | `gh` CLI not installed, Git push failure, or `gh pr create` error         |

## Security and privacy

- Authentication tokens are stored via the `KeychainTokenStore`, which persists credentials in `PreferencesStore` keyed by `provider:baseUrl`. The legacy migration moves plaintext tokens from connection-specific preference keys to this store and deletes the originals.
- Attachment downloads validate that the URL origin matches the connection's `baseUrl` before fetching. Downloads are capped at 50 MB and time out after 60 seconds.
- Provider API requests use a 15-second timeout (`AbortSignal.timeout`).
- Jira supports both Basic auth (username + API token) and Bearer token auth, selected based on whether a `username` is present.

## Source files

- Main: `src/main/taskTracker/`
  - `TaskTrackerManager.ts` - core manager with both config-based and legacy connection-based methods
  - `GlobalConfigManager.ts` - global config persistence with legacy migration
  - `RepoConfigManager.ts` - per-repo `.canopy/config.json` management
  - `configMerge.ts` - three-tier config merge logic
  - `configDefaults.ts` - built-in defaults, board-aware template resolution
  - `branchTemplate.ts` - template rendering, slugification, validation, type mapping
  - `prTemplate.ts` - PR title/body rendering, target branch resolution
  - `prCreation.ts` - `gh` CLI integration for push + PR creation
  - `KeychainTokenStore.ts` - credential storage keyed by provider:baseUrl
  - `providers/jira.ts` - Jira REST + Agile API client
  - `providers/youtrack.ts` - YouTrack REST API client
  - `providers/github.ts` - GitHub GraphQL API client
  - `errors.ts` - typed error union with message formatter
- Store: `src/renderer/src/lib/stores/taskTracker.svelte.ts`
- Components: `src/renderer/src/lib/taskTracker/`
  - `branchCreation.ts` - branch creation flow with stash/confirm dialogs
  - `taskContext.ts` - task context formatting for AI agents
  - `providerLabel.ts` - display name mapping for providers
