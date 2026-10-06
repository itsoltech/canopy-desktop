import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { TaskTrackerConnection } from '../types'
import { githubClient } from './github'

function connection(baseUrl: string): TaskTrackerConnection {
  return {
    id: 'gh',
    provider: 'github',
    name: 'GitHub',
    baseUrl,
    projectKey: 'acme/app',
    authPrefKey: 'tracker.gh.token',
  }
}

describe('githubClient.fetchTaskComments', () => {
  const fetchMock = vi.fn()

  beforeEach(() => {
    fetchMock.mockResolvedValue(
      new Response(
        JSON.stringify({ data: { repository: { issue: { comments: { nodes: [] } } } } }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      ),
    )
    vi.stubGlobal('fetch', fetchMock)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    fetchMock.mockReset()
  })

  it('requests the most recent comments of a long thread', async () => {
    await githubClient.fetchTaskComments(connection('https://github.com'), 'token', '#7')

    const body = JSON.parse(fetchMock.mock.calls[0][1].body as string) as { query: string }
    expect(body.query).toMatch(/comments\(last: 50\b/)
  })

  it('keeps the port of a GitHub Enterprise base URL', async () => {
    await githubClient.fetchTaskComments(connection('https://ghe.example.com:8443'), 'token', '#7')

    expect(fetchMock.mock.calls[0][0]).toBe('https://ghe.example.com:8443/api/graphql')
  })
})
