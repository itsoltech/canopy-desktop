// The controller owns Svelte effects but is tested without mounting a component, so the test
// needs the same effect root that Svelte creates for components.
// eslint-disable-next-line svelte/no-svelte-internal
import { effect_root } from 'svelte/internal/client'
import { flushSync } from 'svelte'
import { describe, expect, it, vi } from 'vitest'
import {
  ambiguousCiRefNames,
  ciRunNextStepHint,
  createCiRunDialogState,
  isGitHubDispatchDenied,
  nextCiRunStage,
  previousCiRunStage,
} from './runDialogState.svelte'
import type { GitHubActionsCiRepoConfigInfo } from './types'

const GITHUB_CONFIG: GitHubActionsCiRepoConfigInfo = {
  provider: 'github-actions',
  baseUrl: 'https://github.com',
  repository: 'itsoltech/canopy-desktop',
  workflows: [
    { path: '.github/workflows/a.yml', label: 'A' },
    { path: '.github/workflows/b.yml', label: 'B' },
  ],
}

function deferred<T>(): {
  promise: Promise<T>
  resolve: (value: T) => void
} {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => (resolve = done))
  return { promise, resolve }
}

function withDialogState(
  api: Record<string, ReturnType<typeof vi.fn>>,
  run: (state: ReturnType<typeof createCiRunDialogState>) => Promise<void>,
): Promise<void> {
  vi.stubGlobal('window', { api })
  let state!: ReturnType<typeof createCiRunDialogState>
  const dispose = effect_root(() => {
    state = createCiRunDialogState('repo', 'main', GITHUB_CONFIG)
  })
  flushSync()
  return run(state).finally(() => {
    dispose()
    vi.unstubAllGlobals()
  })
}

describe('CI run stage navigation', () => {
  it('describes configuration or confirmation as the next provider-specific step', () => {
    expect(ciRunNextStepHint('teamcity', true, true)).toBe('Next: configure parameters.')
    expect(ciRunNextStepHint('teamcity', true, false)).toBe('Next: review and confirm the build.')
    expect(ciRunNextStepHint('github-actions', true, true)).toBe('Next: configure inputs.')
    expect(ciRunNextStepHint('github-actions', true, false)).toBe(
      'Next: review and confirm the workflow.',
    )
    expect(ciRunNextStepHint('github-actions', false, false)).toBe(
      'After selection, Canopy will check whether configuration is required.',
    )
  })

  it('always requires confirmation, including jobs without parameters', () => {
    expect(nextCiRunStage('select', false)).toBe('confirm')
    expect(nextCiRunStage('select', true)).toBe('configure')
    expect(nextCiRunStage('configure', true)).toBe('confirm')
    expect(nextCiRunStage('confirm', false)).toBeNull()
  })

  it('backs up one step while leaving select as the close boundary', () => {
    expect(previousCiRunStage('confirm', false)).toBe('select')
    expect(previousCiRunStage('confirm', true)).toBe('configure')
    expect(previousCiRunStage('configure', true)).toBe('select')
    expect(previousCiRunStage('select', false)).toBeNull()
  })
})

describe('ambiguousCiRefNames', () => {
  it('reports names shared by a branch and tag without flagging same-kind duplicates', () => {
    expect(
      ambiguousCiRefNames([
        { name: 'develop', kind: 'branch' },
        { name: 'release', kind: 'branch' },
        { name: 'release', kind: 'tag' },
        { name: 'v1', kind: 'tag' },
        { name: 'v1', kind: 'tag' },
      ]),
    ).toEqual(['release'])
  })
})

describe('GitHub dispatch permission errors', () => {
  it('uses the structured HTTP status instead of matching rendered error text', () => {
    expect(isGitHubDispatchDenied('github-actions', 403)).toBe(true)
    expect(isGitHubDispatchDenied('github-actions', 502)).toBe(false)
    expect(isGitHubDispatchDenied('teamcity', 403)).toBe(false)
  })
})

describe('CI run dialog controller', () => {
  it('ignores a stale refs response after the selected workflow changes', async () => {
    const first = deferred<Array<{ name: string; kind: 'branch' }>>()
    const second = deferred<Array<{ name: string; kind: 'branch' }>>()
    const ciJobRefs = vi.fn((_repo: string, jobId: string) =>
      jobId.endsWith('/a.yml') ? first.promise : second.promise,
    )

    await withDialogState(
      {
        ciJobRefs,
        ciJobParameters: vi.fn().mockResolvedValue({ parameters: [], schemaRevision: 'sha' }),
      },
      async (state) => {
        state.initialize()
        const selectingSecond = state.selectJob('.github/workflows/b.yml')
        second.resolve([{ name: 'main', kind: 'branch' }])
        await selectingSecond
        first.resolve([{ name: 'stale', kind: 'branch' }])
        await Promise.resolve()

        expect(state.jobId).toBe('.github/workflows/b.yml')
        expect(state.branchNames).toEqual(['main'])
        expect(state.selectedRefName).toBe('main')
      },
    )
  })

  it('resets confirmation when the selected ref changes', async () => {
    await withDialogState(
      {
        ciJobRefs: vi.fn().mockResolvedValue([
          { name: 'main', kind: 'branch' },
          { name: 'next', kind: 'branch' },
        ]),
        ciJobParameters: vi.fn().mockResolvedValue({ parameters: [], schemaRevision: 'sha' }),
      },
      async (state) => {
        state.initialize()
        await vi.waitFor(() => expect(state.selectedRefName).toBe('main'))
        await state.loadParameters()
        expect(state.canContinue).toBe(true)
        state.primaryAction()
        expect(state.stage).toBe('confirm')

        state.selectedRefName = 'next'
        flushSync()
        expect(state.selectedRefName).toBe('next')
        expect(state.stage).toBe('select')
      },
    )
  })

  it('reloads the schema and requires review after a schema-change rejection', async () => {
    const ciJobParameters = vi
      .fn()
      .mockResolvedValueOnce({ parameters: [], schemaRevision: 'old-sha' })
      .mockResolvedValueOnce({ parameters: [], schemaRevision: 'new-sha' })

    await withDialogState(
      {
        ciJobRefs: vi.fn().mockResolvedValue([{ name: 'main', kind: 'branch' }]),
        ciJobParameters,
        ciTriggerJob: vi.fn().mockResolvedValue({
          ok: false,
          error: {
            code: 'CiWorkflowSchemaChanged',
            message: 'Workflow inputs changed; review them again',
          },
        }),
      },
      async (state) => {
        state.initialize()
        await vi.waitFor(() => expect(state.selectedRefName).toBe('main'))
        await state.loadParameters()
        expect(state.canContinue).toBe(true)
        state.primaryAction()
        state.primaryAction()

        await vi.waitFor(() => expect(ciJobParameters).toHaveBeenCalledTimes(2))
        await vi.waitFor(() => expect(state.running).toBe(false))
        expect(state.stage).toBe('select')
        expect(state.triggerError).toContain('review them again')
      },
    )
  })
})
