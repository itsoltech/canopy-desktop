# TeamCity CI/CD

> Build status per branch, triggering any job on any branch, and a live view of the server's activity — from the CI/CD sidebar section and the branch context menu.

**Status:** Experimental (behind the CI/CD sidebar section, hidden by default)
**Introduced:** v0.13.0
**Platforms:** All

## Overview

The integration follows the Project management architecture:

- **Repo-owned configuration**: which TeamCity server a repository uses and which build
  configurations ("jobs") are available in it live in the git-tracked
  `.canopy/config.json`, written by the per-repo configurator (CI/CD sidebar section →
  gear icon). Committing the file shares the setup with the whole team.
- **Personal credentials**: access tokens are stored per server URL in the OS-encrypted
  keychain, managed globally in **Settings → CI connections** (its own section, separate from the Project management connections; the TeamCity add form carries a "Generate →" link to the server's token page once the URL is typed).
- **Personal opt-in**: the whole feature is gated by the **CI/CD sidebar section**
  (`sidebar.sections`, hidden by default — enable it in Settings → Sidebar). A `ci`
  block arriving via the git-shared config never turns the feature on by itself.
  Repositories without a `ci` block are unaffected either way: no rows, no requests,
  no polling.

## Behavior

### CI/CD sidebar section

- **Not configured**: a "Configure TeamCity" entry opens the per-repo configurator.
- **Configured**: the server row (click opens TeamCity), a highlighted **Last job** card
  with the newest build of the active worktree's branch (build number and status chip;
  clicking the card opens that build in TeamCity). A row whose status fetch fails —
  e.g. its job was deleted or re-ided on the server — shows an `Unavailable` chip with
  the reason, and only that row degrades, never the whole card. The chip vocabulary is
  shared with the activity window: `SUCCESS` → **Success**, `FAILURE` and TeamCity's
  `ERROR` → **Failed** (ERROR is an infra/agent failure — red in TeamCity's own UI, so
  it must not read as neutral), anything else — including cancelled builds, which carry
  `UNKNOWN` — → **Unknown**. Then a **Run job…**
  entry, and a summary
  row labelled **Jobs history** — **Running job** while anything is active — whose chip
  shows "2 running · 1 queued" or "Idle". Clicking the row opens the activity window
  with the detailed list.
- **Token missing**: a banner links to Settings → CI connections.

### Per-repo configurator (modal)

Pick one of your CI servers (or add a new one: URL + token with a connection test via
`GET /app/rest/server` and an explicit destination confirm before the token is stored),
then **Load available jobs** and choose, from the server's full list grouped by
TeamCity project, which jobs are available in this repository — analogous to the
project/board selection in Project management. Sidebar labels are editable. Configured
jobs the server no longer returns are named in a warning and dropped on the next Save
(when all of them are stale, Save stays disabled until a live job is ticked). Saving
writes the `ci` block to `.canopy/config.json`; the modal can also remove it.

### Run job… (any job, any branch)

