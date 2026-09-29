import { afterEach, describe, expect, it } from 'vitest'
import { closeDialog, confirm, confirmState, dialogState, showProjectCi } from './dialogs.svelte'

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

describe('confirm', () => {
  it('cancels a pending confirmation that a newer one replaces', async () => {
    const first = confirm({ title: 'Close tab?', message: 'This tab has a running process.' })
    const second = confirm({ title: 'Remote action request', message: 'Remote device wants…' })

    await expect(first).resolves.toBe(false)
    expect(confirmState.current?.title).toBe('Remote action request')

    confirmState.current?.onConfirm()
    await expect(second).resolves.toBe(true)
    expect(confirmState.current).toBeNull()
  })
})
