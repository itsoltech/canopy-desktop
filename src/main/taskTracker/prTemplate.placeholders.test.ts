import { describe, expect, it } from 'vitest'
import { renderPRBody, renderPRTitle } from './prTemplate'
import type { TrackerTask } from './types'

function task(overrides: Partial<TrackerTask> = {}): TrackerTask {
  return {
    key: 'PROJ-1',
    summary: 'Fix login',
    description: 'Steps to reproduce',
    status: 'Open',
    priority: 'High',
    type: 'bug',
    url: 'https://tracker.example/PROJ-1',
    ...overrides,
  }
}

describe('PR template placeholders', () => {
  it('inserts tracker text containing $ replacement patterns literally', () => {
    const summary = "Price shown as $$ instead of $& and $' in cart"

    expect(renderPRTitle('[{taskKey}] {taskTitle}', task({ summary }))).toBe(`[PROJ-1] ${summary}`)
  })

  it('does not expand placeholders that appear inside substituted tracker text', () => {
    const summary = 'Document the {taskDescription} and {taskUrl} tokens'

    expect(renderPRBody('{taskTitle}\n\n{taskDescription}', task({ summary }))).toBe(
      `${summary}\n\nSteps to reproduce`,
    )
  })

  it('keeps unknown placeholders and fills missing fields with an empty string', () => {
    expect(
      renderPRBody('{taskKey} {unknown} {parentKey}|{boardKey}', task({ parentKey: undefined })),
    ).toBe('PROJ-1 {unknown} |PROJ')
  })
})
