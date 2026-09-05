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
})
