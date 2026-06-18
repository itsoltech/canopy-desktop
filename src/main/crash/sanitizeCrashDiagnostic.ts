const REDACTED = '[redacted]'
const DEFAULT_MAX_CHARS = 4000
const USER_PATH_PATTERN = /(?:[A-Z]:\\Users\\[^\\\s]+\\|\/(?:Users|home)\/[^/\s]+\/)/gi
const URL_PATTERN = /\bhttps?:\/\/[^\s<>"'`]+/gi
const KEY_VALUE_SECRET_PATTERN =
  /\b(token|secret|password|passwd|apikey|api_key|authorization)\b(\s*[:=]\s*)(?:Bearer\s+)?[^\s,;'"`<>)]+/gi
const BEARER_TOKEN_PATTERN = /\bBearer\s+[A-Za-z0-9._~+/=-]{6,}/gi
const TOKEN_LIKE_PATTERN =
  /\b(?:token|secret|password|passwd|apikey|api_key|authorization|bearer)[A-Za-z0-9_\-:=./+]{3,}/gi
const ENV_VALUE_PATTERN = /\b[A-Z][A-Z0-9_]{2,}=([^\s]+)/g

export function sanitizeDiagnosticText(value: string | undefined): string | undefined {
  if (!value) return value
  return value
    .replace(USER_PATH_PATTERN, '~/')
    .replace(URL_PATTERN, REDACTED)
    .replace(KEY_VALUE_SECRET_PATTERN, (_match, key: string, separator: string) => {
      return `${key}${separator}${REDACTED}`
    })
    .replace(BEARER_TOKEN_PATTERN, `Bearer ${REDACTED}`)
    .replace(TOKEN_LIKE_PATTERN, REDACTED)
    .replace(ENV_VALUE_PATTERN, (match) => {
      const idx = match.indexOf('=')
      return idx >= 0 ? `${match.slice(0, idx + 1)}${REDACTED}` : REDACTED
    })
}

export function sanitizeStack(
  value: string | undefined,
  maxChars = DEFAULT_MAX_CHARS,
): string | undefined {
  const sanitized = sanitizeDiagnosticText(value)
  if (!sanitized) return sanitized
  if (sanitized.length <= maxChars) return sanitized
  return `${sanitized.slice(0, maxChars - 20)}\n... (truncated)`
}
