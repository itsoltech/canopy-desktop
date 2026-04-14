import { join } from 'path'
import os from 'os'
import { mkdir, writeFile, symlink, unlink } from 'fs/promises'
import { ok, err, ResultAsync } from '../../errors'
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

  async deploy(skill: CanopySkill, targetRoot: string): Promise<Result<string, SkillError>> {
    const dir = skill.scope === 'global' ? this.globalDir() : join(targetRoot, this.projectDir)
    const mkdirResult = await ResultAsync.fromPromise(
      mkdir(dir, { recursive: true }),
      (e): SkillError => ({
        _tag: 'InstallFailed',
        skillId: skill.id,
        reason: `Failed to create directory: ${e instanceof Error ? e.message : String(e)}`,
      }),
    )
    if (mkdirResult.isErr()) return err(mkdirResult.error)

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
      const symlinkResult = await ResultAsync.fromPromise(
        (async () => {
          try {
            await unlink(filePath)
          } catch {
            /* file may not exist */
          }
          await symlink(skill.sourceUri, filePath)
        })(),
        (e): SkillError => ({
          _tag: 'SymlinkFailed',
          path: filePath,
          cause: e instanceof Error ? e.message : String(e),
        }),
      )
      if (symlinkResult.isErr()) return err(symlinkResult.error)
    } else {
      const writeResult = await ResultAsync.fromPromise(
        writeFile(filePath, content, 'utf-8'),
        (e): SkillError => ({
          _tag: 'InstallFailed',
          skillId: skill.id,
          reason: `Failed to write file: ${e instanceof Error ? e.message : String(e)}`,
        }),
      )
      if (writeResult.isErr()) return err(writeResult.error)
    }

    return ok(filePath)
  },

  async undeploy(skill: CanopySkill, targetRoot: string): Promise<Result<void, SkillError>> {
    const dir = skill.scope === 'global' ? this.globalDir() : join(targetRoot, this.projectDir)
    const filePath = join(dir, `${skill.id}.md`)
    const unlinkResult = await ResultAsync.fromPromise(
      unlink(filePath).catch(() => {}),
      (e): SkillError => ({
        _tag: 'InstallFailed',
        skillId: skill.id,
        reason: `Failed to remove: ${e instanceof Error ? e.message : String(e)}`,
      }),
    )
    if (unlinkResult.isErr()) return err(unlinkResult.error)
    return ok(undefined)
  },
}
