import { describe, expect, it, vi } from 'vitest'
import { ok, err, type Result } from 'neverthrow'
import { liveTrackerBindingKeys } from './liveTrackerBindings'

interface Tracker {
  id: string
}

function deps(
  overrides: Partial<Parameters<typeof liveTrackerBindingKeys<Tracker>>[0]> = {},
): Parameters<typeof liveTrackerBindingKeys<Tracker>>[0] {
  return {
    globalTrackers: [],
    workspacePaths: [],
    windowPaths: [],
    workspaceCount: 0,
    listMax: 100,
    configExists: async () => true,
    loadTrackers: async (): Promise<Result<{ trackers: Tracker[] }, unknown>> =>
      ok({ trackers: [] }),
    bindingKeyFor: (tracker) => `tracker:${tracker.id}`,
    ...overrides,
  }
}

describe('liveTrackerBindingKeys', () => {
  it('unions global, persisted-workspace and open-window trackers', async () => {
    const keys = await liveTrackerBindingKeys(
      deps({
        globalTrackers: [{ id: 'global' }],
        workspacePaths: ['/persisted'],
        windowPaths: ['/open'],
        workspaceCount: 1,
        loadTrackers: async (repoRoot) =>
          ok({ trackers: [{ id: repoRoot === '/persisted' ? 'from-persisted' : 'from-open' }] }),
      }),
    )

    expect(keys).toEqual(new Set(['tracker:global', 'tracker:from-persisted', 'tracker:from-open']))
  })

  it('treats a repo with no config as contributing nothing, NOT as unknown', async () => {
    // The regression this exists for: `load` reports a missing file with the same
    // tag as a read failure, so failing open on every error meant one
    // unconfigured workspace — the normal case — disabled pruning entirely.
    const loadTrackers = vi.fn(async () => ok({ trackers: [{ id: 'configured' }] }))
    const keys = await liveTrackerBindingKeys(
      deps({
        workspacePaths: ['/unconfigured', '/configured'],
        workspaceCount: 2,
        configExists: async (repoRoot) => repoRoot === '/configured',
        loadTrackers,
      }),
    )

    expect(keys).toEqual(new Set(['tracker:configured']))
    expect(loadTrackers).toHaveBeenCalledTimes(1)
    expect(loadTrackers).toHaveBeenCalledWith('/configured')
  })

  it('fails open when a config exists but cannot be read', async () => {
    const keys = await liveTrackerBindingKeys(
      deps({
        workspacePaths: ['/unreadable'],
        workspaceCount: 1,
        loadTrackers: async () => err({ _tag: 'ConfigParseError' }),
      }),
    )

    // undefined = "keep every binding": pruning against a partial set would unbind
    // a credential the unreadable repository still uses.
    expect(keys).toBeUndefined()
  })

  it('fails open when the machine-global config exists but is invalid', async () => {
    const loadTrackers = vi.fn(async () => ok({ trackers: [] }))
    const keys = await liveTrackerBindingKeys(
      deps({
        globalTrackers: undefined,
        workspacePaths: ['/configured'],
        workspaceCount: 1,
        loadTrackers,
      }),
    )

    expect(keys).toBeUndefined()
    expect(loadTrackers).not.toHaveBeenCalled()
  })

  it('fails open when more workspaces are persisted than the listing returns', async () => {
    const loadTrackers = vi.fn(async () => ok({ trackers: [] }))
    const keys = await liveTrackerBindingKeys(
      deps({
        workspacePaths: Array.from({ length: 100 }, (_, i) => `/ws-${i}`),
        workspaceCount: 101,
        listMax: 100,
        loadTrackers,
      }),
    )

    expect(keys).toBeUndefined()
    // Bails before doing any work — the answer cannot be right at any cost.
    expect(loadTrackers).not.toHaveBeenCalled()
  })
})
