import { describe, expect, it } from 'vitest'
import { ipcErrorMessage } from './errors'

describe('ipcErrorMessage', () => {
  it('drops the Electron transport wrapper the user cannot act on', () => {
    // Verbatim from the jobs-history window before this existed.
    const raw = new Error(
      "Error invoking remote method 'ci:activity': Error: TeamCity API error 401: Could not authenticate with provided token",
    )
    expect(ipcErrorMessage(raw)).toBe(
      'TeamCity API error 401: Could not authenticate with provided token',
    )
  })

  it('strips a class prefix repeated by nested rethrows', () => {
    expect(ipcErrorMessage(new Error('Error: TypeError: boom'))).toBe('boom')
  })

  it('leaves a plain main-process message alone', () => {
    expect(ipcErrorMessage(new Error('TeamCity API error 500: upstream down'))).toBe(
      'TeamCity API error 500: upstream down',
    )
  })

  it('falls back rather than showing an empty box', () => {
    expect(ipcErrorMessage(new Error("Error invoking remote method 'ci:activity': Error: "))).toBe(
      'Request failed',
    )
    expect(ipcErrorMessage(undefined, 'Could not load history')).toBe('Could not load history')
  })
})
