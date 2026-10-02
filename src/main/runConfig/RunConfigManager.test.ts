import { mkdtemp, mkdir, readFile, rm, writeFile } from 'fs/promises'
import { tmpdir } from 'os'
import { join } from 'path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { RunConfigManager } from './RunConfigManager'

describe('RunConfigManager.addConfiguration', () => {
  let dir: string

  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), 'canopy-run-config-'))
  })

  afterEach(async () => {
    await rm(dir, { recursive: true, force: true })
  })

  it('creates .canopy/run.toml when none exists', async () => {
    const result = await new RunConfigManager().addConfiguration(dir, {
      name: 'dev',
      command: 'npm run dev',
    })

    expect(result.isOk()).toBe(true)
    const saved = await new RunConfigManager().loadFile(dir)
    expect(saved._unsafeUnwrap().configurations.map((c) => c.name)).toEqual(['dev'])
  })

  it('refuses to overwrite a run.toml it cannot parse', async () => {
    const tomlPath = join(dir, '.canopy', 'run.toml')
    const conflicted = '<<<<<<< HEAD\n[[configurations]]\nname = "dev"\ncommand = "npm run dev"\n'
    await mkdir(join(dir, '.canopy'))
    await writeFile(tomlPath, conflicted, 'utf-8')

    const result = await new RunConfigManager().addConfiguration(dir, {
      name: 'test',
      command: 'npm test',
    })

    expect(result.isErr()).toBe(true)
    expect(result._unsafeUnwrapErr()._tag).toBe('RunConfigParseError')
    expect(await readFile(tomlPath, 'utf-8')).toBe(conflicted)
  })
})
