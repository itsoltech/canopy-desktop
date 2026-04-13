import { match } from 'ts-pattern'
import type { AgentType } from '../agents/types'

export type ProfileError =
  | { _tag: 'ProfileNotFound'; id: string }
  | { _tag: 'ProfileNameConflict'; agentType: AgentType; name: string }
  | { _tag: 'ProfileLastDeletion'; id: string }
  | { _tag: 'ProfileWriteError'; reason: string }
  | { _tag: 'ProfileValidationError'; reason: string }

export function profileErrorMessage(error: ProfileError): string {
  return match(error)
    .with({ _tag: 'ProfileNotFound' }, (e) => `Profile not found: ${e.id}`)
    .with(
      { _tag: 'ProfileNameConflict' },
      (e) => `A ${e.agentType} profile named "${e.name}" already exists`,
    )
    .with(
      { _tag: 'ProfileLastDeletion' },
      () => 'Cannot delete the only profile for this agent — create another one first',
    )
    .with({ _tag: 'ProfileWriteError' }, (e) => `Failed to write profile: ${e.reason}`)
    .with({ _tag: 'ProfileValidationError' }, (e) => `Invalid profile: ${e.reason}`)
    .exhaustive()
}
