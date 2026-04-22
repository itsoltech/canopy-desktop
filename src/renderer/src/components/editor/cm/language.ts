import { languages, type LanguageDescription } from '@codemirror/language-data'
import type { LanguageSupport } from '@codemirror/language'

function findLanguageDescription(filePath: string): LanguageDescription | null {
  const base = filePath.split('/').pop() ?? filePath
  const dotIdx = base.lastIndexOf('.')
  const ext = dotIdx >= 0 ? base.slice(dotIdx + 1).toLowerCase() : ''
  return (
    languages.find((l) => l.extensions?.includes(ext)) ??
    languages.find((l) => l.filename?.test(base)) ??
    null
  )
}

export function detectLanguageName(filePath: string): string | null {
  return findLanguageDescription(filePath)?.name ?? null
}

export async function detectLanguage(filePath: string): Promise<LanguageSupport | null> {
  const desc = findLanguageDescription(filePath)
  if (!desc) return null
  try {
    return await desc.load()
  } catch {
    return null
  }
}
