# GitHub integration

> Fetch PR status for branches, detect repository identity from git remotes, and create pull requests via the GitHub GraphQL API.

**Status:** Stable
**Introduced:** v0.9.0
**Platforms:** All

## Overview

The GitHub integration provides PR awareness across the Canopy UI. It fetches open pull requests for all branches in a repository and maps them by head branch name, so the branch list and worktree sidebar can display PR state (open, draft, review decision, CI status). It also resolves repository identity (owner, repo, default branch) from git remote URLs and supports creating PRs directly via the GraphQL API.

This integration is separate from the task tracker's GitHub Issues provider. The task tracker handles issue browsing and branch/PR creation via templates and the `gh` CLI. This `GitHubService` handles PR status display and GraphQL-based PR creation for the general Git workflow.

Authentication uses a GitHub token obtained from a matching task tracker connection (provider `github` with a matching host or `projectKey`). The service looks up connections via `TaskTrackerManager.getConnections()` and reads the token from the preferences store.

## Behavior

### Detecting repository identity

1. Canopy calls `getRepoIdentifier(repoRoot)`.
2. `GitRepository.getRemoteUrl(repoRoot)` reads the `origin` remote URL.
3. `parseGitHubRemote(url)` matches against three patterns: SSH shorthand (`git@host:owner/repo.git`), HTTPS (`https://host/owner/repo.git`), and SSH URL (`ssh://git@host/owner/repo.git`).
4. On match, it returns `{ owner, repo, host, apiUrl }`. The API URL is `https://api.github.com/graphql` for `github.com` and `https://{host}/api/graphql` for GitHub Enterprise instances.
5. If no pattern matches, an `InvalidRemoteUrl` error is returned. If the remote cannot be read, a `NoGitHubRemote` error is returned.

### Fetching PR status for branches

1. The renderer calls `loadBranchPRs(repoRoot)` from the GitHub store.
2. Requests are debounced at 30 seconds per repository. A forced refresh bypasses the debounce.
3. `GitHubService.findGitHubConnection(repoRoot)` locates a task tracker connection with provider `github` whose `projectKey` matches `owner/repo`, or whose host matches and `projectKey` is empty.
4. If no matching connection or token is found, the fetch silently returns (no error toast).
5. `fetchOpenPRsForBranches()` runs a GraphQL search query: `repo:{owner}/{repo} is:pr is:open head:{branch1} head:{branch2} ...`, fetching up to 50 PRs.
6. Each PR result includes: `number`, `title`, `state`, `url`, `headRefName`, `baseRefName`, `isDraft`, `reviewDecision` (APPROVED, CHANGES_REQUESTED, REVIEW_REQUIRED, or null), and `checksState` (from `statusCheckRollup.state`: SUCCESS, FAILURE, PENDING, etc.).
7. Results are stored in a reactive `branchPRs` map keyed by head branch name. The map is merged across repositories (multiple repos in the same workspace).
8. Rate limit (403) and auth errors (401) show a toast notification. Other errors are silently ignored.

### Getting repository info

1. `getRepoInfo(apiUrl, token, owner, repo)` fetches the repository's node ID and default branch name via GraphQL.
2. Returns `{ id, defaultBranch }`. The `id` is the GraphQL node ID needed for mutations (e.g., PR creation). The `defaultBranch` falls back to `"main"` if `defaultBranchRef` is null.

### Creating a pull request

1. The renderer calls `githubCreatePR(repoRoot, params)`.
2. `GitHubService.createPR()` executes the `createPullRequest` GraphQL mutation with `repositoryId`, `headRefName`, `baseRefName`, `title`, `body`, and `draft` flag.
3. On success, the new PR is returned with its URL and metadata.
4. The returned PR has `reviewDecision: null` and `checksState: null` (newly created PRs have no reviews or checks yet).

### Store lifecycle

- `loadBranchPRs(repoRoot)` is called when a workspace is activated or a branch changes.
- `loadRepoInfo(repoRoot)` fetches repo metadata (default branch, repo ID).
- `resetGitHubState()` clears all cached PR data and repo info (called on workspace switch).

## Configuration

No user-facing configuration. The integration uses the GitHub token from the first matching task tracker connection. The token must have `repo` scope for private repositories.

The debounce interval is hardcoded at 30 seconds (`DEBOUNCE_MS`).

## Error states

| Error                | User sees                                      | Cause                                                                                      |
| -------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `GitHubTokenMissing` | "GitHub token is not configured"               | No task tracker connection with provider `github` found, or connection has no stored token |
| `GitHubApiError`     | "GitHub API error ({status}): {message}"       | Non-200 HTTP response from the GraphQL endpoint (auth failure, server error)               |
| `GitHubGraphQLError` | "GitHub GraphQL error: {messages}"             | GraphQL response contains errors (invalid query, permission denied on a field)             |
| `GitHubRateLimited`  | "GitHub rate limit exceeded, resets at {time}" | HTTP 403 with `x-ratelimit-reset` header. Displays reset time in local format              |
| `GitHubNetworkError` | "GitHub network error: {message}"              | DNS failure, timeout (15 seconds), or connection refused                                   |
| `InvalidRemoteUrl`   | "Invalid GitHub remote URL: {url}"             | Remote URL does not match any supported GitHub URL pattern                                 |
| `NoGitHubRemote`     | "No GitHub remote found in {repoRoot}"         | Could not read the `origin` remote URL from the git repository                             |

## Source files

- Main: `src/main/github/`
  - `GitHubService.ts` - PR search, PR creation, repo info queries
  - `graphql.ts` - GraphQL fetch with rate limit and error handling
  - `remoteUrl.ts` - git remote URL parsing (SSH, HTTPS, SSH URL formats)
  - `errors.ts` - typed error union with message formatter
  - `types.ts` - `RepoIdentifier`, `GitHubPR`, `BranchPRMap`, `CreatePRInput`, `GitHubRepoInfo`
- Store: `src/renderer/src/lib/stores/github.svelte.ts`