A centered modal (rendered from the app layer — sidebar-hosted dialogs would be
pinned to the sidebar column by its backdrop-filter): pick a configured job and a
branch through a searchable list (branches come from TeamCity itself —
`/app/rest/buildTypes/id:X/branches`; typing filters the list and a branch must be picked).
The generic sidebar **Run job…** entry leaves the branch empty, so an active `develop`
worktree is never armed implicitly. The worktree context menu (right-click a branch in
PROJECTS → **Run CI Job on Branch…**) explicitly prefills that worktree's branch, and
the prefilled branch stays selected even when TeamCity has not listed it yet. Otherwise
the dialog never picks a branch on its own, and **Run** stays disabled until one is
chosen. The GIT section deliberately carries no CI entries; it holds CI-independent git
actions. Parameter metadata is loaded for the selected job before submission. If the job
prompts for parameters (TeamCity's "Run custom build"), the primary action says
**Configure** and Canopy shows an equivalent dynamic form:
text inputs with descriptions, checkboxes honoring custom checked/unchecked values,
single selects, multi-selects with All/None joined by the spec's value separator, and
masked inputs for `password` parameters. Password prompts always start **empty** — the
value is never fetched into the app, so a secret cannot appear on screen, in a
screenshot, or in the renderer's heap. Leave one blank to run with the value stored on
the TeamCity server (the property is omitted from the trigger payload); type into it
only to override that value for this run.
Fields are prefilled with the configuration's current values; checkboxes follow
TeamCity's dialog semantics exactly — an unchecked checkbox submits its
`uncheckedValue` (configs may carry whole CLI fragments there), never the raw stored
value. Required parameters (`validationMode='not_empty'`) block submission. Chosen
values are sent as build `properties`. Only jobs without prompt parameters expose
**Run** and queue immediately. TeamCity's `[0xNNNN]` and legacy `|0xNNNN` escapes in
parameter labels, descriptions, and options are decoded to Unicode for display;
malformed escapes remain literal. A build triggered from Canopy is then **observed to
completion**: the app polls it every 10 s and shows a green or red toast with the outcome
when it finishes
(`SUCCESS` → succeeded; `FAILURE` and TeamCity's `ERROR` → failed; anything else →
"finished with unknown status"). The poll gives up after ~5 minutes of consecutive
API failures (a suspend/resume or VPN reconnect survives) or after two hours total,
and says so with a "Stopped watching…" toast naming the job and its build number.
That toast is **sticky** — it stays until dismissed (✕ or Escape), because this
state has no other surface in the app. A transient toast arriving meanwhile (a
queue confirmation, another build's outcome) folds into the sticky message instead
of taking the slot or being dropped — capped at the newest few, with the most
severe folded outcome deciding the toast's color (a green result never repaints a
slot that still carries a failure); a URL toast from a terminal link click only
displaces the sticky message temporarily (it returns when the URL toast goes).
When one outage costs several observed builds their watcher, the unacknowledged
give-ups aggregate into one toast that still names each job and build number. The
build itself is unaffected.

### Activity

The sidebar carries only a one-row summary (running/queued counts, or "Idle");
clicking it opens a dedicated activity window with the details. Canopy scopes its TeamCity
queries to build configurations selected in this repository's `.canopy/config.json` and
fetches up to 20 running builds, their queue and 10 recent matching builds. Job name, branch,
TeamCity's build-specific `statusText`, outcome and progress are shown per row (start
time + duration for finished builds — same-day times as HH:MM, older ones as
YYYY-MM-DD HH:MM; elapsed time for running); click opens the
build in TeamCity. The summary chip shows a single running build's percentage and the
queued count, refreshes immediately after a trigger from Canopy, and polls every 30 s
(10 s while anything is active) while the section is mounted; the window is resizable
and refreshes every 10 s while open.

The running, queued and recent queries degrade independently. If one or two slices fail, the
sidebar shows a **Partial** chip (the tooltip carries the reasons), its live announcement says that
CI activity is partial, and the activity window keeps the available builds with a **Partial
history** banner. If all three queries fail, the row keeps the **Error** state and its full reason.

## Configuration

Written by the configurator (hand-editing works too) in `.canopy/config.json`:

```json
{
  "ci": {
    "provider": "teamcity",
    "baseUrl": "https://teamcity.example.com",
    "buildTypes": [
      { "id": "Gakko_Build", "label": "Build" },
      { "id": "Gakko_Tests", "label": "Tests" }
    ]
  }
}
```

- `provider` — `teamcity` selects this provider; GitHub Actions uses a separate configuration
  shape documented in [github-actions.md](github-actions.md).
- `baseUrl` — http(s) origin of the TeamCity server. All requests go to this origin
  only (the renderer never supplies a URL for repo-scoped calls).
- `buildTypes[].id` — TeamCity build configuration id (`[A-Za-z0-9_]`, ≤255 chars).
  An entry whose id violates the charset (a hand-edited typo like `Gakko-Build`) is
  dropped at read time but **counted and named** in the configurator's warning, with
  a correct-the-file recovery (it is not a TeamCity id, so the picker cannot show
  it) — and when every entry is invalid, the block-scope error itself names the ids.
  It never silently disappears on Save.
- `buildTypes[].label` — sidebar label; defaults to the id, capped at 100 chars.
- `buildTypes` — at most 50 entries when read; duplicate `id`s collapse to the first
  occurrence and extras beyond the cap are ignored rather than rejected (the raw
  block is preserved on disk). The configurator names the dropped entries with the
  recovery matching their cause — typo'd ids point at fixing the file, cap overflow
  at re-ticking in the picker (when the job still exists on the server; ids the
  server no longer has are named as unrecoverable) — before a Save would drop them for real, and itself
  refuses to select more than 50 (Save is disabled with the count named); the
  `ci:saveConfig` IPC path rejects >50 outright.

