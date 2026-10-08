# YouTrack — implementation plan and delegation

Date: 2026-09-10. User requests implementation by **gpt-5.6-luna, max**.
Owner of implementation: delegated Luna worker. Parent owns this plan and independent review.

## Outcome

YouTrack becomes a third usable Tasks provider alongside GitHub and Jira. A user
connects a YouTrack instance in Preferences, selects and switches projects in
Tasks, reads issues/comments and manages ordinary issue work without leaving
Canopy. Existing GitHub/Jira configuration, drafts and linked worktrees remain valid.

This is an issue tracker integration, not CI/CD or a server-administration UI.
No credentials or live tenant have been provided; contract tests use mock HTTP
and isolated SQLite. Report real-tenant validation as outstanding, not as passed.

## Working rules and ownership

- Original implementation context: `/Users/nix/GIT/canopy-desktop-2`, based on
  `108be9b`. Current development is in `canopy-desktop` on `rust-rewrite`;
  see [rust-rewrite.md](rust-rewrite.md).
- There is a substantial **uncommitted, authorized** change set for worktree cleanup,
  button loading/motion, disabled styling and modal interaction. Preserve it.
  Do not reset, checkout, overwrite, reformat unrelated files, commit or push.
- Luna owns the YouTrack adapter, necessary provider-contract/state/UI wiring,
  relevant tests and `docs/youtrack.md`. Shared integration files may be edited
  incrementally. Parent owns this plan and independent verification notes.
- You are not alone in the codebase. Accommodate concurrent work; do not revert
  other edits. Read AGENTS.md and the GPUI skill; use the current locked APIs.
- Preserve existing native design and use ButtonLoading, shared buttons/modals,
  Markdown, task reader/editor, project/filter controls and worktree linking.
- No new dependencies unless a concrete gap requires one. Never write tokens to
  SQLite, logs, error text, tests/fixtures or repo files. Existing credential
  retirement/replacement semantics must continue to apply.

## Architecture

1. Add `Provider::Youtrack`, a site-scoped `AccountScope::Youtrack`, validated
   `ProjectTarget::youtrack`, readable issue keys and correct issue links.
   Keep stable local connection IDs separate from login, server project IDs,
   shortName, readable issue keys and internal issue IDs.
2. Add `integrations/youtrack/` with bounded transport, parsing/reads, schema and
   writes. Implement TaskProvider and the shared client factory. Keep YouTrack
   metadata typed/provider-specific instead of pretending it is Jira ADF/schema.
3. Generalize the **project-listing boundary** used by TaskSourcePicker and
   TaskControls so both Jira and YouTrack can supply paginated project options.
   Do not route YouTrack through a fake Jira SchemaRequest. A small common
   project-page DTO/method with adapters is preferable to broad framework work.
4. Audit provider matches and binary `if Jira { ... } else { GitHub ... }` routing:
   config/credentials, state, query filters, task forms/details, comments,
   attachments, linked tasks and branding all need explicit YouTrack handling.
5. Keep all background writes owned by IntegrationsState; preserve request IDs,
   WriteFinished, uncertain outcomes, cache invalidation, draft retention and
   waiting for writes on quit. UI closures must not own the only write task.

## Connection and transport

- Preferences → Integrations → YouTrack: HTTPS service URL and masked permanent
  token. Email is not required. Support Cloud and current Server REST deployments,
  including a service prefix such as `https://host/youtrack` and explicit HTTPS
  ports. Normalize trailing slash/host without losing the prefix.
- Reject embedded credentials, query/fragment, traversal and malformed URLs.
  Derive `/api/...` beneath the configured service, not with a root-relative join
  that discards `/youtrack`. Never infer a YouTrack host from origin/repo files.
- Verify with `/api/users/me?fields=id,login,name,fullName`. Persist the connection
  only after verification/credential storage succeeds. Preserve other providers,
  support multiple instances, replace/check/disconnect through existing paths.
- Provide a token-help action using the verified JetBrains documentation. Do not
  invent a universal profile URL or a URL that preselects token permissions.
