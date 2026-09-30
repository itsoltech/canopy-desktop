import { describe, expect, it } from 'vitest'
import { parseGitHubRemote } from './remoteUrl'

describe('parseGitHubRemote', () => {
  it.each([
    'git@github.com:owner/service.api.git',
    'https://github.com/owner/service.api.git',
    'ssh://git@github.com/owner/service.api.git',
  ])('accepts dotted repository names and strips only the terminal .git suffix', (remote) => {
    const result = parseGitHubRemote(remote)

    expect(result.isOk() && result.value).toMatchObject({
      host: 'github.com',
      owner: 'owner',
      repo: 'service.api',
    })
  })

  it('drops embedded credentials from the invalid-remote error', () => {
    const result = parseGitHubRemote('https://x-access-token:secret@example.com/group/sub/repo.git')

    expect(result.isErr() && result.error).toEqual({
      _tag: 'InvalidRemoteUrl',
      url: 'https://example.com/group/sub/repo.git',
    })
  })
})
