import { match } from 'ts-pattern'

export type GitError =
  | { _tag: 'NotAGitRepo'; path: string }
  | { _tag: 'GitCommandFailed'; command: string; message: string }
  | { _tag: 'InvalidRef'; ref: string }
  | { _tag: 'WatcherStartFailed'; path: string; message: string }

export function gitErrorMessage(error: GitError): string {
  return match(error)
    .with({ _tag: 'NotAGitRepo' }, (e) => `Not a git repository: ${e.path}`)
    .with({ _tag: 'GitCommandFailed' }, (e) => `Git ${e.command} failed: ${e.message}`)
    .with({ _tag: 'InvalidRef' }, (e) => `Invalid git ref: ${e.ref}`)
    .with(
      { _tag: 'WatcherStartFailed' },
      (e) => `Failed to start git watcher at ${e.path}: ${e.message}`,
    )
    .exhaustive()
}
