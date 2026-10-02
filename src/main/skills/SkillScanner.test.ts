import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@electron-toolkit/utils', () => ({ is: { dev: false } }))

import { scanSkills } from './SkillScanner'

describe('scanSkills', () => {
  let project: string
  let commands: string

  beforeEach(() => {
    project = mkdtempSync(join(tmpdir(), 'canopy-skill-scan-'))
    commands = join(project, '.claude', 'commands')
    mkdirSync(commands, { recursive: true })
  })

  afterEach(() => {
    rmSync(project, { recursive: true, force: true })
  })

  async function projectSkillIds(): Promise<string[]> {
    const skills = await scanSkills(project)
    return skills.filter((skill) => skill.scope === 'project').map((skill) => skill.id)
  }

  it('skips a repository skill file too large to be a prompt', async () => {
    writeFileSync(join(commands, 'review.md'), 'Review the diff.')
    writeFileSync(join(commands, 'huge.md'), 'x'.repeat(6 * 1024 * 1024))

    const ids = await projectSkillIds()

    expect(ids).toContain('review')
    expect(ids).not.toContain('huge')
  })

  it('still reads a skill file symlinked to a regular file', async () => {
    writeFileSync(join(project, 'shared-review.md'), 'Review the diff.')
    symlinkSync(join(project, 'shared-review.md'), join(commands, 'review.md'))

    expect(await projectSkillIds()).toContain('review')
  })
})
