# GitHub Actions CI/CD

> Configure, dispatch, and monitor selected `workflow_dispatch` workflows from a GitHub.com repository.

**Status:** Experimental
**Introduced:** v0.13.0
**Platforms:** All

## Overview

GitHub Actions is a second provider for Canopy's opt-in CI/CD sidebar section. The repository
owns the workflow allowlist in `.canopy/config.json`; a personal access token stays in the OS
keychain. Canopy derives the repository from the workspace's `origin` remote and requires an
exact match with the configured `owner/repository` before it reads credentials or contacts the
API.

Only GitHub.com is supported in this release. GitHub Enterprise Server and custom API origins
are intentionally out of scope.

## Setup

1. Create a fine-grained personal access token restricted to the repository. Grant
   **Actions: write** and **Contents: read** repository permissions.
2. Enable the CI/CD sidebar section in Settings if it is hidden.
3. Open the repository CI/CD configurator, choose **GitHub Actions**, and add or reuse the
   GitHub.com credential.
4. Test the connection. This performs read-only repository and workflow discovery.
5. Select only the workflows that should be available from Canopy, edit their labels if needed,
   and save. The resulting configuration can be committed for the team; the token is never
   written to the repository.

Classic personal access tokens are also accepted when their repository scope permits the same
operations, but fine-grained tokens are preferred.

## Running a workflow

- The generic **Run job…** action starts without a selected ref. The worktree action may prefill
  its branch only when that exact remote branch exists.
- Branches and tags are distinct choices. Canopy rejects an ambiguous name that exists as both.
- Input controls are generated from the workflow file at the selected ref. Supported types are
  `string`, `boolean`, `choice`, and `environment`.
- Workflow inputs are not treated as secrets. Do not paste tokens, passwords, or other secrets
  into them; use GitHub Actions secrets instead.
- Before dispatch, the main process reloads the ref and workflow schema, validates the inputs,
  and opens a native confirmation showing the exact repository, workflow, ref, current commit,
  and non-secret inputs. Cancelling or closing the dialog sends no request.
- A dispatch is sent once and is never retried automatically. The versioned GitHub API response
  must contain the exact run ID. If the outcome is network-ambiguous or no ID is returned,
  Canopy directs the user to GitHub Actions instead of guessing or dispatching again.

Git refs can move between opening the form and confirmation. The commit shown by the native
confirmation is the latest commit resolved immediately before dispatch; Canopy validates the
same ref and workflow schema again after confirmation.

## Status and history

The sidebar and activity window query only workflows selected in the repository configuration.
History is merged across those workflows, preserving GitHub's display title and states such as
queued, in progress, waiting, cancelled, neutral, failed, and successful. A run opens directly
on GitHub.

Queries and pagination are bounded. If one configured workflow cannot be loaded or more history
exists beyond the bounded page, the activity view marks the result as partial rather than
presenting it as complete. Environment approvals and run cancellation remain GitHub operations;
Canopy only shows the waiting state and link.

Run watching is in-memory. Restarting Canopy does not restore an active watcher, but the scoped
history remains the recovery surface.

## Configuration

```json
{
  "ci": {
    "provider": "github-actions",
    "baseUrl": "https://github.com",
    "repository": "itsoltech/canopy-desktop",
    "workflows": [
      {
        "path": ".github/workflows/release.yml",
        "label": "Release"
      }
    ]
  }
}
```

- `baseUrl` must be exactly `https://github.com`.
- `repository` is the `owner/repository` pair and must exactly match the workspace `origin` remote, ignoring only GitHub's
  case-insensitive spelling.
- Workflow paths must be direct `.yml` or `.yaml` files under `.github/workflows/`.
- At most 50 workflows may be configured; duplicate workflow paths collapse to the first
  entry when reading a hand-edited file and are rejected by the save path.

Older Canopy versions do not understand this provider and show the CI block as unsupported.
Upgrade the team before committing a GitHub Actions configuration. Removing the `ci` block rolls
back the repository integration without deleting personal credentials or changing remote
workflows.

## Error states

| State                | Behavior                                                                 |
| -------------------- | ------------------------------------------------------------------------ |
| Missing token        | Open the repository CI/CD configurator; no API call is made.             |
| Repository mismatch  | Reject before token access or network use.                               |
| Workflow unavailable | Keep the per-workflow discovery error visible and do not allow dispatch. |
| Ref/schema changed   | Reload the form and require a fresh confirmation.                        |
| Rate limited         | Pause background work until the reported reset time.                     |
| Dispatch ambiguous   | Do not retry; link to repository Actions history.                        |
| Unknown GitHub state | Display **Unknown** rather than inferring success or failure.            |

## Source files

- Main provider and parser: `src/main/ci/github-actions/`
- Provider adapters and orchestration: `src/main/ci/providers/`, `src/main/ci/CiManager.ts`
- IPC and preload boundary: `src/main/ci/ipc.ts`, `src/preload/index.ts`
- Renderer flows: `src/renderer/src/components/ci/`,
  `src/renderer/src/components/preferences/GitHubActionsCiConfigurator.svelte`
