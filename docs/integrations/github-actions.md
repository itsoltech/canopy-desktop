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

1. Enable the CI/CD sidebar section in Settings if it is hidden.
2. Open the repository CI/CD configurator and choose **GitHub Actions**.
   **Load workflows** stays inactive while Canopy resolves `origin`. An absent remote, an
   unsupported GitHub Enterprise host, and a git lookup failure are reported as distinct states.
3. After Canopy confirms a supported `github.com` origin, select **Generate token on GitHub**.
   Canopy asks GitHub to preselect **Actions: write** and
   **Contents: read**; confirm both permissions and the token expiry on GitHub. Choose the
   repository owner, restrict **Repository access** to this repository, then paste the generated
   token into Canopy.
4. Load the workflows. Canopy validates and stores the token, then performs read-only repository
   and workflow discovery.
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

The open history window refreshes every 60 seconds. In the sidebar CI/CD section, the activity
summary polls every 60 seconds while any configured workflow has a run in flight and every 300
seconds otherwise. The **Last run** card uses the same cadence, but keys it to runs for the active
worktree's branch, so a run on another branch speeds up the activity summary but not the card. All
three surfaces re-fetch immediately after Canopy dispatches a workflow.

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
- The configurator resolves `repository` from the local `origin`. If that differs from the
  git-shared value (for example in a fork), it warns that saving will rewrite the shared value and
  cause `CiRepositoryMismatch` for clones that still use the previous origin.
- Workflow paths must be direct `.yml` or `.yaml` files under `.github/workflows/`.
- At most 50 workflows may be configured; duplicate workflow paths collapse to the first
  entry when reading a hand-edited file and are rejected by the save path.

Older Canopy versions do not understand this provider and show the CI block as unsupported.
Upgrade the team before committing a GitHub Actions configuration. Removing the `ci` block rolls
back the repository integration without deleting personal credentials or changing remote
workflows.

## Error states

| `CiError` variant         | Behavior                                                                   |
| ------------------------- | -------------------------------------------------------------------------- |
| `CiNotConfigured`         | Offer CI setup instead of making an API call.                              |
| `CiConfigInvalid`         | Keep the invalid file/block reason visible so it can be corrected.         |
| `CiConfigUnwritable`      | Preserve the configuration and report the local update failure.            |
| `CiAuthMissing`           | Open the repository CI/CD configurator; no authenticated API call is made. |
| `CiRepositoryMismatch`    | Reject before token access or network use.                                 |
| `CiWorkflowSchemaInvalid` | Block discovery or dispatch and show the schema reason.                    |
| `CiWorkflowSchemaChanged` | Reload the inputs and require the user to review them again.               |
| `CiRefChanged`            | Require a fresh confirmation for the ref's new commit.                     |
| `CiDispatchCancelled`     | Keep the run form open; nothing was dispatched.                            |
| `CiDispatchAmbiguous`     | Do not retry; instruct the user to check repository Actions history.       |
| `CiRateLimited`           | Pause background work until the reported reset time.                       |
| `CiApiError`              | Show the sanitized GitHub status and message.                              |

An unknown GitHub run state is displayed as **Unknown** rather than inferred as success or
failure.

## Security and privacy

- GitHub Actions uses a repository-audience binding to a credential declaring `actions.read`,
  `contents.read` and `actions.dispatch`. Service, repository and capability checks keep it separate
  from GitHub Issues tokens and Git source transport. See
  [Integration credentials](credentials.md).
- Tokens are stored through `KeychainTokenStore`, encrypted with Electron `safeStorage` when the
  operating system provides it, and never written to `.canopy/config.json` or workflow inputs. If
  no OS keyring is available, Canopy warns before storing the token unencrypted in its local
  database on this machine.
- Every repo-scoped IPC call is authorized against the sender's workspace before the main process
  resolves the origin remote, reads a token, or calls GitHub. The configured repository must match
  that remote.
- GitHub requests go only to `https://api.github.com`, reject redirects, have bounded response
  sizes and timeouts, and use the pinned `2026-03-10` API version. The dispatch endpoint returns
  the exact workflow-run ID; Canopy does not search heuristically or retry an ambiguous dispatch.
- The native confirmation is the final trust boundary for dispatch. It shows the repository,
  workflow, ref, resolved commit and all non-secret inputs. Workflow inputs are ordinary values;
  secrets belong in GitHub Actions secrets.

## Source files

- Main provider and parser: `src/main/ci/github-actions/`
- Provider adapters and orchestration: `src/main/ci/providers/`, `src/main/ci/CiManager.ts`
- IPC and preload boundary: `src/main/ci/ipc.ts`, `src/preload/index.ts` — `ci:jobsStatus`,
  `ci:jobRefs`, `ci:jobParameters`, `ci:triggerJob`, `ci:runActivity`, `ci:run`,
  `ci:githubSetup`, `ci:testGitHubConnection`, and `ci:setGitHubCredential`
- Renderer flows: `src/renderer/src/components/ci/`,
  `src/renderer/src/components/preferences/GitHubActionsCiConfigurator.svelte`
- Shared credential registry: `src/main/credentials/`,
  `src/main/taskTracker/KeychainTokenStore.ts`
