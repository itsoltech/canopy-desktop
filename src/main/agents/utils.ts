export function deepMerge(
  target: Record<string, unknown>,
  source: Record<string, unknown>,
): Record<string, unknown> {
  const out = { ...target }
  for (const [key, val] of Object.entries(source)) {
    // Guard against prototype pollution: this merges untrusted parsed JSON
    // (e.g. a repo's .codex/hooks.json or ~/.gemini/settings.json), so a
    // crafted __proto__/constructor/prototype key must never reach assignment.
    if (key === '__proto__' || key === 'constructor' || key === 'prototype') continue
    if (
      val !== null &&
      typeof val === 'object' &&
      !Array.isArray(val) &&
      typeof out[key] === 'object' &&
      out[key] !== null &&
      !Array.isArray(out[key])
    ) {
      out[key] = deepMerge(out[key] as Record<string, unknown>, val as Record<string, unknown>)
    } else {
      out[key] = val
    }
  }
  return out
}

export function truncate(text: string, max: number): string {
  return text.length > max ? text.slice(0, max - 3) + '...' : text
}

// Keys whose value is a file's *body* rather than a description of it. Claude
// Code 2.1.280 made `file_text`/`file_content` accepted aliases for Write's
// `content`, so a Write that names its target `path` instead of `file_path`
// now validates upstream and reaches PreToolUse in that shape. Without this
// guard the loop below would pick whichever body key came first and put 80
// characters of file contents in a notch detail or an OS notification.
const BODY_KEYS = new Set([
  'content',
  'file_text',
  'file_content',
  'old_string',
  'new_string',
  'new_source',
])

export function summarizeToolInput(input?: Record<string, unknown>): string {
  if (!input) return ''

  if (typeof input.command === 'string') {
    return truncate(input.command, 80)
  }
  if (typeof input.file_path === 'string') {
    return input.file_path
  }
  if (Array.isArray(input.questions) && input.questions.length > 0) {
    const first = input.questions[0] as Record<string, unknown> | undefined
    if (first && typeof first.question === 'string') {
      return truncate(first.question, 80)
    }
  }
  if (typeof input.query === 'string') {
    return truncate(input.query, 80)
  }
  if (typeof input.url === 'string') {
    return truncate(input.url, 80)
  }
  if (typeof input.pattern === 'string') {
    let summary = input.pattern
    if (typeof input.path === 'string') {
      summary += ` in ${input.path}`
    }
    return truncate(summary, 80)
  }
  // Must stay below the `pattern` branch: Grep and Glob send `{pattern, path}`
  // and their summary is the pattern. Above `description`, so a Write carrying
  // both a `path` alias and a stray `description` still summarizes as the path.
  if (typeof input.path === 'string') {
    return input.path
  }
  if (typeof input.prompt === 'string') {
    return truncate(input.prompt, 80)
  }
  if (typeof input.description === 'string') {
    return truncate(input.description, 80)
  }
  if (typeof input.skill === 'string') {
    return input.skill
  }

  for (const [key, val] of Object.entries(input)) {
    if (BODY_KEYS.has(key)) continue
    if (typeof val === 'string' && val.length > 0) {
      return truncate(val, 80)
    }
  }

  return ''
}
