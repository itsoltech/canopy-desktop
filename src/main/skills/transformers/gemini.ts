import { join } from 'path'
import os from 'os'
import { mkdir, writeFile, symlink, unlink } from 'fs/promises'
import { existsSync } from 'fs'
import { ok, err } from '../../errors'
import type { Result } from 'neverthrow'
import type { SkillError } from '../errors'
import type { CanopySkill } from '../types'
import type { SkillTransformer } from '../SkillTransformer'

export const geminiTransformer: SkillTransformer = {
  agent: 'gemini',
  projectDir: '.gemini/skills',

  globalDir(): string {
    return join(os.homedir(), '.gemini', 'skills')
  },

  async deploy(skill: CanopySkill, targetRoot: string): Promise<Result<string, SkillError>> {
    const dir = skill.scope === 'global' ? this.globalDir() : join(targetRoot, this.projectDir)
    await mkdir(dir, { recursive: true })
    const fileName = `${skill.id}.md`
    const filePath = join(dir, fileName)
    const content = `# ${skill.name}\n\n${skill.description ? `${skill.description}\n\n` : ''}${skill.prompt}\n`

    if (skill.installMethod === 'symlink' && skill.sourceType === 'local') {
      try {
        if (existsSync(filePath)) await unlink(filePath)
        await symlink(skill.sourceUri, filePath)
      } catch (e) {
        return err({
          _tag: 'SymlinkFailed',
          path: filePath,
          cause: e instanceof Error ? e.message : String(e),
        })
      }
    } else {
      await writeFile(filePath, content, 'utf-8')
    }
    return ok(filePath)
  },

  async undeploy(skill: CanopySkill, targetRoot: string): Promise<Result<void, SkillError>> {
    const dir = skill.scope === 'global' ? this.globalDir() : join(targetRoot, this.projectDir)
    const filePath = join(dir, `${skill.id}.md`)
    try {
      if (existsSync(filePath)) await unlink(filePath)
    } catch (e) {
      return err({
        _tag: 'InstallFailed',
        skillId: skill.id,
        reason: `Failed to remove: ${e instanceof Error ? e.message : String(e)}`,
      })
    }
    return ok(undefined)
  },
}
