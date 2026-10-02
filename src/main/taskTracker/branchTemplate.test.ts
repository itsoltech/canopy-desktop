import { describe, expect, it } from 'vitest'
import { buildVariables } from './branchTemplate'
import type { TrackerTask } from './types'

const task = (summary: string): TrackerTask => ({
  key: 'GAKKO-12',
  summary,
  description: '',
  status: 'To Do',
  priority: 'Medium',
  type: 'task',
})

describe('buildVariables', () => {
  it('folds accented letters in the title slug to ASCII instead of dropping them', () => {
    expect(buildVariables(task('Dodać obsługę żądań'), null).taskTitle).toBe('dodac-obsluge-zadan')
    expect(buildVariables(task('Straße für Größe'), null).taskTitle).toBe('strasse-fur-grosse')
  })
})
