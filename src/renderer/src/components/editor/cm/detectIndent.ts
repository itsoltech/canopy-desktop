export interface IndentInfo {
  type: 'space' | 'tab'
  size: number
}

export function detectIndent(content: string): IndentInfo {
  const lines = content.split('\n').slice(0, 1000)
  let tabCount = 0
  const indentLevels: number[] = []

  for (const line of lines) {
    if (line.length === 0) continue
    if (line[0] === '\t') {
      tabCount++
      continue
    }
    if (line[0] === ' ') {
      let n = 0
      while (n < line.length && line[n] === ' ') n++
      // Skip 1-space prefixes (common for JSDoc continuation: " *")
      if (n >= 2) indentLevels.push(n)
    } else {
      indentLevels.push(0)
    }
  }

  if (tabCount > indentLevels.filter((n) => n > 0).length) {
    return { type: 'tab', size: 4 }
  }

  // Count most common POSITIVE step between consecutive indent levels.
  // This is robust vs. the "any multiple of 2 beats 4" trap because each
  // 4-space indent increase contributes +4 to the histogram, not +2.
  const diffCounts: Record<number, number> = {}
  let prev = 0
  for (const n of indentLevels) {
    if (n > prev) {
      const d = n - prev
      if (d >= 1 && d <= 8) {
        diffCounts[d] = (diffCounts[d] ?? 0) + 1
      }
    }
    prev = n
  }

  let best = 0
  let bestCount = 0
  for (const [d, count] of Object.entries(diffCounts)) {
    if (count > bestCount) {
      bestCount = count
      best = Number(d)
    }
  }

  if (best === 0) {
    // No increases seen — fall back to the smallest indent that actually
    // appears in the file (ignores the "divisor of 2 trap").
    const positive = indentLevels.filter((n) => n > 0)
    if (positive.length > 0) {
      best = Math.min(...positive)
    } else {
      // Nothing to go on — default to 4 spaces (more common across languages)
      return { type: 'space', size: 4 }
    }
  }

  // Snap to the nearest common size
  const snapped = best >= 7 ? 8 : best >= 3 ? 4 : 2
  return { type: 'space', size: snapped }
}

export function indentUnitString(info: IndentInfo): string {
  if (info.type === 'tab') return '\t'
  return ' '.repeat(info.size)
}
