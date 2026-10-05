import { afterEach, describe, expect, it } from 'vitest'
import { closeDialog, dialogState, prompt, showAbout } from './dialogs.svelte'

function openPromptProps(): { onSubmit: (r: { value: string }) => void; onCancel: () => void } {
  const current = dialogState.current
  if (current.type !== 'input') throw new Error(`expected an input dialog, got ${current.type}`)
  return current.props
}

describe('prompt()', () => {
  afterEach(() => closeDialog())

  it('resolves with the submitted value and closes the dialog', async () => {
    const pending = prompt({ title: 'Commit message' })
    openPromptProps().onSubmit({ value: 'fix: thing' })

    await expect(pending).resolves.toEqual({ value: 'fix: thing' })
    expect(dialogState.current.type).toBe('none')
  })

  it('resolves null when another dialog replaces it', async () => {
    const pending = prompt({ title: 'Commit message' })
    showAbout()

    await expect(pending).resolves.toBeNull()
    expect(dialogState.current.type).toBe('about')
  })

  it('resolves null when the dialog is closed from outside', async () => {
    const pending = prompt({ title: 'Rename' })
    closeDialog()

    await expect(pending).resolves.toBeNull()
  })

  it('settles a prompt replaced by another prompt and keeps the new one working', async () => {
    const first = prompt({ title: 'First' })
    const second = prompt({ title: 'Second' })

    await expect(first).resolves.toBeNull()
    openPromptProps().onSubmit({ value: 'two' })
    await expect(second).resolves.toEqual({ value: 'two' })
  })
})
