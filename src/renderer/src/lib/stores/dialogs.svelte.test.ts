import { afterEach, describe, expect, it } from 'vitest'
import { closeDialog, dialogState, showProjectCi } from './dialogs.svelte'

describe('project CI dialog entry mode', () => {
  afterEach(() => closeDialog())

  it('routes token recovery without opening the shared configuration editor', () => {
    showProjectCi('credentials')

    expect(dialogState.current).toEqual({ type: 'projectCi', mode: 'credentials' })
  })

  it('keeps explicit configuration entry separate', () => {
    showProjectCi()

    expect(dialogState.current).toEqual({ type: 'projectCi', mode: 'configuration' })
  })
})
