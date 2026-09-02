import { describe, expect, it } from 'vitest'
import { BLOCKED_ENV_VARS } from './envBlocklist'

describe('BLOCKED_ENV_VARS', () => {
  // Each of these makes a shell read an attacker-chosen file (or run an
  // attacker-chosen string) before the agent CLI's own command does, which is
  // the same arbitrary-execution class already covered by EDITOR/GIT_ASKPASS.
  it.each([
    ['BASH_ENV', 'bash sources it in non-interactive shells'],
    ['ENV', 'POSIX sh sources it at startup'],
    ['ZDOTDIR', 'zsh reads its startup files from this directory'],
    ['PROMPT_COMMAND', 'bash executes it before every prompt'],
  ])('blocks %s (%s)', (name) => {
    expect(BLOCKED_ENV_VARS.has(name)).toBe(true)
  })

  it('declares every entry in uppercase so callers can normalize with toUpperCase()', () => {
    for (const name of BLOCKED_ENV_VARS) {
      expect(name).toBe(name.toUpperCase())
    }
  })
})
