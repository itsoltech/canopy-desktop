import { ok, err } from 'neverthrow'
import type { Result } from 'neverthrow'
import type { GitHubError } from './errors'
import type { RepoIdentifier } from './types'

const SSH_SHORTHAND = /^git@([^:]+):([^/]+)\/([^/.]+?)(?:\.git)?$/
const HTTPS_URL = /^https?:\/\/([^/]+)\/([^/]+)\/([^/.]+?)(?:\.git)?$/
const SSH_URL = /^ssh:\/\/git@([^/]+)\/([^/]+)\/([^/.]+?)(?:\.git)?$/

function apiUrlForHost(host: string): string {
  if (host === 'github.com') return 'https://api.github.com/graphql'
  return `https://${host}/api/graphql`
}

export function parseGitHubRemote(url: string): Result<RepoIdentifier, GitHubError> {
  for (const pattern of [SSH_SHORTHAND, HTTPS_URL, SSH_URL]) {
    const m = url.match(pattern)
    if (m) {
      return ok({
        owner: m[2],
        repo: m[3],
        host: m[1],
        apiUrl: apiUrlForHost(m[1]),
      })
    }
  }
  return err({ _tag: 'InvalidRemoteUrl', url })
}
