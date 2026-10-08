# Task project switching, quick filters and attachment previews

## Jira projects and filters

`TaskControls` is always visible above the Jira task list. Its first dropdown lists
accessible **projects**, such as GAKKO and ISSUE; these are distinct from Agile
Scrum/Kanban boards. The list loads on first showing Tasks for a connection, supports
search and pagination, and retains the current project even if discovery fails.
Switching projects immediately saves the existing repository override without an
Apply step. The cached repository context makes the selection visible without
waiting for another job on the libgit2 worker. The gear retains provider/site setup.
Project-list results belong to the credential identity; account changes cancel them.

The second dropdown contains seven built-in Jira filters:

- Active tasks.
- My active tasks.
- Unassigned active tasks.
- Current sprint (including completed tasks in that sprint).
- Unassigned in the current sprint.
- Completed tasks.
- All tasks.

`currentUser()` and `openSprints()` are evaluated by Jira, not cached as account or
sprint IDs. All views stay within the chosen project. The previous Active/Done
segments remain for GitHub; Jira filters replace those segments so a custom filter
is not silently intersected with an old Active/Done selection.

Preferences → **Task filters** provides add, edit, duplicate-from-built-in and
confirmed delete. A custom filter has a stable UUID, name, provider and JQL condition.
The condition is global within Canopy's Jira filters; the current project is applied
separately. Empty condition means all tasks in the selected project. `ORDER BY` is
not accepted; the task browser orders by latest update. Bounds and balanced quotes/
parentheses are checked locally, while Jira validates its fields/functions. This is
local Canopy configuration, not publishing a saved filter on the Jira server.

The selected filter is persisted per Jira site/project; deleting a custom filter
returns affected selections to Active tasks. Definitions and selections use the
existing serialized configuration writer and backward-compatible serde defaults.
The cache key includes site/project, credentials, search and effective expression.
Changing or deleting the active expression clears pages and cursors; late results
cannot replace a different view. Renaming a filter does not need another API read.

## Attachment previews

Click an attachment name in Jira details to open a separate **Canopy — Attachment**
window. The download icon still opens Save as. Preview uses macOS `QLPreviewView`,
so images, PDFs, video and other types supported by the system can be inspected
without building another multimedia editor. Video does not autoplay. Native
controls and decoded-format support depend on macOS. Unsupported content retains
its Save as action; it is not silently opened in another application.

The preview fetches the attachment through the existing authenticated Jira API
path, including its issue-membership check and 25 MiB limit. It never gives the
API token or an API-provided remote URL to Quick Look. The downloaded data lives
in a unique mode-0700 temporary directory with a mode-0400 file, not in the user's
worktree. Filenames cannot escape that directory and retain their extension.
Quick Look owns an `Arc` to the file cache until native close/cleanup completes.
Closing the preview removes the temporary file; no preview tab or temp path is
persisted across app restarts. Force-killing the process has no normal-drop guarantee.

Each preview has Root, the shared titlebar, owned focus/read/save tasks and a
CONTENT_REVEAL transition respecting Reduce Motion. Escape and Cmd+W close only
the preview, including when focus is inside Quick Look. Native key-monitor callbacks
only queue a message; GPUI removes the window on its executor. The monitor and
QLPreviewView are released together. Reopening the same attachment reuses its live
window, and closed handles are pruned. Credential replacement/disconnect or deletion
of the attachment/issue closes its preview. Normal quit cancels these read views.
Save as is an explicit atomic copy and leaves the cached attachment read-only.

## Verification scope

`tests/task_filters.rs` covers built-in semantics, JQL composition boundaries,
per-project/site choices, edit/delete behavior, SQLite restore and old configuration,
private temporary-file modes/lifecycle and explicit Save as. `tests/jira.rs` verifies
that the custom condition reaches the API and All tasks does not inherit Active.
The macOS bridge is compiled against the local SDK's QLPreviewView API.

Computer Use returned **Mac locked** during this change. Native appearance,
Quick Look decoder behavior, focus/IME, pointer interactions, rapid dropdown
switching and window resize have not been qualified in the running GUI.

GPUI review checklist:

| Area | Result / evidence |
| --- | --- |
| Versions and API | Yes: existing Cargo.lock/GPUI; QLPreviewView and SelectState checked in SDK/pinned sources. |
| State and identity | Yes: owned entities/tasks, stable filter UUIDs and project keys, credential-scoped discovery. |
| Render / queues | Yes: no I/O or worker creation in render; bounded filters/pages and on-demand previews. |
| Async and errors | Yes: request ownership and cache identity reject stale results; API/storage errors remain visible. |
| Layout / theme / input | No GUI qualification: finite viewports, semantic tokens and owned focus in source only. |
| Windows and memory | Source-reviewed plus cache lifecycle tests; native repeated-open/close GUI test remains pending. |
| Accessibility / FPS | Not claimed; no accessibility or presentation-rate measurement was performed. |
| Distribution | Bridge linked into the macOS app build; launch outside the checkout remains unverified. |
| Regression evidence | API, configuration and temp-file tests are separate from GUI verification. |

References:
- [JQL functions](https://support.atlassian.com/jira-software-cloud/docs/jql-functions/)
- [QLPreviewView](https://developer.apple.com/documentation/quicklookui/qlpreviewview)
