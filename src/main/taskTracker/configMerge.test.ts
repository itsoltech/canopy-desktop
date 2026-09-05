import { describe, expect, it } from 'vitest'
import { mergeConfigs } from './configMerge'
import type { RepoConfig } from './types'

function config(trackers: RepoConfig['trackers']): RepoConfig {
  return {
    version: 1,
    trackers,
    projectOverrides: {},
    filters: { assignedToMe: true, statuses: [] },
  }
}

describe('mergeConfigs tracker precedence', () => {
  it('keeps repository trackers before unrelated personal trackers', () => {
    const resolved = mergeConfigs(
      config([{ id: 'github-personal', provider: 'github', baseUrl: 'https://github.com' }]),
      config([{ id: 'jira-default', provider: 'jira', baseUrl: 'https://itsol.atlassian.net' }]),
    )

    expect(resolved?.config.trackers.map((tracker) => tracker.id)).toEqual([
      'jira-default',
      'github-personal',
    ])
    expect(resolved?.repoTrackerIds).toEqual(['jira-default'])
  })
})
