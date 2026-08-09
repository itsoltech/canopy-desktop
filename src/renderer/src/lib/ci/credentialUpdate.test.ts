import { describe, expect, it, vi } from 'vitest'
import { replaceCiCredential, type CiCredentialUpdateApi } from './credentialUpdate'
import type { CiRepoConfigInfo } from './types'

const github: CiRepoConfigInfo = {
  provider: 'github-actions',
  baseUrl: 'https://github.com',
  repository: 'itsoltech/canopy-desktop',
  workflows: [{ path: '.github/workflows/ci.yml', label: 'CI' }],
}

const teamcity: CiRepoConfigInfo = {
  provider: 'teamcity',
  baseUrl: 'https://tc.itsol.tech',
  buildTypes: [{ id: 'Build', label: 'Build' }],
}

interface FakeApi extends CiCredentialUpdateApi {
  ciSaveConfig: ReturnType<typeof vi.fn>
}

function fakeApi(): FakeApi {
  return {
    ciTestGitHubConnection: vi.fn().mockResolvedValue(undefined),
    ciSetGitHubCredential: vi.fn().mockResolvedValue(undefined),
    ciTestNewConnection: vi.fn().mockResolvedValue(undefined),
    keychainSetCredentials: vi.fn().mockResolvedValue(undefined),
    ciSaveConfig: vi.fn().mockResolvedValue(undefined),
  }
}

describe('replaceCiCredential', () => {
  it('replaces only the repository-bound GitHub token', async () => {
    const api = fakeApi()

    await replaceCiCredential(api, 'repo', github, 'new-token')

    expect(api.ciTestGitHubConnection).toHaveBeenCalledWith('repo', 'new-token')
    expect(api.ciSetGitHubCredential).toHaveBeenCalledWith('repo', 'new-token')
    expect(api.ciSaveConfig).not.toHaveBeenCalled()
  })

  it('replaces only the TeamCity server token', async () => {
    const api = fakeApi()

    await replaceCiCredential(api, 'repo', teamcity, 'new-token')

    expect(api.ciTestNewConnection).toHaveBeenCalledWith('https://tc.itsol.tech', 'new-token')
    expect(api.keychainSetCredentials).toHaveBeenCalledWith(
      'teamcity',
      'https://tc.itsol.tech',
      'new-token',
    )
    expect(api.ciSaveConfig).not.toHaveBeenCalled()
  })
})
