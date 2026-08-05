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
