# Integrations: Tasks

GitHub.com and [Jira Cloud](jira.md) implement the shared TaskProvider contract for
reading and writing tasks. Integrations account/credential configuration is shared
infrastructure. CI/CD is a separate future capability; GitHub Actions and TeamCity
are not implemented. The sections below describe GitHub; Jira contracts and limits
are documented separately.

## User workflow

Preferences → Integrations supports a default classic PAT for multiple organizations
and additional owner-specific connections (fine-grained PATs). Each connection has
its own masked token, identity check, replacement and disconnect action. Up to 16
connections are supported. A matching organization/username takes precedence over
the default connection, case-insensitively. There is no retry with another token
after an access error. Without a matching owner or default connection, Tasks shows
a configuration prompt. Linked tasks resolve credentials from their own repository.

The Create token with required permissions button opens GitHub with a prefilled
form: classic `repo`, or fine-grained `issues=write` and `metadata=read` plus the
entered resource owner. Classic `repo` also grants write access at GitHub; Canopy supports reading and updating issues through explicit user actions. The user selects the repositories, expiration
and approves creation on GitHub. SSO authorization for each organization, approval
of fine-grained tokens and organization restrictions still apply. No `workflow`,
`read:org`, administration or future CI permissions are requested in this stage.
Checking a token calls GET /user; success verifies identity, not access to every
organization or repository. Repository access is checked when loading its issues.

Tokens are stored only in macOS Keychain under a separate integration service.
No gh credentials or Electron secrets are imported automatically. This is PAT
authentication, not an OAuth/device-flow application. Older single connections
retain their default routing without inferring or changing their token permissions.

Tasks detects owner/repository from origin (SSH scp-style, SSH URLs, HTTPS and the
ssh.github.com alias). Git metadata runs on the existing libgit2 worker. An explicit
repository override in Tasks is keyed by the repository's common Git directory,
so sibling worktrees share it. Origin changes update inferred mapping while an
explicit override wins. Unknown SSH aliases and Enterprise hosts require future
provider-host support; API requests currently target api.github.com only.

The sidebar shows tasks linked to the current worktree and opens Tasks in the
inspector. Tasks provides Open/Closed pages, local filtering over loaded titles,
numbers, labels and assignees, descriptions, an Open on GitHub action, local linking
and unlinking, and New worktree through the existing creation dialog. Its branch
and destination are suggestions the user can edit. Successful local creation is
followed by a SQLite task association; failures are visible and do not delete the
created worktree. Linked task identity is provider + project key + issue number,
not title or row index. Clicking a linked task fetches its details even when absent
from the loaded page or in the other state filter.

## Runtime and persistence

`integrations/` contains normalized immutable task data, configuration, Keychain
access and the GitHub adapter. `AppState.integrations` owns connection work,
current project resolution, bounded in-memory caches and async requests.
Desktop startup explicitly installs the shared real HTTP transport using
`Application::with_http_client`; native GPUI's default is a null client. The
transport is `gpui-pre-reqwest-client` 0.3.8, pinned to the facade's GPUI snapshot
as a direct dependency. It uses platform TLS certificate
verification and normal proxy discovery, preserves request redirect/deadline
policies and limits idle body reads to 20 seconds. A streaming body adapter enters
the transport's Tokio context for each read poll: 0.3.8 returns the body to GPUI,
and body timers otherwise panic outside Tokio.

`SettingsClient` serializes versioned `_canopy_rust_integrations` writes separately
from ordinary preferences. It stores account identity, opaque Keychain references,
repo overrides and task links by worktree path; no access tokens or issue bodies.
Invalid/future payloads are rejected without replacement. Layout persists the Tasks
inspector selection with a default for older snapshots.

Configuration writes are serialized. Replacing a token first verifies/stores the
new credential, persists its reference, then retires only that connection's previous
credential. Connection IDs remain stable during replacement; other connections are preserved.
Disconnect persists removal of the account before deleting the secret. Retired
credential references survive failed cleanup, with a retry action in Preferences.
Failed persistence keeps the previous active configuration. Quit awaits active
configuration writes; outstanding list/detail/origin reads are canceled.

Lists fetch only when Tasks is opened/refreshed or Load more is requested. No
background polling is added. Cached pages are keyed by credential reference/project/state and
never reused across accounts; switching accounts clears selected task details.
Changing worktree, target, filter or connection cancels stale reads. Fetches have
20-second deadlines and a 4 MiB response bound. Authorization headers are marked
sensitive and redirects are not followed. Diagnostics do not include tokens or
raw HTTP error bodies. Rate-limit and permission failures leave a visible error
and permit a later manual retry.

