# TeamCity CI/CD

> Build status per branch, triggering any job on any branch, and a live view of the server's activity — from the CI/CD and GIT sections of the sidebar.

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
  keychain, managed globally in **Settings → CI connections** (its own section, separate from the Project management connections; the add form has a provider select — TeamCity today — and a "Generate →" link to the server's token page once the URL is typed).
- **Personal opt-in**: the whole feature is gated by the **CI/CD sidebar section**
  (`sidebar.sections`, hidden by default — enable it in Settings → Sidebar). A `ci`
  block arriving via the git-shared config never turns the feature on by itself.
  Repositories without a `ci` block are unaffected either way: no rows, no requests,
  no polling.

## Behavior

### CI/CD sidebar section

- **Not configured**: a "Configure TeamCity" entry opens the per-repo configurator.
- **Configured**: the server row (click opens TeamCity), a **Run job…** entry, and the
  server's current **activity**.
- **Token missing**: a banner links to Settings → CI connections.

### Per-repo configurator (modal)

Pick one of your CI servers (or add a new one: URL + token with a connection test via
`GET /app/rest/server` and an explicit destination confirm before the token is stored),
then **Load build configurations** and choose, from the server's full list grouped by
TeamCity project, which jobs are available in this repository — analogous to the
project/board selection in Project management. Sidebar labels are editable. Saving
writes the `ci` block to `.canopy/config.json`; the modal can also remove it.

### Run job… (any job, any branch)

Pick a configured job and a branch (the list comes from TeamCity itself —
`/app/rest/buildTypes/id:X/branches`, default branch first). If the job prompts for
parameters (TeamCity's "Run custom build"), Canopy shows an equivalent dynamic form:
text inputs with descriptions, checkboxes honoring custom checked/unchecked values,
single selects, and multi-selects with All/None joined by the spec's value separator.
Fields are prefilled with the configuration's current values; required parameters
(`validationMode='not_empty'`) block submission. Chosen values are sent as build
`properties`. Jobs without prompt parameters queue immediately.

### Activity

Everything running or queued on the server right now (server-wide, like TeamCity's own
queue page; capped at 20+20): job name, branch, progress percentage; click opens the
build. Polls every 30 s, tightening to 10 s while anything is active, only while the
section is mounted.

### Branch build rows (GIT section)

For every configured job, the GIT section shows the newest build of the active
worktree's branch with a status chip (Queued / Running N% / Success / Failed / Unknown /
No builds); click opens the build. Status polls every 45 s (10 s while a build is
active); unconfigured repos get a single discovery fetch and no interval. The ▶ button
queues a build **of the current branch** — it is disabled while a build is active and
when the branch has no upstream (a branch that exists only locally has nothing for
TeamCity to build; push first). Builds triggered from Canopy are observed to completion
and finish with a toast.

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
| `CiNotConfigured` | No (valid) `ci` block in the repo config  | Sections show their "configure" entries; GIT rows hidden      |
| `CiAuthMissing`   | No token stored for the configured server | Credential banners linking to Settings → CI connections     |
| `CiApiError`      | HTTP/network/API failure                  | Muted error line in the section (full message in the tooltip) |

Additional surfaces that are not `CiError` variants:

- A failed trigger shows a toast with whatever `ci:trigger` rejected with — a formatted
  `CiApiError`, or a plain `Error` from the handler's input validation.
- A branch with no builds is not an error: the GIT row shows a "No builds" chip and the
  trigger stays available (once the branch is pushed).

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
- Build type ids and branch names cross the IPC boundary through strict charsets
  (`BUILD_TYPE_ID_PATTERN` = `[A-Za-z0-9_]{1,255}`, branch = `[A-Za-z0-9._/-]{1,255}`)
  that cannot escape a parenthesized TeamCity locator value. Triggering and
  parameter/branch queries are limited to build types declared in the repo config, and
  custom build properties are validated (name charset, value size, count cap) before
  they reach the request body.

## Source files

| Area                  | Files                                                                                                                                                                                                 |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Types                 | `src/main/ci/types.ts`                                                                                                                                                                                |
| Errors                | `src/main/ci/errors.ts` (typed `CiError` union + formatter)                                                                                                                                           |
| Config parsing        | `src/main/ci/config.ts` (+ tests)                                                                                                                                                                     |
| Parameter specs       | `src/main/ci/parameters.ts` (+ tests) — "Run custom build" typed-spec parser                                                                                                                          |
| Activity & branches   | `src/main/ci/activity.ts` (+ tests)                                                                                                                                                                   |
| TeamCity client       | `src/main/ci/teamcity.ts` (+ tests)                                                                                                                                                                   |
| Config/keychain glue  | `src/main/ci/CiManager.ts`                                                                                                                                                                            |
| IPC                   | `ci:config`, `ci:status`, `ci:trigger`, `ci:build`, `ci:buildParameters`, `ci:branches`, `ci:activity`, `ci:listBuildTypes`, `ci:saveConfig`, `ci:testNewConnection` in `src/main/ipc/handlers.ts`    |
| Renderer store        | `src/renderer/src/lib/stores/ci.svelte.ts`                                                                                                                                                            |
| Renderer helpers      | `src/renderer/src/lib/ci/status.ts`, `src/renderer/src/lib/ci/runBuildForm.ts` (+ tests)                                                                                                              |
| Sidebar               | `src/renderer/src/components/sidebar/CiSection.svelte` (CI/CD section), `src/renderer/src/components/sidebar/GitSection.svelte` (branch rows), `src/renderer/src/components/ci/RunBuildDialog.svelte` |
| Per-repo configurator | `src/renderer/src/components/preferences/ProjectCiModal.svelte`                                                                                                                                       |
| Settings              | `src/renderer/src/components/preferences/CiConnectionsPrefs.svelte` (CI connections)                                                                                                                        |
