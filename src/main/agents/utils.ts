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

// Resume session ids round-trip through a renderer-supplied serialized layout, which is
// only type-checked (assertOptionalString), not shape-checked. They are then passed as a
// bare argv element to an external agent CLI, so a value starting with "-" would be read
// as a flag by that CLI's own parser. Require a leading alphanumeric and a conservative
// charset that still covers every id shape the agents emit (UUID, ULID, ts-prefixed).
const RESUME_SESSION_ID = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/

export function isValidResumeSessionId(id: string): boolean {
  return RESUME_SESSION_ID.test(id)
}

export function truncate(text: string, max: number): string {
  return text.length > max ? text.slice(0, max - 3) + '...' : text
}

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
  if (typeof input.prompt === 'string') {
    return truncate(input.prompt, 80)
  }
  if (typeof input.description === 'string') {
    return truncate(input.description, 80)
  }
  if (typeof input.skill === 'string') {
    return input.skill
  }

  for (const val of Object.values(input)) {
    if (typeof val === 'string' && val.length > 0) {
      return truncate(val, 80)
    }
  }

  return ''
}
