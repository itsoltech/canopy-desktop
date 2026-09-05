import { describe, it, expect } from 'vitest'
import { ipcErrorMessage } from './ipcErrorMessage'

describe('ipcErrorMessage', () => {
  it('strips the Electron IPC invoke prefix and inner Error prefix', () => {
    const e = new Error(
      "Error invoking remote method 'trackerConfig:fetchTransitions': Error: Jira API error 404: not found",
    )
    expect(ipcErrorMessage(e)).toBe('Jira API error 404: not found')
  })

  it('strips the IPC prefix when there is no inner Error prefix', () => {
    const e = new Error(
      "Error invoking remote method 'trackerConfig:addComment': No tracker configured",
    )
    expect(ipcErrorMessage(e)).toBe('No tracker configured')
  })

  it('strips repeated serialized error class prefixes consistently with CI surfaces', () => {
    const e = new Error(
      "Error invoking remote method 'git:getPullRequest': Error: TypeError: connect ECONNREFUSED",
    )
    expect(ipcErrorMessage(e)).toBe('connect ECONNREFUSED')
  })

  it('leaves plain error messages untouched', () => {
    expect(ipcErrorMessage(new Error('Network down'))).toBe('Network down')
  })

  it('stringifies non-Error values', () => {
    expect(ipcErrorMessage('boom')).toBe('boom')
  })

  it('uses the fallback for values that do not carry a user-facing message', () => {
    expect(ipcErrorMessage({ message: 'internal shape' }, 'Failed')).toBe('Failed')
  })

  it('falls back when the message is empty', () => {
    expect(ipcErrorMessage(new Error(''), 'Failed')).toBe('Failed')
  })
})
