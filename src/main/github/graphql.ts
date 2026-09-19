import { errAsync, okAsync } from 'neverthrow'
import type { ResultAsync } from 'neverthrow'
import { fromExternalCall, errorMessage } from '../errors'
import type { GitHubError } from './errors'

interface GraphQLResponse<T> {
  data?: T
  errors?: Array<{ message: string }>
}

/**
 * `apiUrl` is derived from the repo's git remote host, so the responding server
 * is not necessarily github.com. Scrub the bearer token out of any body we turn
 * into an error message before it reaches the renderer — matches the redaction
 * the TeamCity and GitHub Actions clients already apply on every error branch.
 */
function redactToken(message: string, token: string): string {
  return token ? message.replaceAll(token, '[redacted]') : message
}

export function graphqlFetch<T>(
  apiUrl: string,
  token: string,
  query: string,
  variables?: Record<string, unknown>,
): ResultAsync<T, GitHubError> {
  return fromExternalCall(
    fetch(apiUrl, {
      method: 'POST',
      // Forbid redirects: apiUrl is derived from the repo's git remote host, so
      // a 3xx could otherwise forward the bearer token to an unintended origin.
      redirect: 'error',
      headers: {
        Authorization: `Bearer ${token}`,
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
      body: JSON.stringify({ query, variables }),
      signal: AbortSignal.timeout(15_000),
    }),
    (e): GitHubError => ({ _tag: 'GitHubNetworkError', message: errorMessage(e) }),
  ).andThen((res) => {
    if (res.status === 401 || res.status === 403) {
      const resetHeader = res.headers.get('x-ratelimit-reset')
      if (resetHeader && res.status === 403) {
        return errAsync<T, GitHubError>({
          _tag: 'GitHubRateLimited',
          resetAt: parseInt(resetHeader, 10),
        })
      }
      return fromExternalCall(
        res.text().catch(() => ''),
        (e): GitHubError => ({
          _tag: 'GitHubApiError',
          status: res.status,
          message: errorMessage(e),
        }),
      ).andThen((body) =>
        errAsync<T, GitHubError>({
          _tag: 'GitHubApiError',
          status: res.status,
          message: redactToken(body || res.statusText, token),
        }),
      )
    }

    if (!res.ok) {
      return fromExternalCall(
        res.text().catch(() => ''),
        (e): GitHubError => ({
          _tag: 'GitHubApiError',
          status: res.status,
          message: errorMessage(e),
        }),
      ).andThen((body) =>
        errAsync<T, GitHubError>({
          _tag: 'GitHubApiError',
          status: res.status,
          message: redactToken(body || res.statusText, token),
        }),
      )
    }

    return fromExternalCall(res.json() as Promise<GraphQLResponse<T>>, (e): GitHubError => ({
      _tag: 'GitHubApiError',
      status: 0,
      message: errorMessage(e),
    })).andThen((json) => {
      if (json.errors?.length) {
        return errAsync<T, GitHubError>({
          _tag: 'GitHubGraphQLError',
          errors: json.errors.map((e) => ({ message: redactToken(e.message, token) })),
        })
      }
      if (!json.data) {
        return errAsync<T, GitHubError>({
          _tag: 'GitHubApiError',
          status: 0,
          message: 'No data in GraphQL response',
        })
      }
      return okAsync(json.data as T)
    })
  })
}
