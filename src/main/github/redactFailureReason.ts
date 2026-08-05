import { errorMessage } from '../errors'

const REDACTED = '[redacted]'
const CREDENTIAL_URL_PATTERN = /\b(https?:\/\/)[^/@\s]+@/gi
const GH_TOKEN_PATTERN = /\b(?:gh[posru]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,})\b/g
const TOKEN_QUERY_PATTERN = /([?&](?:access_)?token=)[^&\s]+/gi

/** Remove credentials from gh stderr before an IPC error reaches the renderer. */
export function redactGitHubFailureReason(reason: string): string {
  return reason
    .replace(CREDENTIAL_URL_PATTERN, `$1${REDACTED}@`)
    .replace(GH_TOKEN_PATTERN, REDACTED)
    .replace(TOKEN_QUERY_PATTERN, `$1${REDACTED}`)
}

export function isMissingGitHubCli(error: unknown): boolean {
  return (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    (error as { code?: unknown }).code === 'ENOENT'
  )
}

/** Build one consistent, redacted user-facing reason for gh CLI failures. */
export function gitHubCliFailureReason(
  error: unknown,
  timeoutMs: number,
  stderrOverride?: string,
): string {
  if (
    typeof error === 'object' &&
    error !== null &&
    'killed' in error &&
    (error as { killed?: unknown }).killed === true
  ) {
    return `GitHub CLI request timed out after ${timeoutMs / 1000} seconds`
  }
  const stderr =
    stderrOverride ??
    (typeof error === 'object' &&
    error !== null &&
    'stderr' in error &&
    typeof (error as { stderr?: unknown }).stderr === 'string'
      ? (error as { stderr: string }).stderr
      : '')
  return redactGitHubFailureReason(stderr.trim() || errorMessage(error))
}