Lists use GraphQL `repository.issues(first: 30)` ordered by updated time, so PRs
never occupy issue page slots. `pageInfo.endCursor`/`hasNextPage` drive continuation;
`totalCount` supplies the loaded/total indicator for Open or Closed. Cursors and
totals are cached with their account/repository/filter, and malformed cursors or
GraphQL errors (including HTTP 200 with partial data) never advance the list.
The loaded list is bounded at 1,000 issues per target/filter; search is local,
not a server-wide issue search. Descriptions are rendered as native GFM Markdown, capped at
65,536 characters. Images become links opened on demand; raw HTML is displayed as
text, so descriptions cannot automatically load remote or local image files. GitHub
Projects boards, attachment uploads and administrative issue operations remain outside
the daily editing workflow described below.

## Validation

Tests cover SSH/HTTPS origin detection, manual override precedence, linked-worktree
identity, read-only GraphQL queries, sensitive Authorization/no redirects, REST identity lookup,
issue-only pagination, cursor handling, GraphQL partial errors, sanitized HTTP errors, SQLite restore and protection of
future configuration versions. Multi-organization regression tests cover owner
precedence, missing routes, replacement isolation, duplicate owner/default rejection,
legacy account restore and prefilled permission URLs.
Live private-repository access requires the user to connect an account in Preferences;
no authenticated GitHub success is claimed without that check.

References: https://docs.github.com/en/rest/issues/issues and
https://docs.github.com/en/rest/authentication/authenticating-to-the-rest-api .
REST API version 2022-11-28 is explicitly pinned and remains supported per GitHub's
version policy: https://docs.github.com/en/rest/about-the-rest-api/api-versions .

Release GUI checked the inferred origin label, Tasks empty state, direct opening
of Preferences → Integrations, masked-token control and empty-token validation.
Authenticated issue browsing and the complete task-to-worktree flow still require
a user-configured GitHub account for live end-to-end qualification.

