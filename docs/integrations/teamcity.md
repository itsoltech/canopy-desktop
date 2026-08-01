# TeamCity CI

> Show the TeamCity build status of the active worktree's branch in the sidebar GIT section and queue new builds without leaving Canopy.

**Status:** Experimental (behind the `ci.enabled` preference, default off)
**Introduced:** v0.13.0
**Platforms:** All

## Overview

Canopy can show the TeamCity build status of the active worktree's branch and queue new
builds, directly from the **GIT** section in the left sidebar. The integration is
double-gated: the repository must declare a `ci` block in `.canopy/config.json` (written
by the Settings → CI/CD configurator) AND the user must opt in via the **ci.enabled**
preference (same page, default off). The repo config is a git-shared file — without the personal opt-in, one
teammate committing a `ci` block would enable new UI and a background poller for
everyone opening the repo. Repositories without the block are unaffected either way:
no rows, no requests, no polling.

## Behavior

### Build status rows

For every configured build type, the GIT section shows a row (below the PR rows) with
the build type's label and a status chip for the newest build on the current branch:

- **Queued** — the build sits in the TeamCity queue.
- **Running N%** — in progress, with TeamCity's completion estimate when available.
- **Success** / **Failed** — the outcome of the last finished build.
- **Unknown** — a finished build without a recognizable status (e.g. cancelled).
- **No builds** — the branch has no builds of this configuration yet.

Clicking a row opens the build in TeamCity in the external browser. While the first
fetch for a configured repo is in flight, the section shows a "Checking builds…"
loader instead of popping rows in later.

Status polls every 45 s, tightening to 10 s while any build is queued or running.
Polling only runs while the feature is enabled and the repo is known to configure CI —
unconfigured repos get a single discovery fetch (per worktree/branch change or pref
toggle) and no interval. Switching worktrees re-keys the state — a slow response for
the previous worktree never overwrites the current one.

### Triggering a build

The ▶ button on a row queues a build of that configuration for the current branch
(`POST /app/rest/buildQueue`). It is disabled while a build of that configuration is
already queued or running.

