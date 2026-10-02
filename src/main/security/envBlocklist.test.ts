import { describe, expect, it } from 'vitest'
import { BLOCKED_ENV_VARS } from './envBlocklist'

describe('BLOCKED_ENV_VARS', () => {
  it.each(['BASH_ENV', 'NODE_PATH', 'RUBYOPT', 'GIT_SSH', 'GIT_CONFIG_COUNT', 'GIT_CONFIG_GLOBAL'])(
    'blocks %s, which makes a spawned process run attacker-chosen code',
    (name) => {
      expect(BLOCKED_ENV_VARS.has(name)).toBe(true)
    },
  )

  it('leaves ordinary agent settings alone', () => {
    expect(BLOCKED_ENV_VARS.has('ANTHROPIC_MODEL')).toBe(false)
  })
})
