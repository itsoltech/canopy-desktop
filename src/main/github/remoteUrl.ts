import { ok, err } from 'neverthrow'
import type { Result } from 'neverthrow'
import type { GitHubError } from './errors'
import type { RepoIdentifier } from './types'

const SSH_SHORTHAND = /^git@([^:]+):([^/]+)\/([^/]+?)(?:\.git)?$/
const HTTPS_URL = /^https?:\/\/([^/]+)\/([^/]+)\/([^/]+?)(?:\.git)?$/
const SSH_URL = /^ssh:\/\/git@([^/]+)\/([^/]+)\/([^/]+?)(?:\.git)?$/

// Strip any embedded `userinfo@` credentials from a captured host (e.g. a
// remote configured as `https://x-access-token:ghp_TOKEN@github.com/o/r`) so a
// token can never leak into the derived API URL or the stored host string.
function stripCredentials(host: string): string {
  const at = host.lastIndexOf('@')
  return at === -1 ? host : host.slice(at + 1)
}

function apiUrlForHost(host: string): string {
  if (host === 'github.com') return 'https://api.github.com/graphql'
  return `https://${host}/api/graphql`
}

export function parseGitHubRemote(url: string): Result<RepoIdentifier, GitHubError> {
  for (const pattern of [SSH_SHORTHAND, HTTPS_URL, SSH_URL]) {
    const m = url.match(pattern)
    if (m) {
      const host = stripCredentials(m[1])
      return ok({
        owner: m[2],
        repo: m[3],
        host,
        apiUrl: apiUrlForHost(host),
      })
    }
  }
  return err({ _tag: 'InvalidRemoteUrl', url })
}
