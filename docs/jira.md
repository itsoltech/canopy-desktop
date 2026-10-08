# Jira Cloud Tasks

Jira is the second `TaskProvider`, alongside GitHub. CI/CD remains a separate
capability. This adapter uses Jira Cloud REST v3 and the Jira Software Agile API;
it is not a Jira Data Center or Service Management portal adapter.

See [task browsing and previews](task-browser.md) for quick project switching, saved
JQL filters and native attachment preview windows.

## Connect and choose a project

Preferences → Integrations → Jira → Connect Jira collects the HTTPS site root,
Atlassian email and API token. Each site has an independent Keychain reference.
Tokens without scopes use the site URL; tokens with scopes use
`https://api.atlassian.com/ex/jira/{cloudId}`. In the latter case verification
checks `serverInfo.baseUrl` against the configured site before checking `/myself`.
A Cloud ID for a different tenant is rejected. Identity is `accountId`, not email
or a mutable display name. Replacing a token preserves its connection ID and
other GitHub/Jira connections; disconnect uses the shared credential-retirement
and SQLite workflow.

The token creation button opens Atlassian's account security page. Unlike GitHub,
Canopy does not claim to preselect scopes on that page. Typical platform scopes
are `read:jira-work`, `write:jira-work`, `read:jira-user`; Jira Software operations
have their additional board/sprint scopes. Account/project permissions remain
independent of token scopes. A verified identity does not prove every action is
permitted in every project.

In Tasks → task source, select the Jira site, browse accessible projects (with
pagination), or enter the project key. Apply stores the mapping by common Git
directory, shared by sibling worktrees. Use origin restores GitHub inference.
Jira site/project is explicit: a Git remote is not evidence of a Jira project.
Task identity includes provider, site, project and issue key. Older GitHub targets
have no site and retain their previous routing and persisted links.

## Task workflow

- Quick filters select active/completed tasks, the current sprint or a custom JQL
  view, while each row and details show the actual workflow status. Search Jira searches text or an exact issue key
  on the server within the selected project. Search uses enhanced `/search/jql`,
  30 issues per cursor page, at most 1,000 retained rows.
- Expanded details keep the existing Description/Comments reading layout and
  POPOVER/STATE_CHANGE motion. Related tasks and subtasks can be opened inside
  the same reading flow. Worktree creation/linking reuses the existing path.
- Create issue loads project-specific createmeta types and all field metadata.
  Subtask creation is supported through a subtask type and its required parent.
- Edit issue refreshes the task first and uses editmeta. Summary, description,
  labels, assignee, priority, dates, components, fix versions, parent and custom
  fields are presented according to their schemas and allowed values. Only
  dirty fields are submitted. Unchanged ADF and other fields are not rewritten.
  String/number/date/boolean and option fields have native controls; unusual
  structured field types retain a JSON editor instead of silently discarding data.
- Status uses the same details dropdown as GitHub and YouTrack, replacing the
  separate Change status buttons. Options use available transition IDs, names,
  destinations and required screen fields (`expand=transitions.fields`), never
  hardcoded To Do → Doing → Done states. A transition without required fields
  executes immediately after revalidation, sending only its ID (no defaults or
  unrelated fields). Missing/incomplete metadata cannot authorize a quick write.
- A transition with required fields, even those with defaults, opens its form.
  The selected action is fixed; a changed workflow never falls back to the first
  transition. Its draft is keyed by transition ID. Required and explicitly edited
  fields plus an optional comment are submitted together; untouched optional
  fields are omitted. Ordinary issue editing excludes status, including old draft
  values. Permissions and workflow validators remain server-authoritative.
- The selector keeps the confirmed status while saving and adopts the server's
  result, not the transition's name. Reads are view-owned and account/workspace
  scoped; writes use IntegrationsState without an edit draft. An uncertain write
  requires an explicit Refresh; successful writes with failed reload remain saved
  with a stale-data warning. There is no polling or automatic mutation retry.
- Add, edit and delete comments use the shared composer, draft store and comment
  list. Deletion asks for confirmation. Comment IDs are scoped to the exact task
  URL; API permissions decide whether another author's comment can be changed.
- Attachment preview, upload, download and deletion use authenticated API endpoints.
  Transfers are limited to 25 MiB. Files are selected explicitly, downloads use
  a Save dialog and an atomic temporary file; attachment URLs from API payloads
  never control where credentials are sent. Deletion rechecks task membership.