Token setup references:
- https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens
- https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry
  (documents the classic token form's `scopes` parameter).

Multi-organization release GUI qualification used three explicitly seeded fixture
connections without Keychain secrets. Verified navigation from regular Preferences,
three independent connection rows, owner prefill on Replace token, automatic
scrolling to the replacement form, both token modes and missing-owner validation.
No live token was created and no private repository was accessed in this check.

Transport regression checks use a real local HTTP server for header delivery,
NoFollow redirects and response-body deadlines. An explicit ignored live test
contacts GitHub with a deliberately invalid token and requires its authentication
response, distinguishing actual HTTPS transport from mocked responses. It does
not authenticate as the user or read private repository data.

Live transport qualification passed: public GitHub API response HTTP 200 with JSON
body consumption and HTTP 401 for a deliberately invalid token. The release GUI
form also received the expected GitHub authentication error for that test value,
confirming the production bootstrap installs the client. No user token was read.

## Presentation

Preferences uses divider-separated connections with compact identity, replacement
and disconnect actions. The connection form opens on Add/Replace and closes after
saving; token scopes, SSO/approval requirements and Keychain storage remain visible.
Tasks uses a provider header, compact Open/Closed filters, status markers and label
chips. Details keep the issue body scrollable and worktree actions at the bottom.
Unconfigured, loading and empty-filter states have dedicated presentation. Shared
provider context and feedback live in `ui/components/integrations.rs`; issue rows,
metadata and loading placeholders live in `ui/components/tasks.rs`.

Visual qualification used actual GPUI views with temporary sample data: two GitHub
connections, adding a token, Preferences at 720 px width, long task titles, selected
issue details, list scrolling at 240 px width and empty filtering. The sample harness
was removed from the checkout and is not part of the application.

## Task description Markdown

`ui/markdown.rs` owns a persistent GPUI `TextViewState` for the selected task.
Source changes are prepared off the UI thread; unchanged descriptions keep their
state, and generation checks discard old preparation results. Switching tasks
clears the previous content immediately. Closing details disables selection/link
activation during the exit transition. The native view shares the details scroller
and supports selection, headings, emphasis, GFM lists/checklists, quotes, tables,
links and fenced/inline code. Colors, heading sizes, code surfaces and spacing use
Canopy tokens; wide tables can scroll horizontally and code wraps within the available width.

`markdown` 1.0.0 was already present through GPUI Kit and is now directly referenced
for AST-based preparation. Browser links resolve against the issue URL and accept
HTTP(S) only; images are represented by links and raw HTML is literal. Tests cover
formatting preservation, Unicode, image references, code containing HTML/image
examples and URL handling. No new parser version or web view was introduced.

Markdown GUI qualification used the production view with sample headings, emphasis,
checklists, nested lists, quotes, code, tables and image links at 380 px and 240 px
width. Switching to a second description removed the previous content. The temporary
preview harness was removed; no user issues or credentials were used.

The issue list now uses https://docs.github.com/en/graphql/guides/using-pagination-in-the-graphql-api
and the Issues read permission documented at
https://docs.github.com/en/graphql/guides/forming-calls-with-graphql .
A read-only live check through the existing Canopy connection returned all six open
issues for itsoltech/canopy-desktop in one page, with totalCount 6 and no next page.
The shared GitHub heading places the logo in the title's explicit 20 px line box;
the subtitle remains aligned with the title in Tasks and Preferences.

Release GUI checked the shared logo/title alignment in Tasks and Preferences using
an isolated data directory. Pagination tests cover 30+1 issues, a complete six-issue
repository, Open/Closed variables and rejected partial/error pages.

## Expanded task details

The expand action in the inspector opens a large, centered modal without replacing
the selected task or list. Description and Comments use separate reading surfaces;
wide windows keep metadata beside them and narrower windows move metadata into the
description. The reading type scale is larger than the compact inspector. Link and
New worktree remain at the bottom; New worktree closes the preview before opening
the existing creation dialog with its task/branch suggestions.

Comments are fetched only when expanded details open or the user refreshes/loads
another page. `TaskProvider::comments` is provider-neutral; GitHub uses read-only
`repository.issue.comments` queries with 30 comments per cursor page. Author, UTC
creation time, stable ID, Markdown body and browser URL are normalized. Deleted
authors are represented explicitly. Partial GraphQL errors, invalid cursors and
unsafe comment URLs are rejected. The dialog retains at most 200 comments and
shows a browser handoff when that limit is reached. Variable-height comments are
virtualized; Markdown height changes invalidate only their list row.

POPOVER controls entrance/exit (250/150 ms), with the shared backdrop and 8 px
movement. STATE_CHANGE controls Description/Comments transitions. All respect
Reduce Motion. Content actions stay disabled during transition, closing cancels
comment requests, and the modal remains mounted through exit. Explicit initial
focus supports Escape; normal close restores the previous focus. Changing the
worktree/credential closes the preview without restoring stale focus. Agent focus
requests also dismiss it; the large modal counts as obscuring the terminal for
notch visibility. Main-window shortcuts and native video are blocked/hidden while
it is open.

The release GUI fixture verified a long Markdown description, the metadata column,
responsive layout down to 800 x 500, 30+2 comments, local task linking, isolation of
Cmd+T and transition into the existing worktree form. The test discovered missing
initial focus for Escape; explicit focus was added. Follow-up visual verification
of that fix and Reduce Motion was interrupted by the Mac screen lock. Unit tests
cover comment normalization, pagination, deleted authors, partial errors and URLs.
No real comment was created or edited; the GUI fixture uses a test-only credential.

The task reader's presentation is extracted into `components/task_detail.rs`.
Controllers keep their data, focus, cancellation and motion ownership; comment
Markdown/height state lives in `task_detail/comment_card.rs`. This refactor changes
no provider calls, pagination policy or animation timings. GUI re-verification
remains blocked by the locked Mac; source checks and automated checks are separate
from the earlier successful visual qualification.

## Daily issue editing

Tasks has a New issue action. Existing issues can change title/body, open/closed
state, individual labels/assignees and milestone. Repository option lists paginate
in sets of 100 and can be filtered locally. Comments support Markdown composition
with preview, editing and explicit per-comment delete confirmation. New issue and
text editors are separate modal states, reusing the task reader's focus and motion
contracts. Closing keeps local drafts; saving shows the server's returned data.

`TaskProvider::write` and `options` use provider-neutral commands/results. GitHub
writes use REST POST/PATCH/DELETE with the existing transport: 201 and 204 are
accepted; request/response failures after dispatch are treated as uncertain.
There is no automatic retry. Reads needed after an accepted label operation may
fail independently; the result still reports that the change was saved. Returned
metadata is checked for GitHub's documented silent omission of fields when the
caller lacks repository permissions. Labels and assignees use targeted add/remove
endpoints rather than replacing a stale full set. Comment IDs are resolved only
from a matching task's canonical GitHub comment URL.

One global integration write runs at a time. It survives closing the initiating
view and quit awaits it. Results invalidate task caches, update matching readers
and reconcile comments by stable ID while maintaining chronological order. A
created issue opens in the reader only if its originating worktree is still active.
Switching account/worktree closes the editor and preserves its original draft.

`DraftsState` coalesces local drafts independently of network busy state. A separate
versioned `_canopy_rust_task_drafts` SQLite table stores values and editing baselines;
keys include connection ID, verified login, repository and editor kind. Drafts contain
ordinary user text, not access tokens. Failed/uncertain writes retain the draft and
its warning; success removes it. Corrupt/newer draft schemas are not overwritten.
Normal quit flushes API results and drafts before stopping the remaining services;
a draft save failure leaves the app open. Force-kill durability is not promised.

The current workflow covers create, title/body, close/reopen, label assignment,
assignees, milestones and comments. It does not implement issue deletion/transfer,
locking/pinning, Projects, issue relations, label-definition administration or file
uploads. These were offered as an optional broader scope; no answer was received,
so implementation followed the stated daily-workflow assumption.

Validation: mock transport tests exercise successful mutations, exact targeted
payloads, 201/204, partial metadata success, uncertain outcomes without retries,
permissions/rate limits, comment ownership and draft persistence/future-schema
protection. No real issue or comment was created, modified or deleted in testing.
The Mac remained locked, so end-to-end interaction with the new editors still
requires GUI qualification after unlock.
