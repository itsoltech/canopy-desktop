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
  keychain, managed globally in **Settings → CI connections** (its own section, separate from the Project management connections; TeamCity is the only provider today, and the add form carries a "Generate →" link to the server's token page once the URL is typed).
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
  clicking the card opens that build in TeamCity), a **Run job…** entry, and a summary
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
`/app/rest/buildTypes/id:X/branches`, default branch first; a typed name not on the
list can still be used). The same modal opens from the worktree context menu —
right-click a branch in PROJECTS → **Run CI Job on Branch…** (prefilled with that
worktree's branch); the GIT section deliberately carries no CI entries, it holds
CI-independent git actions. If the job
prompts for
parameters (TeamCity's "Run custom build"), Canopy shows an equivalent dynamic form:
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
values are sent as build `properties`. Jobs without prompt parameters queue
immediately.

### Activity

The sidebar carries only a one-row summary (running/queued counts, or "Idle");
clicking it opens a dedicated activity window with the details: everything running or
queued on the server (server-wide, like TeamCity's own queue page; capped at 20+20)
plus the 10 most recent finished builds with their outcome. Job name, branch and
progress per row (start time + duration for finished builds — same-day times as
HH:MM, older ones as YYYY-MM-DD HH:MM; elapsed time for running); click opens the
build in TeamCity. The summary chip shows a single running build's percentage and the
queued count, refreshes immediately after a trigger from Canopy, and polls every 30 s
(10 s while anything is active) while the section is mounted; the window is resizable
and refreshes every 10 s while open.

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

- `provider` — only `teamcity` is supported.
- `baseUrl` — http(s) origin of the TeamCity server. All requests go to this origin
  only (the renderer never supplies a URL for repo-scoped calls).
- `buildTypes[].id` — TeamCity build configuration id (`[A-Za-z0-9_]`, ≤255 chars).
- `buildTypes[].label` — sidebar label; defaults to the id.

The block is validated at read time (`parseCiConfig`, applied by `CiManager`); a
malformed block degrades to "not configured". The raw value is preserved verbatim
across config saves — Settings actions that round-trip `.canopy/config.json` never
delete or rewrite a hand-edited `ci` block, even an invalid one.

## Error states

The typed error union `CiError` has three variants:

| Variant           | Meaning                                   | Surface                                                       |
| ----------------- | ----------------------------------------- | ------------------------------------------------------------- |
| `CiNotConfigured` | No (valid) `ci` block in the repo config  | Section shows its "configure" entry                           |
| `CiAuthMissing`   | No token stored for the configured server | Credential banners linking to Settings → CI connections       |
| `CiApiError`      | HTTP/network/API failure                  | Muted error line in the section (full message in the tooltip) |

Additional surfaces that are not `CiError` variants:

- A failed trigger shows a toast with whatever `ci:trigger` rejected with — a formatted
  `CiApiError`, or a plain `Error` from the handler's input validation.
- A branch with no builds is not an error: the Last-job card shows a "No builds" chip
  and running a job stays available.

## Security and privacy

- Access tokens are stored via `KeychainTokenStore` keyed `teamcity:<baseUrl>`,
  encrypted at rest via Electron `safeStorage` (OS-native: DPAPI / Keychain / keyring;
  plaintext fallback in Canopy's local DB when no OS keyring is available). They are
  never written to `.canopy/config.json` or any other repository file.
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
  that cannot escape a parenthesized TeamCity locator value. Triggering and
  parameter/branch queries are limited to build types declared in the repo config, and
  custom build properties are validated (name charset, value size, count cap) before
  they reach the request body.

## Source files

| Area                  | Files                                                                                                                                                                                                                                                     |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Types                 | `src/main/ci/types.ts`                                                                                                                                                                                                                                    |
| Errors                | `src/main/ci/errors.ts` (typed `CiError` union + formatter)                                                                                                                                                                                               |
| Config parsing        | `src/main/ci/config.ts` (+ tests)                                                                                                                                                                                                                         |
| Parameter specs       | `src/main/ci/parameters.ts` (+ tests) — "Run custom build" typed-spec parser                                                                                                                                                                              |
| Activity & branches   | `src/main/ci/activity.ts` (+ tests)                                                                                                                                                                                                                       |
| TeamCity client       | `src/main/ci/teamcity.ts` (+ tests)                                                                                                                                                                                                                       |
| Config/keychain glue  | `src/main/ci/CiManager.ts` (+ tests — allowlist, token gate, config validation)                                                                                                                                                                           |
| IPC                   | `ci:config`, `ci:status`, `ci:trigger`, `ci:build`, `ci:buildParameters`, `ci:branches`, `ci:activity`, `ci:listBuildTypes`, `ci:saveConfig`, `ci:testNewConnection` in `src/main/ipc/handlers.ts`                                                        |
| Renderer store        | `src/renderer/src/lib/stores/ci.svelte.ts`                                                                                                                                                                                                                |
| Renderer helpers      | `src/renderer/src/lib/ci/status.ts`, `src/renderer/src/lib/ci/runBuildForm.ts`, `src/renderer/src/lib/ci/format.ts` (+ tests), `src/renderer/src/lib/ci/types.ts`, `src/renderer/src/lib/a11y/focusTrap.ts` (shared dialog focus trap)                    |
| Sidebar               | `src/renderer/src/components/sidebar/CiSection.svelte` (CI/CD section), `src/renderer/src/components/ci/CiLastJobCard.svelte` (Last-job card), `src/renderer/src/components/sidebar/ProjectTreeSection.svelte` (Run CI Job on Branch… context-menu entry) |
| Dialogs               | `src/renderer/src/components/ci/CiRunJobModal.svelte`, `src/renderer/src/components/ci/RunBuildDialog.svelte`, `src/renderer/src/components/ci/CiActivityModal.svelte` (rendered from `MainLayout`)                                                       |
| Per-repo configurator | `src/renderer/src/components/preferences/ProjectCiModal.svelte`, `src/renderer/src/components/ci/CiJobPicker.svelte` (job selection list)                                                                                                                 |
| Settings              | `src/renderer/src/components/preferences/CiConnectionsPrefs.svelte` (CI connections), `src/renderer/src/components/preferences/_partials/CiServerForm.svelte` (add/edit server form)                                                                      |
