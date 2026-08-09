import type { CiRepoConfigInfo } from './types'

export interface CiCredentialUpdateApi {
  ciTestGitHubConnection(repoRoot: string, token: string): Promise<unknown>
  ciSetGitHubCredential(repoRoot: string, token: string): Promise<unknown>
  ciTestNewConnection(baseUrl: string, token: string): Promise<unknown>
  keychainSetCredentials(provider: string, baseUrl: string, token: string): Promise<unknown>
}

/** Replace one machine-local CI credential without reading or writing shared CI configuration. */
export async function replaceCiCredential(
  api: CiCredentialUpdateApi,
  repoRoot: string,
  config: CiRepoConfigInfo,
  token: string,
): Promise<void> {
  if (config.provider === 'github-actions') {
    await api.ciTestGitHubConnection(repoRoot, token)
    await api.ciSetGitHubCredential(repoRoot, token)
    return
  }

  await api.ciTestNewConnection(config.baseUrl, token)
  await api.keychainSetCredentials('teamcity', config.baseUrl, token)
}
