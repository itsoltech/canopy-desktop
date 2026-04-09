import { parseSkillsCommand, isSkillsCommand } from './skillsCommand'
import {
  formatSkillList,
  formatInstallSuccess,
  formatRemoveSuccess,
  formatUpdateSuccess,
  formatSkillInfo,
  formatError,
} from './skillsCommandOutput'

export { isSkillsCommand }

export async function executeSkillsCommand(input: string): Promise<string> {
  const parsed = parseSkillsCommand(input)
  if (!parsed)
    return formatError('Unknown command. Usage: canopy skills <install|list|remove|update|info>')

  try {
    switch (parsed.action) {
      case 'list': {
        const opts: Record<string, unknown> = {}
        if (parsed.args.global) opts.scope = 'global'
        if (typeof parsed.args.agent === 'string') opts.agent = parsed.args.agent
        const skills = await window.api.listSkills(opts)
        return formatSkillList(skills)
      }

      case 'install': {
        const source = parsed.positional[0]
        if (!source)
          return formatError(
            'Usage: canopy skills install <source> [--agent <agent>] [--global] [--copy|--symlink]',
          )
        const opts: Record<string, unknown> = { source }
        if (typeof parsed.args.agent === 'string') opts.agents = [parsed.args.agent]
        if (parsed.args.global) opts.scope = 'global'
        if (parsed.args.symlink) opts.method = 'symlink'
        if (parsed.args.copy) opts.method = 'copy'
        const skill = await window.api.installSkill(opts)
        return formatInstallSuccess(skill)
      }

      case 'remove': {
        const id = parsed.positional[0]
        if (!id) return formatError('Usage: canopy skills remove <skill-id>')
        await window.api.removeSkill(id)
        return formatRemoveSuccess(id)
      }

      case 'update': {
        const id = parsed.positional[0]
        if (!id && !parsed.args.all)
          return formatError('Usage: canopy skills update <skill-id> | --all')
        if (parsed.args.all) {
          const skills = await window.api.listSkills()
          for (const s of skills) {
            await window.api.updateSkill(s.id)
          }
          return `Updated ${skills.length} skill(s)\r\n`
        }
        const skill = await window.api.updateSkill(id!)
        return formatUpdateSuccess(skill)
      }

      case 'info': {
        const id = parsed.positional[0]
        if (!id) return formatError('Usage: canopy skills info <skill-id>')
        const skill = await window.api.getSkill(id)
        if (!skill) return formatError(`Skill not found: ${id}`)
        return formatSkillInfo(skill)
      }

      default:
        return formatError('Unknown action. Available: install, list, remove, update, info')
    }
  } catch (e) {
    return formatError(e instanceof Error ? e.message : String(e))
  }
}