- Issue relationships use types from Jira, support both directions, removal
  confirmation and navigation to the linked task. Removal rechecks membership.
- Move to sprint loads project boards and active/future sprints with bounded
  pagination; Backlog uses the Agile API, without guessing a `customfield_*` ID.
- Log work accepts minutes, a local start time and Markdown description. It
  preserves remaining estimate (`adjustEstimate=leave`). It currently adds a
  worklog; editing/deleting existing worklogs is not part of this UI.
- Watch/unwatch, vote/unvote and paginated changelog activity are available.
- Delete issue uses a separate modal requiring the exact issue key; deleting
  subtasks is a separate unchecked option. Confirmation text is not persisted.
  Success removes the local selection/links; local cleanup failure is distinct
  from remote deletion success. Recent deletion tombstones prevent stale search
  pages from resurrecting a task in this running app.

This covers task operations, not every endpoint in Jira's administrative APIs.
Project/workflow administration, bulk migrations, service-desk visibility controls,
marketplace-specific UI widgets and administration of sprint lifecycles remain
outside this adapter. Generic edit metadata is not a guarantee that all vendor
custom fields can use a first-class specialized control.

## Rich content, ownership and failures

ADF is retained in immutable task/comment snapshots. Readers convert headings,
lists, marks, tables, mentions and supported blocks to native Markdown. Embedded
media remains click-to-download in the attachment list; no remote image is
loaded automatically. While editing, nodes that do not map losslessly to Markdown
become labeled local content references. They resolve only against the retained
original document and restore the exact ADF nodes before sending. Invalid or
moved block references fail validation. Drafts retain the original document map,
including across restart. Unsupported structured custom fields remain explicit.

Every write lives in `IntegrationsState`, survives closing its form and is awaited
on normal quit. Form drafts are account/site/project/action scoped and coalesced
into the separate versioned draft table. Failed/uncertain writes preserve inputs.
Network failure or 5xx after dispatch is not automatically retried, especially
for issue/comment creation, upload and worklogs. A successful write followed by a
failed reload returns success with a notice rather than inviting duplicate writes.
Recent returned numeric issue IDs are supplied through `reconcileIssues` during
search to avoid replacing a successful update with stale search-index results.

The existing window-context and notch navigation guards apply to Jira modals.
Changing account/worktree or choosing an agent closes the old form without a
late success stealing focus. Read tasks are owned by their view and cancellation
scope. No new idle polling, shell subprocess or Electron IPC was added.

HTTP headers are sensitive, redirects are disabled, JSON responses are bounded to
8 MiB, regular requests have a 30-second deadline. The shared body adapter also
limits stalled body reads. Field errors are bounded and token-redacted. `base64`
0.22.1 and `chrono` 0.4.45 were already in Cargo.lock and are now direct dependencies
for Basic authentication and timezone-aware date handling; GPUI versions did not change.

## Verification

`tests/jira.rs` exercises connection routing and Cloud ID/site mismatch, enhanced
search/pagination/reconciliation, project field metadata and transitions, task and
comment writes, rich-content preservation, attachment membership and API transfers,
links/watch/votes/sprint/worklog requests, deletion confirmation, partial success,
uncertain writes, SQLite configuration/draft restore and GitHub coexistence.
These use controlled HTTP responses and temporary databases, not a real Jira token.
The full suite passed with `--test-threads=1`. The default parallel run hit the
existing Git metadata watcher timeout; the same watcher test passed in isolation.
This integration does not modify the Git watcher implementation.

The Mac was locked during this implementation, so the new GUI and real Jira tenant
behavior remain unverified. After connecting real credentials, check a representative
project's create/edit/transition screens and permission errors, then restart with an
unsent draft. API success must be observed before claiming tenant qualification.

## References

- Electron contracts: `next:src/main/taskTracker/providers/jira.ts`
  and the task tracker types/components. No Electron credentials were imported.
- [Atlassian token variants](https://support.atlassian.com/atlassian-account/docs/manage-api-tokens-for-your-atlassian-account/)
- [Issues, createmeta, editmeta and transitions](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issues/)
- [Enhanced issue search](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-search/)
- [Attachments](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-attachments/)
- [Issue relationships](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-links/)
- [Sprints](https://developer.atlassian.com/cloud/jira/software/rest/api-group-sprint/)
