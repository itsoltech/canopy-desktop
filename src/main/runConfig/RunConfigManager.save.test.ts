import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { RunConfigManager } from './RunConfigManager'

const TWO_CONFIGS = `[[configurations]]
name = "dev"
command = "npm run dev"

[[configurations]]
name = "test"
command = "npm test"
`

describe('RunConfigManager writes', () => {
  let root: string
  let project: string

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'canopy-run-config-'))
    project = join(root, 'project')
    mkdirSync(join(project, '.canopy'), { recursive: true })
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  it('rejects renaming a configuration onto the name of another one', async () => {
    const tomlFile = join(project, '.canopy', 'run.toml')
    writeFileSync(tomlFile, TWO_CONFIGS)

    const result = await new RunConfigManager().updateConfiguration(project, 'dev', {
      name: 'test',
      command: 'npm run dev',
    })

    expect(result._unsafeUnwrapErr()).toMatchObject({ _tag: 'RunConfigValidationError' })
    expect(readFileSync(tomlFile, 'utf-8')).toBe(TWO_CONFIGS)
  })

  it('still saves an edit that keeps the configuration name', async () => {
    writeFileSync(join(project, '.canopy', 'run.toml'), TWO_CONFIGS)
    const manager = new RunConfigManager()

    const result = await manager.updateConfiguration(project, 'dev', {
      name: 'dev',
      command: 'pnpm dev',
    })

    expect(result.isOk()).toBe(true)
    const file = (await manager.loadFile(project))._unsafeUnwrap()
    expect(file.configurations.map((c) => [c.name, c.command])).toEqual([
      ['dev', 'pnpm dev'],
      ['test', 'npm test'],
    ])
  })

  it('does not write through a run.toml symlink that points outside the project', async () => {
    const outside = join(root, 'outside.toml')
    const original = 'model = "keep-me"\n'
    writeFileSync(outside, original)
    symlinkSync(outside, join(project, '.canopy', 'run.toml'))

    const result = await new RunConfigManager().addConfiguration(project, {
      name: 'dev',
      command: 'npm run dev',
    })

    expect(result._unsafeUnwrapErr()).toMatchObject({ _tag: 'RunConfigWriteError' })
    expect(readFileSync(outside, 'utf-8')).toBe(original)
  })

  it('does not write into a .canopy directory symlinked outside the project', async () => {
    const elsewhere = join(root, 'elsewhere')
    mkdirSync(elsewhere)
    const linked = join(root, 'linked-project')
    mkdirSync(linked)
    symlinkSync(elsewhere, join(linked, '.canopy'))

    const result = await new RunConfigManager().saveFile(linked, {
      configurations: [{ name: 'dev', command: 'npm run dev' }],
    })

    expect(result._unsafeUnwrapErr()).toMatchObject({ _tag: 'RunConfigWriteError' })
    expect(() => readFileSync(join(elsewhere, 'run.toml'))).toThrow()
  })

  it('keeps writing through a run.toml symlink that stays inside the project', async () => {
    mkdirSync(join(project, 'shared'))
    const shared = join(project, 'shared', 'run.toml')
    writeFileSync(shared, TWO_CONFIGS)
    symlinkSync(shared, join(project, '.canopy', 'run.toml'))

    const result = await new RunConfigManager().deleteConfiguration(project, 'test')

    expect(result.isOk()).toBe(true)
    expect(readFileSync(shared, 'utf-8')).not.toContain('npm test')
  })
})