`ci:config` answers with a structured result: `{ config }` when the block is valid,
`{ config: null }` alone when the repo genuinely has no CI (every "Configure
TeamCity" entry keys on this), and `{ config: null, invalid: { scope, message } }`
when a block exists but cannot be used.

The block is validated at read time (`parseCiConfig`, applied by `CiManager`); a
malformed block reads as `CiConfigInvalid` — kept distinct from "not configured",
because the surfaces that hit it exist precisely because the block is there. The raw
value is preserved verbatim across config saves — Settings actions that round-trip
`.canopy/config.json` never delete or rewrite a hand-edited `ci` block, even an
invalid one, and a save against an unparseable config file fails instead of
re-initializing it (initialization is reserved for a genuinely absent file).

## Error states

The provider-shared `CiError` union also contains GitHub Actions dispatch, ref, schema and rate-limit
variants. The TeamCity paths use these variants:

| Variant              | Meaning                                                                                                                                                                                                                                                                                                                                      | Surface                                                                                                                                                                                                                                                                                                                                        |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `CiNotConfigured`    | No `ci` block in the repo config                                                                                                                                                                                                                                                                                                             | Section shows its "configure" entry; `ci:status` answers `{ configured: false }` (a silent no-op)                                                                                                                                                                                                                                              |
| `CiConfigInvalid`    | A `ci` block exists but cannot be used. Two scopes, distinguished in the message: the whole `.canopy/config.json` cannot be used (bad JSON, unsupported version, a legacy tracker provider — the configurator disables **Save** and points at hand-editing) or only the block's shape is rejected by `parseCiConfig` (re-saving replaces it) | The CI/CD section shows the reason (front-loaded, with an "Open the configurator" button) instead of the "configure" entry, announced by the live region; `ci:config` answers `{ config: null, invalid: { scope, message } }` so the configurator offers the one recovery route the scope calls for; `ci:status` is never polled in this state |
| `CiConfigUnwritable` | A local filesystem failure while updating `.canopy/config.json` (permissions, disk, a transient read error on an existing file)                                                                                                                                                                                                              | The configurator's footer live region — never with the `TeamCity:` prefix, because nothing in the save chain talks to the server                                                                                                                                                                                                               |
| `CiAuthMissing`      | No token stored for the configured server                                                                                                                                                                                                                                                                                                    | Credential banners linking to Settings → CI connections                                                                                                                                                                                                                                                                                        |
| `CiApiError`         | HTTP/network/API failure                                                                                                                                                                                                                                                                                                                     | When every activity query fails, **Jobs history** shows an `Error` chip (full message in its tooltip); one or two failed slices use the documented **Partial** state instead. Affected **Last job** rows show an `Unavailable` chip carrying the failure (the muted `Last job unavailable` line only appears when `ci:status` fails as a whole, which the credential banner otherwise pre-empts)                     |

Additional surfaces that are not `CiError` variants:

- Failures from an action taken inside a modal report inside that modal, never as a
  toast: the toast layer (`z-banner`) paints under a dialog's own scrim (`z-overlay`),
  so the message would be dimmed and unclickable. A failed trigger lands in the Run
  dialog (the picker's live region, or the parameters form's footer) via
  `triggerCiBuild`'s returned message; a failed **Save configuration** or **Remove CI
  configuration** lands in the configurator's footer live region. Because that
  message has no home outside the dialog, dismissal (Escape, backdrop, ✕,
  **Cancel**) is refused while the request is in flight — the controls dim and
  name the reason in their tooltip, and become live again as soon as it resolves.
- The completion toast for a build triggered from Canopy (see Run job…) comes from a
  background `ci:build` poll, not from `CiError` handling — poll failures are tolerated
  (~5 minutes of consecutive failures stop the observation with a "stopped watching"
  toast).
- A branch with no builds is not an error: the Last-job card shows a "No builds" chip
  and running a job stays available.

## Security and privacy

- `repoRoot` on every repo-scoped channel (`ci:config`, `ci:status`, `ci:trigger`,
  `ci:activity`, `ci:branches`, `ci:buildParameters`, `ci:saveConfig`, `ci:build`,
  `ci:jobsStatus`, `ci:jobRefs`, `ci:jobParameters`, `ci:triggerJob`, `ci:runActivity`,
  `ci:run`, `ci:githubSetup`, `ci:testGitHubConnection`, `ci:setGitHubCredential`)
  is resolved through the same workspace-authorization gate `repoConfig:*` uses, and
  only the **resolved** path reaches `CiManager` — a renderer cannot point
  `ci:saveConfig` at an arbitrary writable directory, nor drive `ci:trigger` off a
  config it planted outside the workspace to spend the stored token.
- Access tokens are stored via `KeychainTokenStore` keyed `teamcity:<baseUrl>`,
  encrypted at rest via Electron `safeStorage` (OS-native: DPAPI / Keychain / keyring;
  plaintext fallback in Canopy's local DB when no OS keyring is available). They are
  trimmed and size-bounded before storage or use, and never written to
  `.canopy/config.json` or any other repository file.
- For repo-scoped calls (status, trigger, parameters, branches, activity) the server
  URL always comes from the repo config — the renderer never supplies a URL, so a
  compromised page cannot redirect requests. Redirects are refused
  (`redirect: 'error'`) so the Bearer token cannot be forwarded to another host, and
  every request times out after 15 s. Stored tokens are looked up by the exact
  `baseUrl` — no stored credential for an unknown URL means no token is sent.
- `baseUrl` lives in a **git-shared** file and `http://` is accepted (plaintext
  transport). Every path where a token typed into a form is first stored (Settings → CI
  connections, and the configurator) therefore requires an explicit
  confirm naming the exact URL, with a warning for plain `http://`.
- `password` prompt parameters never reach the renderer: their server-side value is
  discarded at parse time in the main process, the field starts empty, and an
  untouched field is omitted from the trigger payload so TeamCity uses its stored
  secret.
- Build type ids and branch names cross the IPC boundary through strict charsets
  (`BUILD_TYPE_ID_PATTERN` = `[A-Za-z0-9_]{1,255}`, branch = `[A-Za-z0-9._/-]{1,255}`)
  that cannot escape a parenthesized TeamCity locator value. Triggering, activity, and
  parameter/branch queries are limited to build types declared in the repo config. The
  activity queries interpolate those ids into a `buildType:(item:(id:…),…)` locator, so
  nothing outside the repository's configured jobs is fetched or shown. Custom build
  properties are validated (name charset, value size, count cap) before they reach the
  request body.

## Source files

| Area                  | Files                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Types                 | `src/main/ci/types.ts`                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Errors                | `src/main/ci/errors.ts` (typed `CiError` union + formatter)                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Config parsing        | `src/main/ci/config.ts` (+ tests)                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| Parameter specs       | `src/main/ci/parameters.ts` (+ tests) — "Run custom build" typed-spec parser                                                                                                                                                                                                                                                                                                                                                                                                            |
| Activity & branches   | `src/main/ci/activity.ts` (+ tests)                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| TeamCity client       | `src/main/ci/teamcity.ts` (+ tests)                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Config/keychain glue  | `src/main/ci/CiManager.ts` (+ tests — allowlist, token gate, config validation)                                                                                                                                                                                                                                                                                                                                                                                                         |
| IPC                   | Shared channels `ci:config`, `ci:status`, `ci:trigger`, `ci:build`, `ci:buildParameters`, `ci:branches`, `ci:activity`, `ci:listBuildTypes`, `ci:saveConfig`, `ci:testNewConnection`, `ci:jobsStatus`, `ci:jobRefs`, `ci:jobParameters`, `ci:triggerJob`, `ci:runActivity`, `ci:run`, `ci:githubSetup`, `ci:testGitHubConnection`, `ci:setGitHubCredential` in `src/main/ci/ipc.ts` (`registerCiHandlers`, + workspace-authorization tests), registered from `src/main/ipc/handlers.ts` |
| Renderer store        | `src/renderer/src/lib/stores/ci.svelte.ts`                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Renderer helpers      | `src/renderer/src/lib/ci/status.ts` (+ tests), `src/renderer/src/lib/ci/runBuildForm.ts`, `src/renderer/src/lib/ci/format.ts` (+ tests), `src/renderer/src/lib/ci/types.ts`, `src/renderer/src/lib/a11y/focusTrap.ts` (shared dialog focus trap)                                                                                                                                                                                                                                        |
| Sidebar               | `src/renderer/src/components/sidebar/CiSection.svelte` (CI/CD section), `src/renderer/src/components/ci/CiLastJobCard.svelte` (Last-job card), `src/renderer/src/components/sidebar/ProjectTreeSection.svelte` (Run CI Job on Branch… context-menu entry)                                                                                                                                                                                                                               |
| Dialogs               | `src/renderer/src/components/ci/CiRunJobModal.svelte`, `src/renderer/src/components/ci/RunBuildDialog.svelte`, `src/renderer/src/components/ci/CiActivityModal.svelte` (rendered from `MainLayout`)                                                                                                                                                                                                                                                                                     |
| Per-repo configurator | `src/renderer/src/components/preferences/ProjectCiModal.svelte`, `src/renderer/src/components/ci/CiJobPicker.svelte` (job selection list)                                                                                                                                                                                                                                                                                                                                               |
| Settings              | `src/renderer/src/components/preferences/CiConnectionsPrefs.svelte` (CI connections), `src/renderer/src/components/preferences/_partials/CiServerForm.svelte` (add/edit server form)                                                                                                                                                                                                                                                                                                    |
