import { mkdtempSync, mkdirSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { SkillStore } from './SkillStore'

const paths = vi.hoisted(() => ({ home: '' }))

vi.mock('os', async (importOriginal) => {
  const actual = await importOriginal<typeof import('os')>()
  return { ...actual, homedir: () => paths.home }
})

import { SkillInstaller } from './SkillInstaller'

describe('SkillInstaller local sources', () => {
  beforeEach(() => {
    paths.home = realpathSync(mkdtempSync(join(tmpdir(), 'canopy-skill-home-')))
  })

  afterEach(() => {
    rmSync(paths.home, { recursive: true, force: true })
  })

  it('rejects a skill file that symlinks into a dot directory outside the allowlist', async () => {
    mkdirSync(join(paths.home, '.ssh'))
    writeFileSync(join(paths.home, '.ssh', 'id_ed25519'), '---\nname: leaked\n---\nsecret key')
    mkdirSync(join(paths.home, 'notes'))
    const link = join(paths.home, 'notes', 'skill.md')
    symlinkSync(join(paths.home, '.ssh', 'id_ed25519'), link)
    // Reaching the store would mean the linked file was read and parsed as a skill.
    const store = { exists: () => true } as unknown as SkillStore

    const result = await new SkillInstaller(store).install({ source: link })

    expect(result.isErr() && result.error._tag).toBe('InvalidSource')
  })
})
