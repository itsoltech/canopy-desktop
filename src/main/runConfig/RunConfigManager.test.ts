import { describe, expect, it } from 'vitest'
import { isRunConfiguration } from './RunConfigManager'

describe('isRunConfiguration', () => {
  const valid = { name: 'dev', command: 'npm' }

  it('accepts a minimal configuration', () => {
    expect(isRunConfiguration(valid)).toBe(true)
  })

  it('accepts a fully populated configuration', () => {
    expect(
      isRunConfiguration({
        name: 'dev',
        command: 'npm',
        args: 'run dev',
        cwd: 'packages/app',
        env: { NODE_ENV: 'development' },
        max_instances: 2,
        pre_run: 'install',
        post_run: 'cleanup',
      }),
    ).toBe(true)
  })

  it('rejects values that are not objects', () => {
    expect(isRunConfiguration(null)).toBe(false)
    expect(isRunConfiguration(undefined)).toBe(false)
    expect(isRunConfiguration('npm run dev')).toBe(false)
    expect(isRunConfiguration(3)).toBe(false)
    expect(isRunConfiguration([])).toBe(false)
  })

  it('rejects an entry with no command', () => {
    expect(isRunConfiguration({ name: 'dev' })).toBe(false)
  })

  it('rejects an entry with no name', () => {
    expect(isRunConfiguration({ command: 'npm' })).toBe(false)
  })

  it('rejects a non-string command, which would otherwise spawn as undefined', () => {
    expect(isRunConfiguration({ name: 'dev', command: 42 })).toBe(false)
    expect(isRunConfiguration({ name: 'dev', command: ['npm', 'run'] })).toBe(false)
  })

  it('rejects non-string optional text fields', () => {
    for (const key of ['args', 'cwd', 'pre_run', 'post_run']) {
      expect(isRunConfiguration({ ...valid, [key]: 1 })).toBe(false)
    }
  })

  it('allows optional string fields to be absent', () => {
    expect(isRunConfiguration({ ...valid, args: undefined, cwd: undefined })).toBe(true)
  })

  it('rejects a non-numeric max_instances', () => {
    expect(isRunConfiguration({ ...valid, max_instances: '2' })).toBe(false)
  })

  it('rejects env that is not a string map', () => {
    expect(isRunConfiguration({ ...valid, env: 'NODE_ENV=dev' })).toBe(false)
    expect(isRunConfiguration({ ...valid, env: ['NODE_ENV=dev'] })).toBe(false)
    expect(isRunConfiguration({ ...valid, env: { PORT: 3000 } })).toBe(false)
    expect(isRunConfiguration({ ...valid, env: { NESTED: { a: 'b' } } })).toBe(false)
  })

  it('accepts an empty env map', () => {
    expect(isRunConfiguration({ ...valid, env: {} })).toBe(true)
  })

  it('ignores unknown extra fields so forward-compatible files still load', () => {
    expect(isRunConfiguration({ ...valid, future_option: true })).toBe(true)
  })
})
