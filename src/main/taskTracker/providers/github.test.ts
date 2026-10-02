import { afterEach, describe, expect, it, vi } from 'vitest'
import type { TaskTrackerConnection } from '../types'
import { githubClient } from './github'

const connection = (baseUrl: string): TaskTrackerConnection => ({
  id: 'gh',
  provider: 'github',
  name: 'GitHub',
  baseUrl,
  projectKey: 'itsoltech/canopy-desktop',
  authPrefKey: 'taskTracker.token.gh',
})

const json = (data: unknown): Response =>
  new Response(JSON.stringify({ data }), { headers: { 'Content-Type': 'application/json' } })

const noIssues = { repository: { issues: { nodes: [] } } }

describe('githubClient.fetchTasks', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('filters "assigned to me" by the viewer login, not by any assignee', async () => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValueOnce(json({ viewer: { login: 'octocat' } }))
      .mockResolvedValueOnce(json(noIssues))
    vi.stubGlobal('fetch', fetchMock)

    const result = await githubClient.fetchTasks(connection('https://github.com'), 'token', {
      assignedToMe: true,
    })

    expect(result.isOk()).toBe(true)
    const issuesCall = fetchMock.mock.calls.at(-1)
    const body = JSON.parse(String(issuesCall?.[1]?.body))
    expect(body.variables.filterBy).toEqual({ assignee: 'octocat' })
  })

  it('keeps an Enterprise server port in the API endpoint', async () => {
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(json(noIssues))
    vi.stubGlobal('fetch', fetchMock)

    await githubClient.fetchTasks(connection('https://ghe.example.com:8443'), 'token', {})

    expect(fetchMock.mock.calls[0][0]).toBe('https://ghe.example.com:8443/api/graphql')
  })
})
