import { match } from 'ts-pattern'

export type GitError =
  | { _tag: 'NotAGitRepo'; path: string }
  | { _tag: 'GitCommandFailed'; command: string; message: string }
  | { _tag: 'InvalidRef'; ref: string }

export function gitErrorMessage(error: GitError): string {
  return match(error)
    .with({ _tag: 'NotAGitRepo' }, (e) => `Not a git repository: ${e.path}`)
    .with({ _tag: 'GitCommandFailed' }, (e) => `Git ${e.command} failed: ${e.message}`)
    .with({ _tag: 'InvalidRef' }, (e) => `Invalid git ref: ${e.ref}`)
    .exhaustive()
}
