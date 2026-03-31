export function truncate(text: string, max: number): string {
  return text.length > max ? text.slice(0, max - 3) + '...' : text
}

export function summarizeToolInput(input?: Record<string, unknown>): string {
  if (!input) return ''

  if (typeof input.command === 'string') {
    return truncate(input.command, 80)
  }
  if (typeof input.file_path === 'string') {
    return input.file_path as string
  }
  if (Array.isArray(input.questions) && input.questions.length > 0) {
    const first = input.questions[0] as Record<string, unknown> | undefined
    if (first && typeof first.question === 'string') {
      return truncate(first.question as string, 80)
    }
  }
  if (typeof input.query === 'string') {
    return truncate(input.query, 80)
  }
  if (typeof input.url === 'string') {
    return truncate(input.url, 80)
  }
  if (typeof input.pattern === 'string') {
    let summary = input.pattern as string
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
    return input.skill as string
  }

  for (const val of Object.values(input)) {
    if (typeof val === 'string' && val.length > 0) {
      return truncate(val, 80)
    }
  }

  return ''
}