- Auth is sensitive `Authorization: Bearer ...`. Use the existing HTTP client,
  HTTPS validation, no automatic auth redirects, bounded timeouts/response bodies
  and redacted errors. No retries of writes with an uncertain result.
- Handle JSON validation errors, 401, 403, 404, 409, 429, 5xx, malformed/truncated
  success responses and cancellation without claiming an unconfirmed write failed.

## Reading, routing and filters

- Projects: paginate `/api/admin/projects` with explicit fields including internal
  `id`, `shortName`, `name`, `archived`; no fixed first-100 project lookup.
- Issues: explicit `$top`/`$skip` (target page size 50), `fields`, `query`, sort by
  updated. Advance offsets by raw returned entries, not the filtered/rendered
  count. Do not fabricate a total if the response does not supply it.
- Read `customFields`, not the old Electron adapter's suspicious `fields` property.
  Request issue IDs, project identity, summary, description, resolved timestamp,
  tags, custom fields, reporter and attachment metadata required by the UI.
- TaskRef uses a validated readable issue key for display/linking; retain internal
  IDs separately for API operations that require them. Validate that a returned
  issue belongs to the requested project/instance before enabling writes.
- Use the existing project selector, search and reader expansion. Persist target
  and filter choices per provider/site/project; clear stale pages when switching.
- Add YouTrack-native built-ins: active, assigned to me, unassigned, completed,
  all. Add provider selection to custom filters in Preferences. Never send JQL to
  YouTrack or silently reuse Jira filter IDs/defaults.
- Intersect native filters/search with the selected project using a safe parser/
  composition boundary, including OR/quotes/braces. The user filter must not
  remove the project restriction. Validate returned project identity as defense.
- Do not pick the first agile board and call its sprint the current sprint.
  Agile-board/sprint administration is outside this first integration; native
  custom queries can express more specific views where supported.
- Render raw description/comment Markdown with the existing safe renderer; no raw
  server HTML/webview rendering. Handle nullable/deleted comments and pagination.

## Issue management

- Create, edit summary/description, delete issue with explicit confirmation.
- Comments: add, edit and delete using the existing composer, drafts, reader and
  confirmation patterns; server permissions remain authoritative.
- Status/assignee/priority/type and ordinary custom fields: read project/issue
  field definitions and value bundles; serialize the exact field ID and `$type`.
  State fields use StateIssueCustomField and actual bundle values. Derive closed
  status from resolved/isResolved, not English labels such as Done or Fixed.
- For StateMachineIssueCustomField, request `possibleEvents(id,presentation)` and
  submit `event: {id: ...}`. These fields have explicit workflow transitions;
  do not treat them as ordinary freely assignable state values. User bundles
  expose `aggregatedUsers`/individuals/groups rather than generic enum `values`.
- Do not assume the field is literally named State/Assignee. Generic field UI must
  expose actual server names and IDs. Do not treat every user field as Assignee;
  if semantic role is ambiguous, let the user choose the actual field explicitly.
- Support the common scalar, enum/state, user and multi-value field shapes needed
  to create/edit real projects. Respect required/default/read-only metadata.
  Preserve unsupported fields unchanged and explain a required unsupported field;
  never replace it with null or present a raw JSON editor as the normal UX.
- Tags: add/remove existing tags by ID with correct issue association. No global
  tag/project/user administration is needed.
- Prefer structured issue/custom-field endpoints over building free-form commands
  from labels. Workflow scripts may reject a listed state value; show the actual
  rejection and preserve edits. A bundle is not a transition-permission graph.
- After a successful mutation, reconcile the displayed issue without losing the
  current query. A failed follow-up read or tag/attachment step after issue
  creation must produce a saved issue + notice, not invite duplicate creation.
- Unsupported actions (e.g. GitHub milestone operations) must be hidden/disabled
  via provider capabilities rather than falling through to GitHub endpoints.

## Attachments

- Include preview/download, basic multipart upload and explicit deletion. Reuse
  the existing preview/save window and private PreviewFile limits (25 MiB), not a
  second multimedia subsystem.
