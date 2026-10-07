import { describe, expect, it, vi } from 'vitest'

vi.mock('@electron-toolkit/utils', () => ({ is: { dev: true } }))

import { geminiAdapter } from './gemini'

function prefsWithCustomEnv(customEnv: Record<string, string>): {
  get(key: string): string | null
} {
  return { get: (key) => (key === 'gemini.customEnv' ? JSON.stringify(customEnv) : null) }
}

describe('geminiAdapter.buildEnvVars', () => {
  it('cannot redirect the per-session GEMINI_CLI_HOME through custom env', () => {
    const env = geminiAdapter.buildEnvVars(
      prefsWithCustomEnv({ GEMINI_CLI_HOME: '/tmp/elsewhere', FOO: 'bar' }),
    )

    expect(env).not.toHaveProperty('GEMINI_CLI_HOME')
    expect(env.FOO).toBe('bar')
  })
})
