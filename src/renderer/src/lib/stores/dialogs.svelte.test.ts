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
  afterEach(() => confirmState.current?.onCancel())

  it('resolves a replaced confirmation as cancelled instead of leaving it pending', async () => {
    const first = confirm({ title: 'First', message: 'first' })
    const second = confirm({ title: 'Second', message: 'second' })

    await expect(first).resolves.toBe(false)
    expect(confirmState.current?.title).toBe('Second')

    confirmState.current?.onConfirm()
    await expect(second).resolves.toBe(true)
    expect(confirmState.current).toBeNull()
  })

  it('removes the dialog and resolves false when its signal aborts', async () => {
    const controller = new AbortController()
    const pending = confirm({ title: 'Remote', message: 'remote' }, controller.signal)

    controller.abort()

    await expect(pending).resolves.toBe(false)
    expect(confirmState.current).toBeNull()
  })
})