- Generalize attachment fetching at the provider boundary; preserve Jira behavior.
- Revalidate that attachment ID belongs to the current task. Resolve returned
  relative file URLs against the correct service prefix. Send credentials only to
  a validated same-origin service URL/path. Never follow a remote attachment URL
  and forward the token to an arbitrary host; show a useful limitation/error for
  unsupported external hosting. Do not log signed attachment URLs.

## UI and lifecycle acceptance

- YouTrack appears in Preferences, Tasks source/project selector and provider marks.
  Reuse an existing licensed YouTrack mark from Electron if available; otherwise
  use the existing generic issue icon with a clear YouTrack label, not a fake logo.
- Reuse shared task detail modal, Markdown and native form controls. Read/write
  flows have ButtonLoading, correct disabled styling and no click-through backdrop.
- Configuration survives restart; stale responses cannot overwrite a newly
  selected project/account. Cached data/drafts remain scoped by provider + site.
- Linking issues and creating worktrees from YouTrack tasks use the existing
  TaskRef/project flow; no new Git subprocesses or global agent configuration.

## Verification and delivery order

1. Contracts, URL normalization/routing and transport tests.
2. Paged project/issues/comments read path and query/filter tests.
3. Schema-driven writes, confirmation and uncertain-success tests.
4. Preferences, project/filter controls, reader/editor and attachment UI wiring.
5. Regression checks for existing Jira/GitHub, config/drafts/SQLite round trips,
   cross-instance isolation and safe attachment URL handling.
6. `cargo fmt --all -- --check`; `cargo clippy --locked --all-targets -- -D warnings`;
   adequate test suite; `./scripts/build-macos.sh release`.

Use request-recording mocks similar to tests/jira.rs. Assert HTTP method, path,
fields, query escaping, raw pagination offsets, ID types, bearer header sensitivity,
redirect refusal and bounded errors. Test >1 page of projects, renamed state
fields, empty/multi-value fields, required defaults, 204 writes, partial success,
rate limiting, account replacement and backward-compatible existing snapshots.

Report implementation files, test results, remaining limitations and anything that
requires live credentials. Do not call a mock run a successful live YouTrack test.

## Primary sources and Electron reference

- [REST API overview](https://www.jetbrains.com/help/youtrack/devportal/youtrack-rest-api.html)
- [Service URL and context paths](https://www.jetbrains.com/help/youtrack/devportal/api-url-and-endpoints.html)
- [Permanent tokens and scopes](https://www.jetbrains.com/help/youtrack/devportal/Manage-Permanent-Token.html)
- [Pagination](https://www.jetbrains.com/help/youtrack/devportal/api-concept-pagination.html)
- [Projects](https://www.jetbrains.com/help/youtrack/devportal/resource-api-admin-projects.html)
- [Issues](https://www.jetbrains.com/help/youtrack/devportal/resource-api-issues.html)
- [Custom fields](https://www.jetbrains.com/help/youtrack/devportal/api-concept-custom-fields.html)
- [State-machine fields](https://www.jetbrains.com/help/youtrack/devportal/api-entity-StateMachineIssueCustomField.html)
- [Updating a specific field](https://www.jetbrains.com/help/youtrack/devportal/operations-api-issues-issueID-customFields.html)
- [User bundles](https://www.jetbrains.com/help/youtrack/devportal/api-entity-UserBundle.html)
- [Comments](https://www.jetbrains.com/help/youtrack/devportal/resource-api-issues-issueID-comments.html)
- [Attachments](https://www.jetbrains.com/help/youtrack/devportal/resource-api-issues-issueID-attachments.html)
- Electron implementation: `next:src/main/taskTracker/providers/youtrack.ts`
- Electron contracts: `next:src/main/taskTracker/types.ts`

Electron is behavioral inspiration, not an implementation to copy. It contains
fixed read limits, English-field assumptions and first-board fallback; do not
carry those into Rust. Some older documentation calls this a Hub integration;
the issue operations here use the YouTrack `/api` service.
