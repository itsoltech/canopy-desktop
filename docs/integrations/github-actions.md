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

- The generic **Run job…** action and the worktree action prefill the active worktree branch only
  when that exact remote branch exists.
- Branches and tags are distinct choices. Canopy rejects an ambiguous name that exists as both.
- Input controls are generated from the workflow file at the selected ref. Supported types are
  `string`, `boolean`, `choice`, and `environment`.
- Workflow inputs are not treated as secrets. Do not paste tokens, passwords, or other secrets
  into them; use GitHub Actions secrets instead.
- Before dispatch, the shared run dialog shows the exact repository, workflow, ref, and
  non-secret inputs for confirmation. Cancelling or closing the dialog sends no request. The main
  process then reloads the ref and workflow schema and validates the inputs immediately before
  dispatch.
- A dispatch is sent once and is never retried automatically. The versioned GitHub API response
  must contain the exact run ID. If the outcome is network-ambiguous or no ID is returned,
  Canopy directs the user to GitHub Actions instead of guessing or dispatching again.

Git refs can move between opening the form and confirmation. Canopy resolves the selected ref
again immediately before dispatch and rejects the request if the ref changes while the dispatch
is being prepared.

## Status and history

The sidebar and activity window query only workflows selected in the repository configuration.
History is merged across those workflows, preserving GitHub's display title and states such as
queued, in progress, waiting, cancelled, neutral, failed, and successful. Clicking a run **in the
history window** opens it on GitHub; the sidebar card opens the window itself.

The history window has a **branch filter**, preselected to the branch it was opened from and
resettable to **All branches**. It is passed to the GitHub query as `?branch=`, not applied to the
response: `recent` is sliced to the ten newest across every configured workflow before Canopy sees
it, so filtering afterwards would show nothing for a branch whose last run is older than that.
Because a filtered response only ever contains one branch, switching to **All branches** is what
populates the rest of the dropdown.

Queries and pagination are bounded. The history view deliberately shows only the newest page and
does not treat older runs beyond that page as a fetch failure. If a configured workflow is missing
or its page cannot be loaded, the sidebar's CI element gains a warning-coloured `· Incomplete`
suffix naming the failed workflow, and the activity window keeps available runs under
a **Partial history** banner. Running and queued counts are not shown in the sidebar at all — they
live in the window — so the suffix is never suppressed in favour of one. Environment approvals and
run cancellation remain GitHub operations; Canopy only shows the waiting state and link.

The open history window refreshes every 60 seconds. In the sidebar CI/CD section, activity
polls every 60 seconds while any configured workflow has a run in flight. A new or changed
incomplete result gets up to three fast recovery polls before decaying to the 300-second idle
cadence, so permanent configuration drift does not consume the API budget indefinitely. The
card uses the same cadence, but keys
it to runs for the active worktree's branch, so a run on another branch speeds up the activity
poll but not the card. It shows **one** run — the single newest across every configured workflow,
not one per workflow — with its label and number, branch, GitHub display title and
status chip, under a heading that reads **Last job** — or **Running job** while _that run_ is
queued, running or waiting (GitHub's `unknown` state does not count: it means a state
Canopy could not map, not a run in flight). Selection is shared with TeamCity
(`newestLastStatusIndex`): newest timestamp wins, a workflow that has never run loses to one that
has, and with nothing run at all the first configured workflow is named.
Its first line uses the Queued, Started or Finished
timestamp; hovering replaces that timestamp with a history icon. Keyboard focus does not — it stays
on the card, which keeps its focus ring. A shown run whose workflow lookup
failed carries **Unavailable** with its reason; the other workflows are still reachable in the
history window. All three
surfaces re-fetch immediately after Canopy dispatches a workflow.

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

| `CiError` variant         | Behavior                                                                                                                       |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `CiNotConfigured`         | Offer CI setup instead of making an API call.                                                                                  |
| `CiConfigInvalid`         | Keep the invalid file/block reason visible so it can be corrected.                                                             |
| `CiConfigUnwritable`      | Preserve the configuration and report the local update failure.                                                                |
| `CiAuthMissing`           | Open the repository CI/CD configurator; no authenticated API call is made.                                                     |
| `CiCredentialUnavailable` | Explain that a stored credential is ambiguous, incompatible, or missing its secret and must be re-entered for this connection. |
| `CiRepositoryMismatch`    | Reject before token access or network use.                                                                                     |
| `CiWorkflowSchemaInvalid` | Block discovery or dispatch and show the schema reason.                                                                        |
| `CiWorkflowSchemaChanged` | Reload the inputs and require the user to review them again.                                                                   |
| `CiRefChanged`            | Require a fresh confirmation for the ref's new commit.                                                                         |
| `CiDispatchCancelled`     | Keep the run form open; nothing was dispatched.                                                                                |
| `CiDispatchAmbiguous`     | Do not retry; instruct the user to check repository Actions history.                                                           |
| `CiRateLimited`           | Pause background work until the reported reset time.                                                                           |
| `CiApiError`              | Show the sanitized GitHub status and message.                                                                                  |

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
- The renderer confirmation is a user-safety step, not the authorization boundary. The main
  process independently authorizes the workspace and repository, allowlists the workflow,
  re-resolves the ref and schema, and validates all inputs before dispatch. Hosts may additionally
  wire a native confirmation callback. Workflow inputs are ordinary values; secrets belong in
  GitHub Actions secrets.

## Source files

- Main provider and parser: `src/main/ci/github-actions/`
- Provider adapters and orchestration: `src/main/ci/providers/`, `src/main/ci/CiManager.ts`
- IPC and preload boundary: `src/main/ci/ipc.ts`, `src/preload/index.ts` — `ci:jobsStatus`,
  `ci:jobRefs`, `ci:jobParameters`, `ci:triggerJob`, `ci:runActivity`, `ci:run`,
  `ci:githubSetup`, `ci:testGitHubConnection`, and `ci:setGitHubCredential`
- Renderer flow and state: `src/renderer/src/components/ci/CiRunDialog.svelte`,
  `src/renderer/src/components/ci/CiRunParameterFields.svelte`,
  `src/renderer/src/lib/ci/runDialogState.svelte.ts`,
  `src/renderer/src/components/preferences/GitHubActionsCiConfigurator.svelte`
- Shared credential registry: `src/main/credentials/`,
  `src/main/taskTracker/KeychainTokenStore.ts`
