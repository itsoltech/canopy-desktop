import { beforeEach, describe, expect, it, vi } from 'vitest'

// The real preferences store goes through window.api; an in-memory map is enough here.
const h = vi.hoisted(() => ({ values: new Map<string, string>() }))

vi.mock('../stores/preferences.svelte', () => ({
  getPref: (key: string, def = ''): string => h.values.get(key) ?? def,
  setPref: (key: string, value: string): Promise<void> => {
    h.values.set(key, value)
    return Promise.resolve()
  },
}))

import { NO_SPRINT, loadSavedTaskFilters } from './taskFilterPrefs'

const PREF_KEY = 'taskTracker.pickerFilters.jira-main.ABC'

describe('loadSavedTaskFilters', () => {
  beforeEach(() => {
    h.values.clear()
  })

  it('ignores a stored value without the saved-filter shape', () => {
    h.values.set(PREF_KEY, '{}')

    expect(loadSavedTaskFilters('jira-main', 'ABC')).toBeNull()
  })

  it('restores saved filters and maps the legacy no-sprint bucket', () => {
    h.values.set(
      PREF_KEY,
      JSON.stringify({
        excludedStatuses: ['Done'],
        excludedSprints: ['(no sprint)'],
        assignedToMe: true,
      }),
    )

    expect(loadSavedTaskFilters('jira-main', 'ABC')).toEqual({
      excludedStatuses: ['Done'],
      excludedSprints: [NO_SPRINT],
      assignedToMe: true,
    })
  })
})
