import { okAsync } from 'neverthrow'
import { describe, expect, it, vi } from 'vitest'
import type { TeamCityCiConfig } from '../types'
import { TeamCityAdapter } from './teamcity'

const CONFIG: TeamCityCiConfig = {
  provider: 'teamcity',
  baseUrl: 'https://tc.example.com',
  buildTypes: [{ id: 'Gakko_Build', label: 'Build' }],
}

describe('TeamCityAdapter', () => {
  it('normalizes the existing TeamCity status without changing client semantics', async () => {
    const client = {
      fetchBuildForBranch: vi.fn(() =>
        okAsync({
          id: 7,
          number: '7',
          state: 'running' as const,
          status: 'UNKNOWN' as const,
          percentageComplete: 50,
          webUrl: 'build-url',
          branchName: 'next',
        }),
      ),
    }
    const adapter = new TeamCityAdapter(CONFIG, 'token', client)

    const result = await adapter.status({ name: 'next', kind: 'branch' })

    expect(result.isOk() && result.value[0]).toMatchObject({
      jobId: 'Gakko_Build',
      label: 'Build',
      provider: 'teamcity',
      run: {
        provider: 'teamcity',
        runId: '7',
        state: 'running',
        jobId: 'Gakko_Build',
      },
    })
    expect(client.fetchBuildForBranch).toHaveBeenCalledWith(
      'https://tc.example.com',
      'token',
      'Gakko_Build',
      'next',
    )
  })
})
