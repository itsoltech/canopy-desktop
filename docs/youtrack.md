# YouTrack Tasks

Canopy connects to a user supplied YouTrack Cloud or Server service through its
HTTPS REST API. The service URL is explicit and may include a deployment context
path, for example `https://issues.example.com/youtrack`. A permanent token is
stored in the macOS Keychain; SQLite stores only the connection and credential
references. Canopy verifies `/api/users/me` before saving a connection, and each
service has an independent connection and project selection.

In Tasks, the project picker lists `/api/admin/projects` with `$top`/`$skip`
pagination. The selected project uses its readable `shortName`; issue references
use `idReadable`, while the internal issue ID remains in the in-memory YouTrack
details model for writes, comments and attachments. Project switching is scoped
to the configured service and does not infer a YouTrack host from a Git remote.

The issue list requests a bounded page of 50 records, orders by the server's
updated ordering, and advances offsets by the number of raw API entries. Search
and saved filters use YouTrack's native query language. Every expression is
wrapped with an explicit project clause joined with `AND`, so an `OR` in a custom
filter cannot escape the selected project. Built-ins include active
(`#Unresolved`), assigned to me, unassigned, completed (`#Resolved`) and all.

Issue descriptions and comments are rendered as Markdown. Issue details request
tags, reporter, attachments and `customFields`; a resolved timestamp is the
authoritative closed signal, with state bundle values used only when the field is
absent. Comments are paginated and support add, edit and delete. The server
decides whether a user may change a comment or field.

Issue details have a **Status** dropdown that writes immediately, independently
of the edit draft. Its view (`ui/task_edit/status.rs`) is shared with GitHub and
Jira; provider-specific choices and mutations retain their native semantics. Normal state choices come from the selected issue's state
bundle with bounded pagination; archived values remain readable as the current
value but cannot be chosen as a new status. State-machine fields use only their
current `possibleEvents`. These are workflow action labels, not necessarily
names of destination states. Canopy adopts the refreshed server value after a
write, never the event label. Missing choices and errors remain visible, with
an explicit read-only Refresh action; there is no idle polling. Permissions and
workflow guards are decided by YouTrack, not inferred from bundle membership.

Before a status write, Canopy rereads the issue and validates the field and choice
again, then posts only that field's value or event. A rejected change leaves the
last confirmed value; an uncertain write requires Refresh before another write.
An accepted write with failed reload is reported as saved with stale data, not as
a failure or an invitation to repeat the mutation. Closing the view does not
cancel the globally owned write. Reads are scoped to issue, workspace and account,
and stale results are discarded.

Create and edit forms load the selected project's field definitions. Scalar,
date, period, enum, state, user, group, version and multi-value fields use their
server IDs and `$type` values. Existing-issue edit forms exclude status fields,
including status values in older drafts, so saving other fields cannot undo a
status change made in details. Creation still includes configured status fields.
`state[1]` is recognized as a state; a runtime `StateMachineIssueCustomField`
overrides the project's ordinary state type. Required fields and defaults are
validated locally; unsupported fields remain preserved and read-only. Large
enum, state, version, owned-field and user bundles use paginated endpoints.

Tags use the shared searchable picker and are displayed by their YouTrack names;
the provider ID is kept only as the write identity. Existing tags remain
visible even when they are outside the first page or the current permissions
scope. Attachments can be uploaded, previewed, downloaded through Save as, and deleted. Transfers are
limited to 25 MiB. Attachment URLs are resolved against the configured service
and must remain on the same origin and API path; redirects are disabled and the
token is never sent to an external host.

Writes are owned by `IntegrationsState`. Successful mutations reconcile the issue
when possible, preserve a confirmed create when the follow-up read fails, and do
not retry an uncertain request. Errors for authentication, permissions, conflicts,
rate limits and server failures remain provider-specific and redact the token.

The automated YouTrack coverage uses a request-recording mock HTTP client and
isolated in-memory/configured data. It covers URL and service-prefix validation,
Bearer-header sensitivity, raw pagination, native filter composition, lightweight
issue projection, state-machine events, issue-scoped tags, create reconciliation,
uncertain writes and token redaction. Status regressions cover type recognition,
state bundle pagination, workflow-only events, targeted updates, unchanged and
stale choices, rejected writes, and accepted-write/failed-refresh separation.
No live tenant, credential, or external write was used; real service and GUI
qualification remains separate.

API references: [custom field types](https://www.jetbrains.com/help/youtrack/devportal/api-concept-custom-fields.html),
[specific issue field](https://www.jetbrains.com/help/youtrack/devportal/operations-api-issues-issueID-customFields.html),
[state values](https://www.jetbrains.com/help/youtrack/devportal/resource-api-admin-customFieldSettings-bundles-state-bundleID-values.html),
[state-machine events](https://www.jetbrains.com/help/youtrack/devportal/api-entity-StateMachineIssueCustomField.html).
