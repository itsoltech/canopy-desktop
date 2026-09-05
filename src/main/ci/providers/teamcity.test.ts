import { okAsync } from 'neverthrow'
import { describe, expect, it, vi } from 'vitest'
import type { TeamCityCiConfig } from '../types'
import { TeamCityAdapter } from './teamcity'

const CONFIG: TeamCityCiConfig = {
  provider: 'teamcity',
  baseUrl: 'https://tc.example.com',
  buildTypes: [{ id: 'Gakko_Build', label: 'Build' }],
}
const PUBLIC_CONNECTION = { allowPrivate: false }

describe('TeamCityAdapter', () => {
  it('rejects a run owned by a build type outside the repository configuration', async () => {
    const client = {
      fetchBuild: vi.fn(() =>
        okAsync({
          id: 123,
          number: '123',
          state: 'finished' as const,
          status: 'SUCCESS' as const,
          statusText: undefined,
          percentageComplete: undefined,
          webUrl: 'build-url',
          branchName: 'main',
          queuedAt: undefined,
          startedAt: undefined,
          finishedAt: undefined,
          buildTypeId: 'Other_Project',
        }),
      ),
    }
    const adapter = new TeamCityAdapter(CONFIG, 'token', PUBLIC_CONNECTION, client)

    const result = await adapter.run('123')

    expect(result.isErr()).toBe(true)
  })

  it('normalizes the existing TeamCity status without changing client semantics', async () => {
    const client = {
      fetchBuildForBranch: vi.fn(() =>
        okAsync({
          id: 7,
          number: '7',
          state: 'running' as const,
          status: 'UNKNOWN' as const,
          statusText: 'Step 3/7',
          percentageComplete: 50,
          webUrl: 'build-url',
          branchName: 'next',
          queuedAt: 1,
          startedAt: 2,
          finishedAt: undefined,
        }),
      ),
    }
    const adapter = new TeamCityAdapter(CONFIG, 'token', PUBLIC_CONNECTION, client)

    const result = await adapter.status({ name: 'next', kind: 'branch' })

    expect(result.isOk() && result.value[0]).toMatchObject({
      jobId: 'Gakko_Build',
      label: 'Build',
      provider: 'teamcity',
      run: {
        provider: 'teamcity',
        runId: '7',
        state: 'running',
        statusText: 'Step 3/7',
        jobId: 'Gakko_Build',
        queuedAt: 1,
        startedAt: 2,
      },
    })
    expect(client.fetchBuildForBranch).toHaveBeenCalledWith(
      'https://tc.example.com',
      'token',
      PUBLIC_CONNECTION,
      'Gakko_Build',
      'next',
    )
  })

  it('rejects locator-unsafe refs before querying TeamCity', async () => {
    const fetchBuildForBranch = vi.fn(() => okAsync(null))
    const adapter = new TeamCityAdapter(CONFIG, 'token', PUBLIC_CONNECTION, {
      fetchBuildForBranch,
    })

    const result = await adapter.status({ name: 'main),count:999', kind: 'branch' })

    expect(result.isErr()).toBe(true)
    expect(fetchBuildForBranch).not.toHaveBeenCalled()
  })
})
