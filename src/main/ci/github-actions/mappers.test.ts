import { describe, expect, it } from 'vitest'
import { mapGitHubRun } from './mappers'

describe('mapGitHubRun', () => {
  it.each([
    ['requested', 'queued', 'unknown'],
    ['queued', 'queued', 'unknown'],
    ['pending', 'queued', 'unknown'],
    ['waiting', 'waiting', 'unknown'],
    ['in_progress', 'running', 'unknown'],
    ['completed', 'finished', 'success'],
  ] as const)('maps %s to %s/%s', (status, state, conclusion) => {
    expect(
      mapGitHubRun(
        {
          id: 123,
          run_number: 7,
          status,
          conclusion: conclusion === 'success' ? 'success' : null,
          display_title: 'Release next',
          html_url: 'https://github.com/o/r/actions/runs/123',
          head_branch: 'next',
        },
        '.github/workflows/release.yml',
        'Release',
      ),
    ).toMatchObject({
      provider: 'github-actions',
      runId: '123',
      number: '7',
      state,
      conclusion,
      statusText: 'Release next',
      jobId: '.github/workflows/release.yml',
      jobLabel: 'Release',
    })
  })

  it.each([
    ['failure', 'failure'],
    ['timed_out', 'failure'],
    ['startup_failure', 'failure'],
    ['action_required', 'failure'],
    ['cancelled', 'cancelled'],
    ['neutral', 'neutral'],
    ['skipped', 'neutral'],
    ['stale', 'neutral'],
    ['something_new', 'unknown'],
  ] as const)('maps conclusion %s to %s', (rawConclusion, conclusion) => {
    expect(
      mapGitHubRun(
        { id: 1, status: 'completed', conclusion: rawConclusion },
        'release.yml',
        'Release',
      ).conclusion,
    ).toBe(conclusion)
  })
})
