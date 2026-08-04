import { gitErrorMessage, type GitError } from '../git/errors'

export type RepoIdentifierLookupFailure =
  { status: 'missing' } | { status: 'error'; message: string }

export function classifyRepoIdentifierLookupFailure(error: GitError): RepoIdentifierLookupFailure {
  if (error._tag === 'GitCommandFailed' && /no such remote ['"]?origin/i.test(error.message)) {
    return { status: 'missing' }
  }
  return { status: 'error', message: gitErrorMessage(error) }
}
