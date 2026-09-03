import { afterEach, describe, expect, it } from 'vitest'
import { closeDialog, dialogState, showProjectCi } from './dialogs.svelte'

describe('project CI dialog entry mode', () => {
  afterEach(() => closeDialog())

  it('routes token recovery without opening the shared configuration editor', () => {
    showProjectCi('C:/repo-a', 'credentials')

    expect(dialogState.current).toEqual({
      type: 'projectCi',
      repoRoot: 'C:/repo-a',
      mode: 'credentials',
    })
  })

  it('keeps explicit configuration entry separate', () => {
    showProjectCi('C:/repo-b')

    expect(dialogState.current).toEqual({
      type: 'projectCi',
      repoRoot: 'C:/repo-b',
      mode: 'configuration',
    })
  })
})
