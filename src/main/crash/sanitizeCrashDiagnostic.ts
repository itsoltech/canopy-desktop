const REDACTED = '[redacted]'
const DEFAULT_MAX_CHARS = 4000
const USER_PATH_PATTERN = /(?:[A-Z]:\\Users\\[^\\\s]+\\|\/(?:Users|home)\/[^/\s]+\/)/gi
const URL_PATTERN = /\bhttps?:\/\/[^\s<>"'`]+/gi
const SECRET_KEY_VALUE_PATTERN =
  /(["'`]?)(\b(?:x-api-key|api[-_]?key|apikey|access[-_]?token|refresh[-_]?token|client[-_]?secret|session[-_]?id|session|cookie|token|secret|password|passwd)\b)\1(\s*[:=]\s*)(["'`]?)([^\s,;'"`<>)\]}]+)\4/gi
const AUTHORIZATION_SECRET_PATTERN =
  /(["'`]?)(\bauthorization\b)\1(\s*[:=]\s*)(["'`]?)([^\n\r,;<>)}\]]+)\4/gi
const BEARER_TOKEN_PATTERN = /\bBearer\s+[A-Za-z0-9._~+/=-]{6,}/gi
const BASIC_TOKEN_PATTERN = /\bBasic\s+[A-Za-z0-9+/=_-]{6,}/gi
const JWT_PATTERN = /\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}/g
const AWS_KEY_PATTERN = /\b(?:AKIA|ASIA)[A-Z0-9]{16}\b/g
const GH_TOKEN_PATTERN = /\b(?:gh[posru]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,})\b/g
const PEM_PRIVATE_KEY_PATTERN =
  /-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----/g
const TOKEN_LIKE_PATTERN =
  /\b(?:token|secret|password|passwd|apikey|api_key|authorization|bearer)\s*[:=]\s*[A-Za-z0-9_\-./+]{3,}/gi
const ENV_VALUE_PATTERN = /\b[A-Z][A-Z0-9_]{2,}=([^\s]+)/g

export function sanitizeDiagnosticText(value: string | undefined): string | undefined {
  if (!value) return value
  return value
    .replace(USER_PATH_PATTERN, '~/')
    .replace(URL_PATTERN, REDACTED)
    .replace(
      AUTHORIZATION_SECRET_PATTERN,
      (_match, keyQuote: string, key: string, separator: string, valueQuote: string) => {
        return `${keyQuote}${key}${keyQuote}${separator}${valueQuote}${REDACTED}${valueQuote}`
      },
    )
    .replace(
      SECRET_KEY_VALUE_PATTERN,
      (_match, keyQuote: string, key: string, separator: string, valueQuote: string) => {
        return `${keyQuote}${key}${keyQuote}${separator}${valueQuote}${REDACTED}${valueQuote}`
      },
    )
    .replace(BEARER_TOKEN_PATTERN, `Bearer ${REDACTED}`)
    .replace(BASIC_TOKEN_PATTERN, `Basic ${REDACTED}`)
    .replace(JWT_PATTERN, REDACTED)
    .replace(AWS_KEY_PATTERN, REDACTED)
    .replace(GH_TOKEN_PATTERN, REDACTED)
    .replace(PEM_PRIVATE_KEY_PATTERN, REDACTED)
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
