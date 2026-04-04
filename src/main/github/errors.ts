import { match } from 'ts-pattern'

export type GitHubError =
  | { _tag: 'GitHubTokenMissing' }
  | { _tag: 'GitHubApiError'; status: number; message: string }
  | { _tag: 'GitHubGraphQLError'; errors: Array<{ message: string }> }
  | { _tag: 'GitHubRateLimited'; resetAt: number }
  | { _tag: 'GitHubNetworkError'; message: string }
  | { _tag: 'InvalidRemoteUrl'; url: string }
  | { _tag: 'NoGitHubRemote'; repoRoot: string }

export function gitHubErrorMessage(error: GitHubError): string {
  return match(error)
    .with({ _tag: 'GitHubTokenMissing' }, () => 'GitHub token is not configured')
    .with({ _tag: 'GitHubApiError' }, (e) => `GitHub API error (${e.status}): ${e.message}`)
    .with(
      { _tag: 'GitHubGraphQLError' },
      (e) => `GitHub GraphQL error: ${e.errors.map((err) => err.message).join(', ')}`,
    )
    .with(
      { _tag: 'GitHubRateLimited' },
      (e) =>
        `GitHub rate limit exceeded, resets at ${new Date(e.resetAt * 1000).toLocaleTimeString()}`,
    )
    .with({ _tag: 'GitHubNetworkError' }, (e) => `GitHub network error: ${e.message}`)
    .with({ _tag: 'InvalidRemoteUrl' }, (e) => `Invalid GitHub remote URL: ${e.url}`)
    .with({ _tag: 'NoGitHubRemote' }, (e) => `No GitHub remote found in ${e.repoRoot}`)
    .exhaustive()
}
