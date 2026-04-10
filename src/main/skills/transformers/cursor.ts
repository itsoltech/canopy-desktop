import { join } from 'path'
import os from 'os'
import { mkdir, writeFile, symlink, unlink } from 'fs/promises'
import { ok, err, ResultAsync } from '../../errors'
import type { Result } from 'neverthrow'
import type { SkillError } from '../errors'
import type { CanopySkill } from '../types'
import type { SkillTransformer } from '../SkillTransformer'

export const cursorTransformer: SkillTransformer = {
  agent: 'cursor',
  projectDir: '.cursor/rules',

  globalDir(): string {
    return join(os.homedir(), '.cursor', 'rules')
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
    const fileName = `${skill.id}.mdc`
    const filePath = join(dir, fileName)
    const content = `---\ndescription: ${skill.description || skill.name}\nglobs: \nalwaysApply: false\n---\n\n${skill.prompt}\n`

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
      await writeFile(filePath, content, 'utf-8')
    }
    return ok(filePath)
  },

  async undeploy(skill: CanopySkill, targetRoot: string): Promise<Result<void, SkillError>> {
    const dir = skill.scope === 'global' ? this.globalDir() : join(targetRoot, this.projectDir)
    const filePath = join(dir, `${skill.id}.mdc`)
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
