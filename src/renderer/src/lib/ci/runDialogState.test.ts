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
import type {
  CiParameter,
  CiRepoConfigInfo,
  GitHubActionsCiRepoConfigInfo,
  TeamCityCiRepoConfigInfo,
} from './types'

const GITHUB_CONFIG: GitHubActionsCiRepoConfigInfo = {
  provider: 'github-actions',
  baseUrl: 'https://github.com',
  repository: 'itsoltech/canopy-desktop',
  workflows: [
    { path: '.github/workflows/a.yml', label: 'A' },
    { path: '.github/workflows/b.yml', label: 'B' },
  ],
}

const TEAMCITY_CONFIG: TeamCityCiRepoConfigInfo = {
  provider: 'teamcity',
  baseUrl: 'https://tc.example.test',
  buildTypes: [{ id: 'Build_Deploy', label: 'Build & Deploy WIP' }],
}

const DEPLOY_PARAMETER: CiParameter = {
  name: 'Deploy',
  kind: 'checkbox',
  label: 'Deploy',
  description: undefined,
  required: false,
  defaultValue: '',
  options: undefined,
  multiple: false,
  valueSeparator: ',',
  checkedValue: '-Deploy',
  uncheckedValue: undefined,
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
  config: CiRepoConfigInfo = GITHUB_CONFIG,
  initialBranch = 'main',
): Promise<void> {
  vi.stubGlobal('window', { api })
  let state!: ReturnType<typeof createCiRunDialogState>
  const dispose = effect_root(() => {
    state = createCiRunDialogState('repo', initialBranch, config)
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

describe('TeamCity ambiguous trigger outcome', () => {
  it('keeps the dialog open and disables a duplicate submission', async () => {
    const ciTrigger = vi.fn().mockResolvedValue({
      ok: false,
      error: {
        code: 'CiDispatchAmbiguous',
        message: 'TeamCity may have accepted the build. Check TeamCity before starting it again.',
      },
    })

    await withDialogState(
      {
        ciBranches: vi.fn().mockResolvedValue(['main']),
        ciBuildParameters: vi.fn().mockResolvedValue([]),
        ciTrigger,
      },
      async (state) => {
        state.initialize()
        await vi.waitFor(() => expect(state.loading).toBe(false))
        state.primaryAction()
        expect(state.stage).toBe('confirm')
        state.primaryAction()

        await vi.waitFor(() => expect(state.running).toBe(false))
        expect(state.dispatchAmbiguous).toBe(true)
        expect(state.canContinue).toBe(false)
        expect(state.triggerError).toContain('Check TeamCity')

        state.primaryAction()
        expect(ciTrigger).toHaveBeenCalledOnce()

        // An unknown provider outcome is terminal for this dialog instance. Navigating back or
        // changing the target must not create a path to repeat a possibly accepted deployment.
        state.cancelOrBack()
        state.selectedRefName = 'release'
        await state.selectJob('Build_Deploy')
        expect(state.dispatchAmbiguous).toBe(true)
        expect(state.canContinue).toBe(false)
        expect(state.visibleError).toContain('Check TeamCity')
        state.primaryAction()
        expect(ciTrigger).toHaveBeenCalledOnce()
      },
      TEAMCITY_CONFIG,
    )
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
  it('loads and triggers a TeamCity build through the shared controller', async () => {
    const ciBranches = vi.fn().mockResolvedValue(['develop', 'main'])
    const ciBuildParameters = vi.fn().mockResolvedValue([DEPLOY_PARAMETER])
    const ciTrigger = vi.fn().mockRejectedValue(new Error('stop after capturing the request'))

    await withDialogState(
      { ciBranches, ciBuildParameters, ciTrigger },
      async (state) => {
        state.initialize()
        await vi.waitFor(() => expect(state.canContinue).toBe(true))

        expect(ciBranches).toHaveBeenCalledWith('repo', 'Build_Deploy')
        expect(ciBuildParameters).toHaveBeenCalledWith('repo', 'Build_Deploy')
        expect(state.selectedRefName).toBe('develop')
        expect(state.branchNames).toEqual(['develop', 'main'])

        state.primaryAction()
        expect(state.stage).toBe('configure')
        state.primaryAction()
        expect(state.stage).toBe('confirm')
        state.primaryAction()

        await vi.waitFor(() => expect(ciTrigger).toHaveBeenCalledTimes(1))
        expect(ciTrigger).toHaveBeenCalledWith('repo', 'Build_Deploy', 'develop', [
          { name: 'Deploy', value: '' },
        ])
        await vi.waitFor(() => expect(state.running).toBe(false))
      },
      TEAMCITY_CONFIG,
      'develop',
    )
  })

  it('strips Electron IPC wrappers from ref-loading failures', async () => {
    await withDialogState(
      {
        ciJobRefs: vi
          .fn()
          .mockRejectedValue(
            new Error(
              "Error invoking remote method 'ci:jobRefs': Error: GitHub token was rejected",
            ),
          ),
        ciJobParameters: vi.fn(),
      },
      async (state) => {
        state.initialize()
        await vi.waitFor(() => expect(state.visibleError).toBe('GitHub token was rejected'))
      },
    )
  })

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

  it('resolves a typed GitHub ref outside the bounded picker pages', async () => {
    const ciExactJobRef = vi.fn().mockResolvedValue({
      name: 'feature/beyond-page-five',
      kind: 'branch',
      commitSha: 'exact-sha',
    })
    const ciJobParameters = vi.fn().mockResolvedValue({ parameters: [], schemaRevision: 'sha' })

    await withDialogState(
      {
        ciJobRefs: vi.fn().mockResolvedValue([{ name: 'main', kind: 'branch' }]),
        ciExactJobRef,
        ciJobParameters,
      },
      async (state) => {
        state.initialize()
        await vi.waitFor(() => expect(state.refsLoading).toBe(false))
        expect(state.refQuery).toBe('feature/beyond-page-five')

        const resolved = await state.resolveExactRef('feature/beyond-page-five')

        expect(resolved).toBe(true)
        expect(ciExactJobRef).toHaveBeenCalledWith(
          'repo',
          '.github/workflows/a.yml',
          'feature/beyond-page-five',
        )
        expect(state.selectedRefName).toBe('feature/beyond-page-five')
        expect(state.selectedRef).toMatchObject({
          name: 'feature/beyond-page-five',
          kind: 'branch',
          commitSha: 'exact-sha',
        })
        await vi.waitFor(() => expect(ciJobParameters).toHaveBeenCalled())
        expect(ciJobParameters).toHaveBeenCalledWith(
          'repo',
          '.github/workflows/a.yml',
          expect.objectContaining({ name: 'feature/beyond-page-five', kind: 'branch' }),
        )
      },
      GITHUB_CONFIG,
      'feature/beyond-page-five',
    )
  })

  it('ignores an exact-ref response after the user edits the query', async () => {
    const exact = deferred<{ name: string; kind: 'branch'; commitSha: string }>()
    const ciExactJobRef = vi
      .fn()
      .mockReturnValueOnce(exact.promise)
      .mockResolvedValueOnce({ name: 'feature/b', kind: 'branch', commitSha: 'current-sha' })
    const ciJobParameters = vi.fn().mockResolvedValue({ parameters: [], schemaRevision: 'sha' })

    await withDialogState(
      {
        ciJobRefs: vi.fn().mockResolvedValue([{ name: 'main', kind: 'branch' }]),
        ciExactJobRef,
        ciJobParameters,
      },
      async (state) => {
        state.initialize()
        await vi.waitFor(() => expect(state.refsLoading).toBe(false))

        const resolving = state.resolveExactRef('feature/a')
        expect(state.exactRefLoading).toBe(true)
        state.refQuery = 'feature/b'

        expect(state.exactRefLoading).toBe(false)
        expect(state.loading).toBe(false)
        await expect(state.resolveExactRef('feature/b')).resolves.toBe(true)
        expect(state.selectedRefName).toBe('feature/b')
        exact.resolve({ name: 'feature/a', kind: 'branch', commitSha: 'stale-sha' })

        await expect(resolving).resolves.toBe(false)
        expect(state.refQuery).toBe('feature/b')
        expect(state.selectedRefName).not.toBe('feature/a')
        expect(ciJobParameters).not.toHaveBeenCalledWith(
          'repo',
          '.github/workflows/a.yml',
          expect.objectContaining({ name: 'feature/a' }),
        )
      },
      GITHUB_CONFIG,
      'feature/a',
    )
  })

  it('cancels an exact-ref lookup immediately when a visible ref is selected', async () => {
    const exact = deferred<{ name: string; kind: 'branch'; commitSha: string }>()

    await withDialogState(
      {
        ciJobRefs: vi.fn().mockResolvedValue([{ name: 'main', kind: 'branch' }]),
        ciExactJobRef: vi.fn().mockReturnValue(exact.promise),
        ciJobParameters: vi.fn().mockResolvedValue({ parameters: [], schemaRevision: 'sha' }),
      },
      async (state) => {
        state.initialize()
        await vi.waitFor(() => expect(state.refsLoading).toBe(false))

        const resolving = state.resolveExactRef('feature/a')
        expect(state.exactRefLoading).toBe(true)
        state.selectedRefName = 'main'

        expect(state.exactRefLoading).toBe(false)
        expect(state.loading).toBe(false)
        exact.resolve({ name: 'feature/a', kind: 'branch', commitSha: 'stale-sha' })
        await expect(resolving).resolves.toBe(false)
        expect(state.selectedRefName).toBe('main')
      },
      GITHUB_CONFIG,
      'feature/a',
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