If the configuration prompts for parameters (TeamCity's "Run custom build" dialog —
parameters with `display='prompt'`), Canopy first fetches their typed specs and shows
an equivalent **Run build** dialog: text inputs (with the parameter's description),
checkboxes (honoring custom checked/unchecked values), single selects, and
multi-selects with All/None shortcuts joined by the spec's value separator. Fields are
prefilled with the configuration's current values; required parameters
(`validationMode='not_empty'`) block submission while blank. The chosen values are
sent as build `properties`. Configurations without prompt parameters trigger
immediately, with the configuration's defaults.

After triggering, Canopy observes the build and shows a toast when it finishes
("Build … succeeded" / "failed"), even if the section has moved on to other rows.
Observation stops after the build finishes, after repeated API failures, or after 2 h.

### Configuring and connecting (Settings → CI/CD)

The dedicated **CI/CD** Settings page hosts everything:

- The `ci.enabled` opt-in toggle (personal, default off; flipped on automatically when
  you save a configuration yourself).
- The **configurator**: enter the server URL and an access token ("Generate →" opens
  the server's token page), **Test** the connection (`GET /app/rest/server`), then
  **Load build configurations** to pick, from the server's full list (grouped by
  TeamCity project), which build configurations are available in this repository —
  analogous to the per-repo project/board selection in Project management. Labels
  shown in the sidebar are editable. **Save configuration** writes the `ci` block to
  the git-tracked `.canopy/config.json`; commit it to share with the team.
- Credential management (change or remove the stored token) and **Remove CI
  configuration**.

The token is stored via the OS-encrypted credential store (`safeStorage`), keyed
`teamcity:<baseUrl>` and shared across projects that use the same server. Before the
token is used in any way (test, save-and-load, or the "Generate →" link), Canopy asks
the user to confirm the server address once per edit session, because when editing an
existing configuration that address comes from the git-shared repo config (see
Security and privacy). If the repo configures CI but no token is stored, the GIT
section shows a **Connect TeamCity** row that jumps to Settings.

## Configuration

Written by the Settings configurator (hand-editing works too) in `.canopy/config.json`:

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
  only (the renderer never supplies a URL).
- `buildTypes[].id` — TeamCity build configuration id (`[A-Za-z0-9_]`, ≤255 chars).
- `buildTypes[].label` — row label in the sidebar; defaults to the id.

The block is validated at read time (`parseCiConfig`, applied by `CiManager`); a
malformed block degrades to "not configured" (the section stays hidden). The raw value
is preserved verbatim across config saves — Settings actions that round-trip
`.canopy/config.json` never delete or rewrite a hand-edited `ci` block, even an
invalid one.

## Error states

The typed error union `CiError` has three variants:

| Variant           | Meaning                                   | Surface                                                       |
| ----------------- | ----------------------------------------- | ------------------------------------------------------------- |
| `CiNotConfigured` | No (valid) `ci` block in the repo config  | Section hidden entirely                                       |
| `CiAuthMissing`   | No token stored for the configured server | "Connect TeamCity" row linking to Settings                    |
| `CiApiError`      | HTTP/network/API failure                  | Muted error line in the section (full message in the tooltip) |

Additional surfaces that are not `CiError` variants:

- A failed trigger shows a toast with whatever `ci:trigger` rejected with — a formatted
  `CiApiError`, or a plain `Error` from the handler's input validation.
- A branch with no builds is not an error: the row shows a "No builds" chip, the row
  itself is disabled, and the trigger button stays available.

## Security and privacy

- The personal access token is stored via `KeychainTokenStore` keyed
  `teamcity:<baseUrl>`, encrypted at rest via Electron `safeStorage` (OS-native:
  DPAPI / Keychain / keyring; plaintext fallback in Canopy's local DB when no OS
  keyring is available). It is never written to `.canopy/config.json` or any other
  repository file.
- The server base URL always comes from the repo config — the renderer never supplies
  a URL, so a compromised page cannot redirect requests. Redirects are refused
  (`redirect: 'error'`) so the Bearer token cannot be forwarded to another host, and
  every request times out after 15 s.
- `baseUrl` lives in a **git-shared** file: anyone with write access to the repository
  controls where the token goes, and `http://` is accepted (plaintext transport). This
  is why every path that uses the token from the Settings form (connection test, save,
  and the repo-controlled "Generate →" link) requires the user to explicitly confirm
  the server address first — the confirm names the URL and warns when it is plain
  `http://`. The status/trigger paths are additionally protected by the keychain
  lookup being keyed on the exact `baseUrl`: no stored credential for an unknown URL
  means no token is sent.
- Build type ids and branch names cross the IPC boundary through strict charsets
  (`BUILD_TYPE_ID_PATTERN` = `[A-Za-z0-9_]{1,255}`, branch = `[A-Za-z0-9._/-]{1,255}`)
  that cannot escape a parenthesized TeamCity locator value; triggering is further
  limited to build types declared in the repo config.

## Source files

| Area                 | Files                                                                                                                                                                |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Types                | `src/main/ci/types.ts`                                                                                                                                               |
| Errors               | `src/main/ci/errors.ts` (typed `CiError` union + formatter)                                                                                                          |
| Config parsing       | `src/main/ci/config.ts` (+ tests)                                                                                                                                    |
| Parameter specs      | `src/main/ci/parameters.ts` (+ tests) — "Run custom build" typed-spec parser                                                                                         |
| TeamCity client      | `src/main/ci/teamcity.ts` (+ tests)                                                                                                                                  |
| Config/keychain glue | `src/main/ci/CiManager.ts`                                                                                                                                           |
| IPC                  | `ci:config`, `ci:status`, `ci:trigger`, `ci:build`, `ci:buildParameters`, `ci:listBuildTypes`, `ci:saveConfig`, `ci:testNewConnection` in `src/main/ipc/handlers.ts` |
| Renderer store       | `src/renderer/src/lib/stores/ci.svelte.ts`                                                                                                                           |
| Renderer helpers     | `src/renderer/src/lib/ci/status.ts`, `src/renderer/src/lib/ci/runBuildForm.ts` (+ tests)                                                                             |
| Sidebar UI           | `src/renderer/src/components/sidebar/GitSection.svelte`, `src/renderer/src/components/ci/RunBuildDialog.svelte`                                                      |
| Settings page        | `src/renderer/src/components/preferences/CiCdPrefs.svelte` (CI/CD section)                                                                                           |
