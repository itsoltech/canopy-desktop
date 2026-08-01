# TeamCity CI

## Overview

Canopy can show the TeamCity build status of the active worktree's branch and queue new
builds, directly from the **GIT** section in the left sidebar. The integration is
repo-scoped: it activates only when the repository's `.canopy/config.json` declares a
`ci` block. Repositories without one are unaffected — no rows, no requests.

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
Switching worktrees re-keys the state — a slow response for the previous worktree
never overwrites the current one.

### Triggering a build

The ▶ button on a row queues a build of that configuration for the current branch
(`POST /app/rest/buildQueue`). It is disabled while a build of that configuration is
already queued or running. After triggering, Canopy observes the build and shows a
toast when it finishes ("Build … succeeded" / "failed"), even if the section has moved
on to other rows. Observation stops after the build finishes, after repeated API
failures, or after 2 h.

### Connecting

The personal access token is entered in **Settings → Your connections → CI** — the row
appears when the active repository configures a CI server. **Test** performs
`GET /app/rest/server` with the candidate token; **Save credentials** stores the token
via the OS-encrypted credential store (`safeStorage`), keyed `teamcity:<baseUrl>` and
shared across projects that use the same server. The token is never written to any
repository file. If the repo configures CI but no token is stored, the GIT section
shows a **Connect TeamCity** row that jumps to Settings.

## Configuration

Hand-edited in `.canopy/config.json` (no configuration UI in the MVP):

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

A malformed `ci` block degrades to "not configured" (the section stays hidden) rather
than failing the whole config load.

## Error states

| State                      | Surface                                                       |
| -------------------------- | ------------------------------------------------------------- |
| No `ci` block              | Section hidden entirely                                       |
| Token missing              | "Connect TeamCity" row linking to Settings                    |
| Token rejected / API error | Muted error line in the section (full message in the tooltip) |
| Trigger fails              | Toast with the API error message                              |
| Branch with no builds      | "No builds" chip; row disabled, trigger still available       |

## Source files

| Area                 | Files                                                                                    |
| -------------------- | ---------------------------------------------------------------------------------------- |
| Types & errors       | `src/main/ci/types.ts`                                                                   |
| Config parsing       | `src/main/ci/config.ts` (+ tests)                                                        |
| TeamCity client      | `src/main/ci/teamcity.ts` (+ tests)                                                      |
| Config/keychain glue | `src/main/ci/CiManager.ts`                                                               |
| IPC                  | `ci:status`, `ci:trigger`, `ci:build`, `ci:testConnection` in `src/main/ipc/handlers.ts` |
| Renderer store       | `src/renderer/src/lib/stores/ci.svelte.ts`                                               |
| Status chip helpers  | `src/renderer/src/lib/ci/status.ts` (+ tests)                                            |
| Sidebar UI           | `src/renderer/src/components/sidebar/GitSection.svelte`                                  |
| Settings row         | `src/renderer/src/components/preferences/ConnectionsPrefs.svelte`                        |
