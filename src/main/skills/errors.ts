import { match } from 'ts-pattern'

export type SkillError =
  | { _tag: 'SkillNotFound'; skillId: string }
  | { _tag: 'SkillAlreadyInstalled'; skillId: string }
  | { _tag: 'InvalidSource'; source: string; reason: string }
  | { _tag: 'FetchFailed'; source: string; cause: string }
  | { _tag: 'ParseFailed'; source: string; reason: string }
  | { _tag: 'TransformFailed'; agent: string; reason: string }
  | { _tag: 'InstallFailed'; skillId: string; reason: string }
  | { _tag: 'SymlinkFailed'; path: string; cause: string }

export function skillErrorMessage(error: SkillError): string {
  return match(error)
    .with({ _tag: 'SkillNotFound' }, (e) => `Skill not found: ${e.skillId}`)
    .with({ _tag: 'SkillAlreadyInstalled' }, (e) => `Skill already installed: ${e.skillId}`)
    .with({ _tag: 'InvalidSource' }, (e) => `Invalid source "${e.source}": ${e.reason}`)
    .with({ _tag: 'FetchFailed' }, (e) => `Failed to fetch "${e.source}": ${e.cause}`)
    .with({ _tag: 'ParseFailed' }, (e) => `Failed to parse "${e.source}": ${e.reason}`)
    .with({ _tag: 'TransformFailed' }, (e) => `Failed to transform for ${e.agent}: ${e.reason}`)
    .with({ _tag: 'InstallFailed' }, (e) => `Failed to install "${e.skillId}": ${e.reason}`)
    .with({ _tag: 'SymlinkFailed' }, (e) => `Symlink failed at ${e.path}: ${e.cause}`)
    .exhaustive()
}
