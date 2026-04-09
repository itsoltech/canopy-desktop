import { join } from 'path'
import os from 'os'
import { mkdirSync, writeFileSync, symlinkSync, unlinkSync, existsSync } from 'fs'
import { ok, err } from '../../errors'
import type { Result } from 'neverthrow'
import type { SkillError } from '../errors'
import type { CanopySkill } from '../types'
import type { SkillTransformer } from '../SkillTransformer'

export const claudeTransformer: SkillTransformer = {
  agent: 'claude',
  projectDir: '.claude/commands',

  globalDir(): string {
    return join(os.homedir(), '.claude', 'commands')
  },

  deploy(skill: CanopySkill, targetRoot: string): Result<string, SkillError> {
    const dir = skill.scope === 'global' ? this.globalDir() : join(targetRoot, this.projectDir)
    mkdirSync(dir, { recursive: true })

    const fileName = `${skill.id}.md`
    const filePath = join(dir, fileName)

    const frontmatterLines = [`---`]
    if (skill.description) frontmatterLines.push(`description: ${skill.description}`)
    const meta = skill.metadata
    if (meta['allowed-tools']) {
      const tools = meta['allowed-tools'] as string[]
      frontmatterLines.push(`allowed-tools: [${tools.map((t) => `'${t}'`).join(', ')}]`)
    }
    if (meta['disable-model-invocation']) {
      frontmatterLines.push(`disable-model-invocation: true`)
    }
    frontmatterLines.push(`---`)

    const content = `${frontmatterLines.join('\n')}\n\n${skill.prompt}\n`

    if (skill.installMethod === 'symlink' && skill.sourceType === 'local') {
      try {
        if (existsSync(filePath)) unlinkSync(filePath)
        symlinkSync(skill.sourceUri, filePath)
      } catch (e) {
        return err({
          _tag: 'SymlinkFailed',
          path: filePath,
          cause: e instanceof Error ? e.message : String(e),
        })
      }
    } else {
      writeFileSync(filePath, content, 'utf-8')
    }

    return ok(filePath)
  },

  undeploy(skill: CanopySkill, targetRoot: string): Result<void, SkillError> {
    const dir = skill.scope === 'global' ? this.globalDir() : join(targetRoot, this.projectDir)
    const filePath = join(dir, `${skill.id}.md`)
    try {
      if (existsSync(filePath)) unlinkSync(filePath)
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
